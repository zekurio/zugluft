//! zugluftctl — baseline fan-control tool.
//!
//! Lets you validate detection and manual fan control from a terminal before
//! (or without) the GUI. Some direct commands require an elevated terminal,
//! depending on what LibreHardwareMonitor needs for the hardware.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use zugluft_hw::{FanDuty, FanRegState, HwError, Session};

const USAGE: &str = "\
zugluftctl — zugluft fan-control baseline tool (run from an elevated terminal)

USAGE:
    zugluftctl status                  query the zugluft service (no admin)
    zugluftctl calibrate               measure fan RPM responses, stall and
                                       restart duties via the service
                                       (no admin, takes a few minutes)
    zugluftctl detect                  list detected chips, fans and temps
    zugluftctl watch [interval_ms]     continuously print fan readings
    zugluftctl set <fan> <percent> [--chip <n>]
                                       pin a fan to a fixed duty (0-100 %)
    zugluftctl auto <fan|all> [--chip <n>]
                                       hand a fan back to BIOS control
    zugluftctl report                  print hardware report details

`status` and `calibrate` talk to the zugluft service over its pipe. All other
commands talk to the hardware directly (elevated terminal required) — close
or stop the service first if it is actively driving fans.

The first `set` per fan saves the LHM control state to
%LOCALAPPDATA%\\zugluft so a later `auto` can restore it, even across
invocations.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("status") => return cmd_status(),
        Some("calibrate") => return cmd_calibrate(),
        _ => {}
    }
    let result = match args.first().map(String::as_str) {
        Some("detect") => cmd_detect(),
        Some("watch") => cmd_watch(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1000)),
        Some("set") => cmd_set(&args[1..]),
        Some("auto") => cmd_auto(&args[1..]),
        Some("report") => cmd_report(),
        _ => {
            print!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            if matches!(e, HwError::AccessDenied) {
                eprintln!("hint: start your terminal with \"Run as administrator\" and try again");
            }
            ExitCode::FAILURE
        }
    }
}

mod baseline;
mod direct;
mod service;

use baseline::Baseline;
use direct::*;
use service::*;
