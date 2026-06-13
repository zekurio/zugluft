# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

zugluft is a native Rust fan-control tool for Windows (alternative to FanControl). A privileged Windows service owns all hardware access; an unelevated GPUI desktop app and a CLI talk to it over named pipes. There are no tests and no CI — verification is building and running against real hardware.

## Build & run

```powershell
cargo build --release          # build everything
cargo check                    # fast validation
cargo clippy                   # lint

# Service lifecycle (needs elevation; install registers the exe's current path):
.\target\release\zugluft-service.exe install | stop | uninstall
.\target\release\zugluft-service.exe run-console   # foreground, for debugging

# Unelevated daily use:
.\target\release\zugluft.exe          # GUI
.\target\release\zugluftctl.exe status
```

Development gotchas:

- **The running service locks `target\release\zugluft-service.exe`** (it runs from its registered path). Stop the service before a release rebuild; close the GUI too (locks `zugluft.exe`). Claude Code shells are not elevated — use `Start-Process sc.exe -ArgumentList 'stop','zugluft' -Verb RunAs -Wait` to pop a UAC prompt the user approves.
- `zugluftctl detect/watch/set/auto/report` bypass the service and need an elevated terminal; `status` and `calibrate` go through the service pipe and don't.
- Building the LHM bridge needs the .NET SDK 8.0+; with only the runtime installed, `cargo check` still succeeds but runtime hardware detection needs a prebuilt `zugluft-lhm-bridge.dll` beside the executable, in `modules\`, in `%ProgramData%\zugluft\`, or via `ZUGLUFT_LHM_BRIDGE`.
- Live testing requires FanControl/SpeedFan closed — LHM handles low-level synchronization, but it cannot arbitrate duty ownership, so two tools writing different duties to the same fan fight each other. The service detects duty-write conflicts during calibration and aborts with a note.
- Service log: `C:\ProgramData\zugluft\service.log`. Calibration results: `C:\ProgramData\zugluft\calibration.json`. GUI config (display names, custom sensors): `%APPDATA%\zugluft\config.toml`.

## Architecture

Five workspace crates with a strict privilege split:

```
zugluft-app (GUI, unelevated)  ┐
zugluft-cli (zugluftctl)       ┴─ named pipes ─► zugluft-service (LocalSystem) ─► zugluft-hw ─► LibreHardwareMonitor
                                                          shared protocol types: zugluft-ipc
```

- **zugluft-hw** — hardware layer. It now uses LibreHardwareMonitorLib through a small NativeAOT bridge (`lhm-bridge/`, loaded dynamically as `zugluft-lhm-bridge.dll`). LHM owns device-specific motherboard, CPU, GPU, storage and controller support. Rust flattens LHM hardware nodes with relevant sensors into the existing `ChipInfo`/`ChipSnapshot` model, and exposes LHM `IControl.SetSoftware` / `SetDefault` as `set_fan` / `force_auto`. CPU/GPU/storage devices remain sensor-only unless LHM exposes control sensors for them. Raw EC register dumps are no longer available.
- **zugluft-ipc** — protocol (`Request`/`Event`/`ServiceState` enums) + pipe transport. Newline-delimited JSON over **two single-direction pipes** (`\\.\pipe\zugluft.events` service→client, `\\.\pipe\zugluft.control` client→service). Two pipes is deliberate: synchronous pipe handles serialize reads and writes on the same file object, so a blocking read would park every write forever. Don't merge them into one bidirectional pipe.
- **zugluft-service** — `worker.rs` is the single thread owning the hardware session: polls every 500 ms, coalesces duty writes (last-one-wins per fan during slider drags), retries detection every 30 s, runs calibration. `hub.rs` fans out state snapshots to all connected clients (new subscribers immediately get the latest state). `server.rs` runs the pipe accept loops; `main.rs` handles SCM integration and install/uninstall. Manual targets persist under ProgramData and re-apply after service restart; stop/uninstall still restores the LHM control state captured when the session opened until the service starts again.
- **zugluft-app** — GPUI app. `client.rs` runs two threads (events reader, control writer) around a `Shared` struct the UI polls by sequence number. `elevation.rs` does runtime UAC self-elevation via `ShellExecuteW("runas")` for the one-click service install.
- **zugluft-cli** — `status`/`calibrate` via the service; `detect/watch/set/auto/report` direct-to-hardware for development. Direct `set` saves the fan's pre-manual LHM control state to `%LOCALAPPDATA%\zugluft\fan-baseline.txt` so `auto` can restore it across invocations. `report` can no longer dump EC registers through the LHM backend.

State always flows one way: clients send `Request`s, the service publishes complete `ServiceState` snapshots (no deltas, no per-request replies). Custom sensors and fan curves are defined in the GUI's config.toml but evaluated by the service (defs pushed via `SetCustomSensors`/`SetCurves`, persisted under ProgramData), so they keep working with no GUI running. Per-fan curve assignments (`SetFanCurve`, `curve-assignments.json`) live service-side, keyed by chip identity; a `SetDuty` releases the fan's curve. Base curve evaluation (`CurveKind::evaluate`) lives in zugluft-ipc so the GUI editor preview and the service agree; the service then applies curve functions (`identity`, `standard` hysteresis, `ema`) before fan calibration maps speed percent to PWM. New curve kinds are new `CurveKind` variants.

## Key constraints

- **gpui is pinned to crates.io 0.2.2** — entry point is `gpui::Application::new().run(...)`. Zed's main branch uses an unpublished `gpui_platform` split; don't copy examples from zed main verbatim. gpui's default `windows-manifest` feature embeds a Windows manifest, so adding our own manifest (e.g. `requireAdministrator`) causes duplicate-resource link conflicts — that's why elevation is done at runtime instead.
- gpui renders SVG icons as an alpha mask tinted by `text_color`; the fill color in the asset file doesn't matter.
- Edition 2024. The service is std-threads + mpsc channels throughout — no async runtime; keep it that way.
- Safety: duty 0 % stops a fan dead (including CPU headers). Calibration code has temperature guards and restores previous duties; preserve those invariants when touching `worker.rs`/`calibration.rs`.
- Licensing: hardware support comes from LibreHardwareMonitor (MPL-2.0) through the bridge. Do not vendor PawnIO modules or copy LHM register maps back into handwritten Rust drivers unless the project deliberately reverses the LHM migration.

## Roadmap order (from README)

Fan curves in the service (done) → LHM backend (done) → better fan/control pairing and labels → tray icon + config persistence → per-board header names.
