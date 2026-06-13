//! Persisted manual fan targets.
//!
//! Manual targets are stored next to calibration, settings and curve
//! assignments so the service can publish and re-apply the user's last
//! manual mode after a restart or redetect. Stopping the service still drops
//! the hardware session first, so firmware control is restored until the
//! service starts again.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zugluft_ipc::ChipInfo;

use crate::calibration;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    /// Keyed by [`crate::calibration::chip_key`], then fan index.
    chips: HashMap<String, HashMap<usize, u8>>,
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

    pub fn set(&mut self, chip_key: &str, fan: usize, duty: Option<u8>) {
        match duty {
            Some(duty) => {
                self.chips
                    .entry(chip_key.to_string())
                    .or_default()
                    .insert(fan, duty);
            }
            None => {
                let remove_chip = if let Some(fans) = self.chips.get_mut(chip_key) {
                    fans.remove(&fan);
                    fans.is_empty()
                } else {
                    false
                };
                if remove_chip {
                    self.chips.remove(chip_key);
                }
            }
        }
    }

    pub fn targets_for_chips(
        &self,
        chips: &[ChipInfo],
        assignments: &[Vec<Option<String>>],
    ) -> HashMap<(usize, usize), u8> {
        let mut targets = HashMap::new();
        for (ci, info) in chips.iter().enumerate() {
            let key = calibration::chip_key(info);
            let Some(fans) = self.chips.get(&key) else {
                continue;
            };
            for fi in 0..info.control_count {
                if assignments
                    .get(ci)
                    .and_then(|fans| fans.get(fi))
                    .and_then(Option::as_ref)
                    .is_some()
                {
                    continue;
                }
                if let Some(&duty) = fans.get(&fi) {
                    targets.insert((ci, fi), duty);
                }
            }
        }
        targets
    }
}

fn path() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("zugluft")
        .join("manual-targets.json")
}
