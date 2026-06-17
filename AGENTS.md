# Repository Guidelines

## Project Structure & Module Organization

zugluft is a Rust workspace for a Windows fan-control system. Crates live under `crates/`: `zugluft-app` is the GPUI desktop app, `zugluft-cli` builds `zugluftctl`, `zugluft-service` owns privileged hardware control, `zugluft-hw` wraps LibreHardwareMonitor access, and `zugluft-ipc` defines named-pipe protocol types. App fonts, icons, and the tray icon are in `crates/zugluft-app/assets/`. Windows installer and release helpers live in `installer/`, `scripts/`, and `.github/workflows/`.

## Build, Test, and Development Commands

- `cargo check`: fast workspace type-check.
- `cargo build --release`: build optimized binaries in `target/release/`.
- `cargo clippy`: run Rust lints across the workspace.
- `cargo fmt`: format Rust sources with rustfmt.
- `cargo test`: run workspace tests when present.
- `.\target\release\zugluft-service.exe run-console`: run the service in console mode for local debugging.
- `.\target\release\zugluftctl.exe status`: query service status through IPC.
- `.\scripts\package-windows.ps1`: build the Windows release package; requires NSIS and, when rebuilding the bridge, .NET SDK 8.0+.

## Coding Style & Naming Conventions

Use Rust 2024 edition conventions and rustfmt defaults. Keep modules small and aligned with existing domains such as `ui`, `config`, `worker`, and `pipe`. Use `snake_case` for modules, functions, and variables; `PascalCase` for types; and descriptive enum variants for IPC and service state. Prefer explicit error propagation with `Result` over panics in service, hardware, and IPC code.

## Testing Guidelines

Add unit tests near the code they exercise using `#[cfg(test)]` modules, and use integration tests only for cross-crate or end-to-end behavior. Favor deterministic tests for curve math, calibration, IPC serialization, and configuration parsing. Run `cargo test` plus `cargo clippy` before opening a PR. For hardware-facing changes, document any manual validation performed with `zugluft-service.exe run-console` or `zugluftctl`.

## Commit & Pull Request Guidelines

Recent commits use short, imperative subjects such as `Prepare v0.2.1 release`, `refine readme`, and `Refactor app flows and update UI components`. Keep commit subjects concise and focused on one change. Pull requests should include a clear summary, testing notes, linked issues when applicable, and screenshots or screen recordings for visible app UI changes. For release changes, also mention installer/package validation and any version stamping performed.

## Security & Configuration Tips

Close other fan-control tools before testing; competing writers can fight over hardware duties. Be careful with privileged service behavior and paths under `C:\Program Files\zugluft` and `C:\ProgramData\zugluft`. Do not commit local machine configuration from `%APPDATA%\zugluft\config.toml`, service logs, calibration data, or generated `target/` artifacts.
