# zugluft

zugluft is a native Rust fan-control tool for Windows. A privileged Windows
service owns hardware access, while the unelevated desktop app and CLI talk to
it over named pipes.

Version: `0.1.0`

## What Works

- Windows service running as LocalSystem.
- GPUI desktop app for live fan control, fan curves, tuning and calibration.
- `zugluftctl status` through the service without elevation.
- Direct development commands through `zugluftctl detect`, `watch`, `set`,
  `auto` and `report`.
- LibreHardwareMonitor hardware support through the NativeAOT bridge.

Close other fan-control tools before using zugluft. Two tools writing fan
duties at once can fight over the same hardware.

## Install

Download `zugluft-setup-v0.1.0-windows-x64.exe` from the `v0.1.0` release and
run it.

The installer asks for UAC elevation, copies the GUI, CLI, service and
LibreHardwareMonitor bridge DLL to `C:\Program Files\zugluft`, runs the PawnIO
driver installer when PawnIO is not already present, then registers and starts
the Windows service.

After that, launch the app from the install directory:

```powershell
C:\Program Files\zugluft\zugluft.exe
```

The service registration stores the current path to `zugluft-service.exe`. If
you change the install directory, rerun the setup installer.

## Build

```powershell
cargo build --release
```

Building the LibreHardwareMonitor bridge from source requires the .NET SDK
8.0+. The release package includes the bridge DLL, so users do not need the
.NET runtime.

Useful commands:

```powershell
cargo check
cargo clippy
.\target\release\zugluft-service.exe run-console
.\target\release\zugluftctl.exe status
```

## Release

Stable releases are built by the **Release** GitHub Actions workflow from a
tag like `v0.1.0`, or by running it manually with `channel=stable` and
`version=0.1.0`.

- `v0.1.0`
- `zugluft-setup-v0.1.0-windows-x64.exe`
- `checksums.txt`

The setup executable installs an uninstaller at
`C:\Program Files\zugluft\uninstall.exe`. PawnIO is left installed on uninstall
because it is a shared hardware driver used by other tools.

Nightlies are built by the same **Release** workflow on its schedule or with
`channel=nightly`. They use the next minor after the latest stable tag, so
after `v0.1.0` the nightly line is:

```text
v0.2.0-nightly.YYYYMMDD.RUN
```

## Architecture

```text
zugluft-app (GUI)       \
zugluft-cli (zugluftctl) > named pipes > zugluft-service > zugluft-hw > LHM
zugluft-ipc (protocol) /
```

The IPC layer uses two single-direction named pipes:

- `\\.\pipe\zugluft.events` for service-to-client state snapshots.
- `\\.\pipe\zugluft.control` for client-to-service requests.

The service publishes complete state snapshots rather than per-request replies.
Custom sensors, fan curves and per-fan curve assignments are evaluated by the
service so control keeps working with no GUI running.

## Files

- Service log: `C:\ProgramData\zugluft\service.log`
- Calibration data: `C:\ProgramData\zugluft\calibration.json`
- GUI config: `%APPDATA%\zugluft\config.toml`

## License Notice

Hardware support comes from
[LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor),
licensed under MPL-2.0.
