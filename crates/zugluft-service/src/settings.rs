//! Persisted per-fan user tuning ([`FanSettings`]): ramp rates, start/stop
//! overrides, offset and minimum target. Stored next to the calibration
//! results under `%ProgramData%\zugluft`, keyed by chip identity so entries
//! survive service restarts and chip reordering.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zugluft_ipc::FanSettings;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    /// Keyed by [`crate::calibration::chip_key`], then fan index.
    chips: HashMap<String, HashMap<usize, FanSettings>>,
}

impl Store {
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Best-effort, like the service log.
    pub fn save(&self) {
        let path = path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }

    pub fn get(&self, chip_key: &str, fan: usize) -> FanSettings {
        self.chips
            .get(chip_key)
            .and_then(|fans| fans.get(&fan))
            .copied()
            .unwrap_or_default()
    }

    pub fn insert(&mut self, chip_key: &str, fan: usize, settings: FanSettings) {
        self.chips
            .entry(chip_key.to_string())
            .or_default()
            .insert(fan, settings);
    }
}

fn path() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("zugluft")
        .join("fan-settings.json")
}
