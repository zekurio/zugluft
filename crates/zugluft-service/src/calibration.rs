//! Persisted fan calibration results.
//!
//! A calibration run steps every controllable fan through fixed commands and
//! records the stabilized RPM at each step. The command→RPM curve lets the
//! service ask for a target speed and write the command that actually reaches
//! it; `max_rpm` is what percent readouts divide by. Stored under
//! `%ProgramData%\zugluft` so results survive service restarts, keyed by chip
//! identity rather than detection order.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zugluft_ipc::ChipInfo;

const FLAT_RPM_DELTA: f32 = 30.0;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FanCurve {
    /// RPM at full command; the reference for percent displays.
    pub max_rpm: f32,
    /// `(command 0..=255, stabilized rpm)` samples, highest command first.
    pub points: Vec<(u8, f32)>,
    /// Highest command at which the fan stood still while stepping down.
    /// `None` if it kept spinning all the way to 0 (or wasn't probed).
    #[serde(default)]
    pub stop_duty: Option<u8>,
    /// Lowest command that restarted the stopped fan; above `stop_duty`
    /// because of motor hysteresis.
    #[serde(default)]
    pub start_duty: Option<u8>,
}

impl FanCurve {
    /// Command duty for a target speed expressed as percent of `max_rpm`.
    pub fn command_for_speed_percent(&self, percent: f32) -> Option<u8> {
        if !percent.is_finite() || !self.max_rpm.is_finite() || self.max_rpm <= 0.0 {
            return None;
        }
        self.command_for_rpm(self.max_rpm * percent.clamp(0.0, 100.0) / 100.0)
    }

    /// Estimated speed percent for a command read back from hardware.
    pub fn speed_percent_for_command(&self, command: u8) -> Option<f32> {
        if !self.max_rpm.is_finite() || self.max_rpm <= 0.0 {
            return None;
        }
        self.rpm_for_command(command)
            .map(|rpm| (rpm * 100.0 / self.max_rpm).clamp(0.0, 100.0))
    }

    /// Lowest speed (%) this calibration can drive. Fans that stop have a
    /// 0 % floor; pumps and some headers may keep spinning at a nonzero
    /// speed even at the lowest hardware command.
    pub fn minimum_speed_percent(&self) -> Option<f32> {
        if !self.max_rpm.is_finite() || self.max_rpm <= 0.0 {
            return None;
        }
        self.usable_points()
            .first()
            .map(|&(_, rpm)| (rpm * 100.0 / self.max_rpm).clamp(0.0, 100.0))
    }

    fn command_for_rpm(&self, rpm: f32) -> Option<u8> {
        let points = self.usable_points();
        if points.len() < 2 || !rpm.is_finite() {
            return None;
        }

        let target = rpm.max(0.0);
        let (first, last) = (points[0], points[points.len() - 1]);
        if target <= first.1 {
            return Some(first.0);
        }
        if target >= last.1 {
            return Some(last.0);
        }

        let segment = points
            .windows(2)
            .find(|pair| pair[0].1 <= target && target <= pair[1].1)?;
        let (from, to) = (segment[0], segment[1]);
        let fraction = (target - from.1) / (to.1 - from.1).max(f32::EPSILON);
        Some(interpolate_duty(from.0, to.0, fraction))
    }

    fn rpm_for_command(&self, command: u8) -> Option<f32> {
        let points = self.usable_points();
        if points.len() < 2 {
            return None;
        }

        let (first, last) = (points[0], points[points.len() - 1]);
        if command <= first.0 {
            return Some(first.1);
        }
        if command >= last.0 {
            return Some(last.1);
        }

        let segment = points
            .windows(2)
            .find(|pair| pair[0].0 <= command && command <= pair[1].0)?;
        let (from, to) = (segment[0], segment[1]);
        let span = (to.0 as f32 - from.0 as f32).max(f32::EPSILON);
        let fraction = (command as f32 - from.0 as f32) / span;
        Some(from.1 + (to.1 - from.1) * fraction)
    }

    fn usable_points(&self) -> Vec<(u8, f32)> {
        let mut points: Vec<(u8, f32)> = self
            .points
            .iter()
            .copied()
            .filter(|&(_, rpm)| rpm.is_finite() && rpm >= 0.0)
            .collect();
        points.sort_by_key(|&(duty, _)| duty);

        let mut usable: Vec<(u8, f32)> = Vec::new();
        for (duty, rpm) in points {
            match usable.last_mut() {
                Some(last) if rpm <= last.1 + FLAT_RPM_DELTA => {
                    // Treat flat/dead command regions like FanControl's
                    // avoided points: keep the upper edge of the plateau so
                    // inverse lookups do not request lower no-op commands.
                    *last = (duty, rpm);
                }
                _ => usable.push((duty, rpm)),
            }
        }
        usable
    }
}

fn interpolate_duty(from: u8, to: u8, fraction: f32) -> u8 {
    (from as f32 + (to as f32 - from as f32) * fraction)
        .round()
        .clamp(0.0, 255.0) as u8
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    /// Keyed by [`chip_key`], then fan index.
    chips: HashMap<String, HashMap<usize, FanCurve>>,
}

/// Identity that survives re-detection and chip reordering.
pub fn chip_key(info: &ChipInfo) -> String {
    format!("{}@{:04X}/{}", info.name, info.address, info.slot)
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

    pub fn curve(&self, chip_key: &str, fan: usize) -> Option<&FanCurve> {
        self.chips.get(chip_key)?.get(&fan)
    }

    pub fn insert(&mut self, chip_key: &str, fan: usize, curve: FanCurve) {
        self.chips
            .entry(chip_key.to_string())
            .or_default()
            .insert(fan, curve);
    }

    pub fn remove(&mut self, chip_key: &str, fan: usize) {
        if let Some(fans) = self.chips.get_mut(chip_key) {
            fans.remove(&fan);
        }
    }
}

fn path() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("zugluft")
        .join("calibration.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(points: Vec<(u8, f32)>) -> FanCurve {
        FanCurve {
            max_rpm: points.iter().map(|&(_, rpm)| rpm).fold(0.0, f32::max),
            points,
            stop_duty: None,
            start_duty: None,
        }
    }

    #[test]
    fn maps_speed_percent_to_command() {
        let curve = curve(vec![(255, 2000.0), (128, 1000.0), (0, 0.0)]);

        assert_eq!(curve.command_for_speed_percent(0.0), Some(0));
        assert_eq!(curve.command_for_speed_percent(50.0), Some(128));
        assert_eq!(curve.command_for_speed_percent(100.0), Some(255));
    }

    #[test]
    fn interpolates_between_measured_points() {
        let curve = curve(vec![(255, 2000.0), (128, 1000.0), (0, 0.0)]);

        assert_eq!(curve.command_for_speed_percent(25.0), Some(64));
        assert_eq!(curve.command_for_speed_percent(75.0), Some(192));
    }

    #[test]
    fn skips_flat_avoid_region() {
        let curve = curve(vec![
            (255, 2240.0),
            (102, 1867.0),
            (26, 1494.0),
            (3, 1117.0),
            (0, 1117.0),
        ]);

        assert_eq!(curve.command_for_speed_percent(0.0), Some(3));
        let speed = curve.speed_percent_for_command(0).unwrap();
        assert!((speed - 49.866).abs() < 0.01);
    }

    #[test]
    fn reports_minimum_achievable_speed() {
        let curve = curve(vec![(255, 4200.0), (51, 1860.0), (0, 1120.0)]);

        let min = curve.minimum_speed_percent().unwrap();
        assert!((min - 26.666).abs() < 0.01);
    }

    #[test]
    fn stopped_fans_have_zero_minimum_speed() {
        let curve = curve(vec![(255, 1800.0), (13, 130.0), (0, 0.0)]);

        assert_eq!(curve.minimum_speed_percent(), Some(0.0));
    }

    #[test]
    fn falls_back_when_calibration_has_no_range() {
        let curve = curve(vec![(255, 1200.0), (0, 1200.0)]);

        assert_eq!(curve.command_for_speed_percent(50.0), None);
    }
}
