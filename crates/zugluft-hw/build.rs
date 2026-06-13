use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bridge_dir = manifest.join("lhm-bridge");
    println!("cargo:rerun-if-changed={}", bridge_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        bridge_dir.join("zugluft-lhm-bridge.csproj").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        bridge_dir.join("src").join("Bridge.cs").display()
    );
    println!("cargo:rerun-if-env-changed=ZUGLUFT_SKIP_LHM_BRIDGE_BUILD");
    println!("cargo:rerun-if-env-changed=ZUGLUFT_REQUIRE_LHM_BRIDGE");

    if env::var_os("ZUGLUFT_SKIP_LHM_BRIDGE_BUILD").is_some() {
        println!("cargo:warning=skipping LibreHardwareMonitor bridge build by request");
        return;
    }

    if !has_dotnet_sdk() {
        println!(
            "cargo:warning=.NET SDK not found; zugluft-hw will compile, but runtime needs {} from a bridge build",
            bridge_dll_name()
        );
        return;
    }

    let out_dir = Path::new(&env::var("OUT_DIR").expect("OUT_DIR is set")).to_path_buf();
    let publish_dir = out_dir.join("lhm-bridge");
    let intermediate_dir = out_dir.join("lhm-bridge-build");
    let nuget_dir = out_dir.join(".nuget");

    let mut command = Command::new("dotnet");
    command
        .arg("publish")
        .arg(bridge_dir.join("zugluft-lhm-bridge.csproj"))
        .arg("-c")
        .arg("Release")
        .arg("-r")
        .arg("win-x64")
        .arg("-o")
        .arg(&publish_dir)
        .arg("/p:NativeLib=Shared")
        .arg(format!("/p:OutputPath={}/", publish_dir.display()))
        .arg(format!(
            "/p:BaseIntermediateOutputPath={}/",
            intermediate_dir.display()
        ))
        .env("NUGET_PACKAGES", &nuget_dir);

    match command.output() {
        Ok(output) if output.status.success() => {
            let dll = publish_dir.join(bridge_dll_name());
            println!("cargo:rustc-env=ZUGLUFT_BUILT_LHM_BRIDGE={}", dll.display());
        }
        Ok(output) => {
            warn_output(
                ".NET failed to build the LibreHardwareMonitor bridge",
                &output,
            );
            require_or_continue();
        }
        Err(error) => {
            println!("cargo:warning=failed to run dotnet publish: {error}");
            require_or_continue();
        }
    }
}

fn has_dotnet_sdk() -> bool {
    Command::new("dotnet")
        .arg("--list-sdks")
        .output()
        .map(|output| output.status.success() && !output.stdout.is_empty())
        .unwrap_or(false)
}

fn warn_output(label: &str, output: &std::process::Output) {
    println!("cargo:warning={label}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines().take(20) {
        println!("cargo:warning={line}");
    }
}

fn require_or_continue() {
    if env::var_os("ZUGLUFT_REQUIRE_LHM_BRIDGE").is_some() {
        panic!("LibreHardwareMonitor bridge build failed");
    }
}

fn bridge_dll_name() -> &'static str {
    "zugluft-lhm-bridge.dll"
}
