//! User-defined derived sensors (average / min / max over temperature
//! channels).
//!
//! The definitions are owned by the GUI's config.toml and pushed over IPC;
//! the service keeps its own copy under `%ProgramData%\zugluft` so the
//! sensors keep working (and can later drive curves) across service
//! restarts with no GUI running.

use std::path::PathBuf;

use zugluft_ipc::{ChipInfo, ChipSnapshot, CustomSensorDef, CustomSensorValue};

pub fn load() -> Vec<CustomSensorDef> {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Best-effort, like the service log.
pub fn save(defs: &[CustomSensorDef]) {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(defs) {
        let _ = std::fs::write(path, text);
    }
}

/// Evaluates every definition against the current snapshots. Unavailable
/// inputs are skipped; a sensor with no available input reads `None`.
pub fn compute(
    defs: &[CustomSensorDef],
    chips: &[ChipInfo],
    snapshots: &[ChipSnapshot],
) -> Vec<CustomSensorValue> {
    defs.iter()
        .map(|def| CustomSensorValue {
            id: def.id.clone(),
            name: def.name.clone(),
            value: def.evaluate(chips, snapshots),
        })
        .collect()
}

fn path() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("zugluft")
        .join("custom.json")
}
