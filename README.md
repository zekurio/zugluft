# zugluft

Fast, simple fan control for Windows. A native Rust alternative to FanControl:
a small privileged service owns the hardware, the
[GPUI](https://www.gpui.rs/)-rendered app just talks to it, and
LibreHardwareMonitor does the device-specific hardware work.

> *Zugluft* (German): the draft of air you feel when two windows are open.

## Status: V0.5

What works today:

- **`zugluft-service`** — a Windows service (LocalSystem, auto-start) that owns
  all hardware access and serves clients over named pipes. Installing it is
  the only step that ever needs elevation. Stopping it hands every fan back
  to the BIOS.
- **Hardware layer** (`zugluft-hw`): uses
  [LibreHardwareMonitorLib](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)
  through a small NativeAOT bridge. LHM handles motherboard, CPU, GPU, storage
  and controller sensor/control support; zugluft keeps the service, curves,
  calibration and safety policy.
- **`zugluft`** — unelevated GPUI app: one row per fan with live RPM and a
  click/drag target slider, auto/manual/curve switching, temperature readouts
  and graph-based fan curves with function tuning. Offers one-click (one UAC
  prompt) service installation when the service isn't running yet.
- **`zugluftctl`** — CLI: `status` (asks the service, no admin needed), plus
  direct-hardware commands for development: `detect`, `watch`,
  `set <fan> <pct>`, `auto <fan|all>`, `report`.

Not yet: richer control/fan pairing metadata, per-board fan-header names.

## Architecture

```
┌────────────────┐   \\.\pipe\zugluft.events    ┌──────────────────────┐
│  zugluft (GUI) │ ◄─────────── state ───────── │   zugluft-service    │
│   unelevated   │ ──────── requests ─────────► │ (Windows service,    │
└────────────────┘   \\.\pipe\zugluft.control   │  LocalSystem)        │
┌────────────────┐                              │  ┌────────────────┐  │
│   zugluftctl   │ ◄──── status (events) ────── │  │   zugluft-hw   │  │
└────────────────┘                              │  │      LHM       │  │
                                                └──┴────────────────┴──┘
```

- Transport: newline-delimited JSON over two single-direction named pipes.
  (Two pipes because synchronous pipe handles serialize reads and writes on
  the same instance — a blocking read would park every write forever.)
- Pipe ACL: SYSTEM and Administrators get full access, interactive (logged-on
  desktop) users get connect. Network logons get nothing.
- The service polls the chip every 500 ms, coalesces fan writes
  (last-one-wins per fan during slider drags), and keeps manual/curve settings
  while it runs — closing the GUI changes nothing. Service stop/uninstall
  restores BIOS control.

```
crates/
├── zugluft-hw       hardware access (LibreHardwareMonitor NativeAOT bridge)
├── zugluft-ipc      protocol types + named-pipe transport
├── zugluft-service  Windows service: worker, pipe servers, SCM install/uninstall
├── zugluft-cli      zugluftctl — service status + direct-hardware dev tool
└── zugluft-app      GPUI app (pipe client + UI)
```

## Requirements

- Windows.
- The .NET SDK 8.0+ when building `zugluft-hw`'s LHM bridge from source. If
  the SDK is absent, Rust still builds, but runtime hardware detection needs
  `zugluft-lhm-bridge.dll` beside the executable, in `modules\`, in
  `%ProgramData%\zugluft\`, or pointed to by `ZUGLUFT_LHM_BRIDGE`.
- LHM's own low-level requirements still apply for your hardware. Recent LHM
  uses PawnIO for privileged motherboard access, so machines that need it must
  have the relevant driver/module setup available.
- **Close other fan-control tools first.** Multiple programs writing different
  duties to the same fan can still fight each other even when the low-level
  access itself is synchronized.

## Build & run

```powershell
cargo build --release

# One-time service setup (elevated terminal — or just click the button the
# app shows on first start):
.\target\release\zugluft-service.exe install

# Daily use — no elevation:
.\target\release\zugluft.exe
.\target\release\zugluftctl.exe status

# Service management (elevated):
.\target\release\zugluft-service.exe stop        # fans back to BIOS
.\target\release\zugluft-service.exe uninstall
```

The service registers its own executable path — if you move or rebuild the
binary to a new location, run `install` again. Service log:
`C:\ProgramData\zugluft\service.log`.

## Release automation

GitHub Actions builds Windows x64 packages containing `zugluft.exe`,
`zugluft-service.exe`, `zugluftctl.exe`, `zugluft-lhm-bridge.dll`, install
notes and third-party notices.

- Full release: run the **Release** workflow manually with a SemVer input such
  as `0.5.0`. It publishes `v0.5.0` and
  `zugluft-v0.5.0-windows-x64.zip`.
- Nightly release: the **Nightly** workflow runs daily and can also be run
  manually. It finds the latest stable tag `vX.Y.Z`, bumps the patch, and
  publishes a prerelease like `vX.Y.(Z+1)-nightly.YYYYMMDD.RUN`.

### Direct-hardware mode (development)

`zugluftctl detect/watch/set/auto/report` bypass the service and talk to LHM
directly; they may need an elevated terminal depending on the hardware. `set`
records the fan's pre-manual LHM control state in
`%LOCALAPPDATA%\zugluft\fan-baseline.txt`, so `auto` can restore it from a
different invocation. Raw EC register dumps are not available through the LHM
backend.

## Safety notes

- Manual targets are remembered by the service and re-applied after restart;
  stopping the service still restores firmware control until it starts again.
- Setting 0 % on a CPU fan header means the fan stops. The hardware's thermal
  shutdown is your last line of defense; don't test that line.

## Roadmap

1. ~~**Fan curves**~~ — done: temp source → graph curve → function pipeline
   (`identity`, `standard` hysteresis, or `ema`) → target fan %, then calibrated
   fans map that target through their measured command→RPM graph and the
   service writes PWM. Uncalibrated fans fall back to raw PWM percent. Curves
   are evaluated in the service so fans stay controlled from boot, before
   login, GUI closed. Edited in the Curves tab or as `[[curve]]` entries in
   config.toml. Curve kinds are `graph` (editable points, clamped ends),
   `trigger` (threshold with instant switch or ramp), and `linear` (two points,
   extrapolated beyond them); more kinds slot in as new `CurveKind` variants.
2. Improve LHM fan/control pairing and labels.
3. ~~Tray icon~~, config persistence in `%ProgramData%\zugluft`.
4. Per-board fan-header naming.

## Licenses & credit

Hardware support comes from
[LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)
(MPL-2.0). The bridge loads LHM as a NuGet dependency at build time; zugluft
does not vendor PawnIO modules or hardware register maps.
