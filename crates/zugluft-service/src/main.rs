//! zugluft-service — the privileged half of zugluft.
//!
//! Runs as a Windows service (LocalSystem), owns the LibreHardwareMonitor
//! hardware session, and serves fan state and control over `\\.\pipe\zugluft`.
//! Installing the service is the only step that ever needs elevation:
//!
//! ```text
//! zugluft-service install      register + start (elevated terminal or UAC)
//! zugluft-service uninstall    stop + remove
//! zugluft-service run-console  run in the foreground for debugging
//! ```

mod calibration;
mod curves;
mod custom;
mod hub;
mod manual;
mod server;
mod settings;
mod worker;

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
    ServiceInfo, ServiceStartType, ServiceState as ScmState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, service_dispatcher};

use crate::hub::Hub;
use crate::worker::Command;

const SERVICE_NAME: &str = "zugluft";
const SERVICE_DISPLAY_NAME: &str = "zugluft fan control";
const SERVICE_DESCRIPTION: &str = "Drives hardware fans through LibreHardwareMonitor for the zugluft app. \
     Stopping the service returns all fans to BIOS control.";

const USAGE: &str = "\
zugluft-service — privileged hardware service for zugluft

USAGE (elevated terminal):
    zugluft-service install      register as a Windows service and start it
    zugluft-service uninstall    stop and remove the service
    zugluft-service start        start the installed service
    zugluft-service stop         stop the service (restores BIOS fan control)
    zugluft-service run-console  run in the foreground (debugging; Ctrl+C
                                 does NOT restore BIOS control)

Without arguments the binary expects to be launched by the service control
manager.
";

fn main() -> ExitCode {
    let arg = std::env::args().nth(1);
    let result = match arg.as_deref() {
        Some("install") => cmd_install(),
        Some("uninstall") => cmd_uninstall(),
        Some("start") => cmd_start(),
        Some("stop") => cmd_stop(),
        Some("run-console") => {
            run_console();
            Ok(())
        }
        Some(_) => {
            print!("{USAGE}");
            return ExitCode::from(2);
        }
        None => {
            // Launched by the SCM (or by hand without args).
            return match service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => {
                    print!("{USAGE}");
                    ExitCode::from(2)
                }
            };
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            if error.raw_os_error() == Some(5) {
                eprintln!("hint: this command needs an elevated terminal (Run as administrator)");
            }
            ExitCode::FAILURE
        }
    }
}

define_windows_service!(ffi_service_main, service_main);

fn service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_service() {
        log_line(&format!("service error: {error:?}"));
    }
}

fn run_service() -> windows_service::Result<()> {
    let (tx, rx) = channel();

    let handler_tx = tx.clone();
    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = handler_tx.send(Command::Shutdown);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;

    let running = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ScmState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::ZERO,
        process_id: None,
    };
    status_handle.set_service_status(running.clone())?;
    log_line("service started");

    run_core(&tx, &rx);

    status_handle.set_service_status(ServiceStatus {
        current_state: ScmState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        ..running
    })?;
    log_line("service stopped");
    Ok(())
}

/// Shared by service and console modes: pipe listener + hardware worker.
/// Blocks until the worker shuts down.
fn run_core(tx: &Sender<Command>, rx: &Receiver<Command>) {
    let hub = Arc::new(Hub::new());
    server::spawn_listeners(hub.clone(), tx.clone());
    worker::run(&hub, rx);
}

fn run_console() {
    println!(
        "zugluft-service running in console mode; clients connect on {} / {}",
        zugluft_ipc::EVENTS_PIPE,
        zugluft_ipc::CONTROL_PIPE
    );
    println!("note: Ctrl+C kills the process without restoring BIOS fan control");
    let (tx, rx) = channel();
    run_core(&tx, &rx);
}

fn cmd_install() -> std::io::Result<()> {
    let manager =
        service_manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    let executable_path = std::env::current_exe()?;

    let info = ServiceInfo {
        name: SERVICE_NAME.into(),
        display_name: SERVICE_DISPLAY_NAME.into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path,
        launch_arguments: vec![],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };

    let access = ServiceAccess::CHANGE_CONFIG | ServiceAccess::START;
    let service = match manager.create_service(&info, access) {
        Ok(service) => {
            println!(
                "service '{SERVICE_NAME}' installed ({})",
                info.executable_path.display()
            );
            service
        }
        // Most likely it already exists; open it instead.
        Err(_) => manager.open_service(SERVICE_NAME, access).map_err(to_io)?,
    };
    let _ = service.set_description(SERVICE_DESCRIPTION);

    match service.start::<&std::ffi::OsStr>(&[]) {
        Ok(()) => println!("service started"),
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(1056) => {
            println!("service is already running");
        }
        Err(e) => return Err(to_io(e)),
    }
    Ok(())
}

fn cmd_uninstall() -> std::io::Result<()> {
    let manager = service_manager(ServiceManagerAccess::CONNECT)?;
    let access = ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS;
    let service = manager.open_service(SERVICE_NAME, access).map_err(to_io)?;

    if service.query_status().map_err(to_io)?.current_state != ScmState::Stopped {
        let _ = service.stop();
        for _ in 0..50 {
            if service.query_status().map_err(to_io)?.current_state == ScmState::Stopped {
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }
        println!("service stopped (fans back under BIOS control)");
    }
    service.delete().map_err(to_io)?;
    println!("service '{SERVICE_NAME}' removed");
    Ok(())
}

fn cmd_start() -> std::io::Result<()> {
    let manager = service_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager
        .open_service(SERVICE_NAME, ServiceAccess::START)
        .map_err(to_io)?;
    service.start::<&std::ffi::OsStr>(&[]).map_err(to_io)?;
    println!("service started");
    Ok(())
}

fn cmd_stop() -> std::io::Result<()> {
    let manager = service_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager
        .open_service(SERVICE_NAME, ServiceAccess::STOP)
        .map_err(to_io)?;
    service.stop().map_err(to_io)?;
    println!("service stopping (fans back under BIOS control)");
    Ok(())
}

fn service_manager(access: ServiceManagerAccess) -> std::io::Result<ServiceManager> {
    ServiceManager::local_computer(None::<&str>, access).map_err(to_io)
}

fn to_io(error: windows_service::Error) -> std::io::Error {
    match error {
        windows_service::Error::Winapi(e) => e,
        other => std::io::Error::other(format!("{other:?}")),
    }
}

/// Minimal file log; services have no console. Best-effort by design.
pub fn log_line(message: &str) {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "[{timestamp}] {message}");
    }
    // Console mode: mirror to stdout.
    println!("{message}");
}

fn log_path() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("zugluft")
        .join("service.log")
}
