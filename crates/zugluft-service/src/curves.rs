//! Fan curves: persisted curve definitions plus per-fan assignments.
//!
//! Definitions are owned by the GUI's config.toml and pushed over IPC
//! (like custom sensors); assignments are set per fan from the UI. Both
//! are persisted under `%ProgramData%\zugluft` so curve control keeps
//! working across service restarts with no GUI running. Evaluation lives
//! in `zugluft_ipc` so the GUI's editor preview and the service always
//! compute the same target.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zugluft_ipc::{ChipInfo, ChipSnapshot, CurveDef, CurveStatus, CustomSensorValue};

pub fn load_defs() -> Vec<CurveDef> {
    let mut defs: Vec<CurveDef> = std::fs::read_to_string(defs_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    for def in &mut defs {
        def.normalize_functions();
        def.normalize_window();
        def.normalize_kind();
    }
    defs
}

/// Best-effort, like the service log.
pub fn save_defs(defs: &[CurveDef]) {
    let path = defs_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(defs) {
        let _ = std::fs::write(path, text);
    }
}

/// Evaluates every definition against the current snapshots, for clients.
pub fn statuses(
    defs: &[CurveDef],
    chips: &[ChipInfo],
    snapshots: &[ChipSnapshot],
    customs: &[CustomSensorValue],
) -> Vec<CurveStatus> {
    defs.iter()
        .map(|def| {
            let input = def.source.resolve(chips, snapshots, customs);
            CurveStatus {
                id: def.id.clone(),
                name: def.name.clone(),
                input,
                output: input.and_then(|input| def.kind.evaluate(input)),
            }
        })
        .collect()
}

/// Which curve drives which fan, keyed by chip identity so entries
/// survive service restarts and chip reordering (like fan settings).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Assignments {
    /// Keyed by [`crate::calibration::chip_key`], then fan index → curve id.
    chips: HashMap<String, HashMap<usize, String>>,
}

impl Assignments {
    pub fn load() -> Self {
        std::fs::read_to_string(assignments_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Best-effort, like the service log.
    pub fn save(&self) {
        let path = assignments_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }

    pub fn get(&self, chip_key: &str, fan: usize) -> Option<String> {
        self.chips.get(chip_key)?.get(&fan).cloned()
    }

    pub fn set(&mut self, chip_key: &str, fan: usize, curve: Option<&str>) {
        match curve {
            Some(id) => {
                self.chips
                    .entry(chip_key.to_string())
                    .or_default()
                    .insert(fan, id.to_string());
            }
            None => {
                if let Some(fans) = self.chips.get_mut(chip_key) {
                    fans.remove(&fan);
                }
            }
        }
    }
}

fn defs_path() -> PathBuf {
    data_dir().join("curves.json")
}

fn assignments_path() -> PathBuf {
    data_dir().join("curve-assignments.json")
}

fn data_dir() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("zugluft")
}
