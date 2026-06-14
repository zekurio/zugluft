//! User-editable display names and custom sensor definitions.
//!
//! Lives at `%APPDATA%\zugluft\config.toml`, keyed by chip name so entries
//! survive chips being re-ordered between detections:
//!
//! ```toml
//! [chips."ITE IT8688E"]
//! temp1 = "CPU"
//! fan1 = "CPU Fan"
//!
//! [[custom]]
//! id = "mix"
//! name = "CPU/System Mix"
//! kind = "average" # average | min | max
//! inputs = [
//!     { chip = "ITE IT8688E", temp = 1, weight = 2.0 },
//!     { chip = "ITE IT8688E", temp = 2 },
//! ]
//! ```
//!
//! On first run (once hardware is detected) a template with every detected
//! channel commented out is written, so renaming is just uncomment-and-edit.
//! The app reloads the file automatically when it changes on disk and
//! pushes custom sensor definitions to the service, which evaluates them.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::Deserialize;
use zugluft_ipc::{
    ChipInfo, ChipSnapshot, CurveDef, CurveFunction, CurveKind, CurveSource, CustomKind,
    CustomSensorDef,
};

mod store;
mod window;

pub use store::*;
pub use window::*;

const HIDDEN_DEVICE_KEY: &str = "device";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HiddenCategory {
    Fans,
    Temperatures,
    Power,
}

impl HiddenCategory {
    pub fn key(self) -> &'static str {
        match self {
            Self::Fans => "fans",
            Self::Temperatures => "temperatures",
            Self::Power => "power",
        }
    }

    fn from_channel_key(key: &str) -> Option<Self> {
        let (prefix, number) = key
            .char_indices()
            .find(|(_, ch)| ch.is_ascii_digit())
            .map(|(index, _)| key.split_at(index))?;
        if number.parse::<usize>().ok()? == 0 {
            return None;
        }
        match prefix {
            "fan" => Some(Self::Fans),
            "temp" => Some(Self::Temperatures),
            "power" => Some(Self::Power),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TempUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

impl TempUnit {
    fn as_str(self) -> &'static str {
        match self {
            Self::Celsius => "celsius",
            Self::Fahrenheit => "fahrenheit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FanUnit {
    #[default]
    Rpm,
    /// Percent of the fan's calibrated maximum RPM (until calibrated: of
    /// the highest RPM seen this session) — the chip does not report live
    /// duty for fans in auto mode.
    Percent,
}

impl FanUnit {
    fn as_str(self) -> &'static str {
        match self {
            Self::Rpm => "rpm",
            Self::Percent => "percent",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct UnitsConfig {
    temperature: Option<TempUnit>,
    fan: Option<FanUnit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DashboardItemKind {
    Fan,
    Sensor,
    Curve,
}

impl DashboardItemKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fan => "fan",
            Self::Sensor => "sensor",
            Self::Curve => "curve",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DashboardItem {
    kind: DashboardItemKind,
    #[serde(default)]
    chip: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

impl DashboardItem {
    pub fn fan(chip: impl Into<String>, channel: impl Into<String>) -> Self {
        Self {
            kind: DashboardItemKind::Fan,
            chip: Some(chip.into()),
            channel: Some(channel.into()),
            id: None,
        }
    }

    pub fn sensor(chip: impl Into<String>, channel: impl Into<String>) -> Self {
        Self {
            kind: DashboardItemKind::Sensor,
            chip: Some(chip.into()),
            channel: Some(channel.into()),
            id: None,
        }
    }

    pub fn kind(&self) -> DashboardItemKind {
        self.kind
    }

    pub fn chip(&self) -> Option<&str> {
        self.chip.as_deref()
    }

    pub fn channel(&self) -> Option<&str> {
        self.channel.as_deref()
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn is_valid(&self) -> bool {
        let has_text =
            |value: &Option<String>| value.as_ref().is_some_and(|v| !v.trim().is_empty());
        match self.kind {
            DashboardItemKind::Fan | DashboardItemKind::Sensor => {
                has_text(&self.chip) && has_text(&self.channel)
            }
            DashboardItemKind::Curve => has_text(&self.id),
        }
    }

    fn matches(&self, other: &Self) -> bool {
        self.kind == other.kind
            && match self.kind {
                DashboardItemKind::Fan | DashboardItemKind::Sensor => {
                    self.chip() == other.chip() && self.channel() == other.channel()
                }
                DashboardItemKind::Curve => self.id() == other.id(),
            }
    }
}

#[derive(Debug, Default, Deserialize)]
struct DashboardConfig {
    #[serde(default)]
    item: Vec<DashboardItem>,
}

#[derive(Debug, Default, Deserialize)]
pub struct NamesConfig {
    #[serde(default)]
    units: UnitsConfig,
    #[serde(default)]
    dashboard: DashboardConfig,
    #[serde(default)]
    chips: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    custom: Vec<CustomSensorDef>,
    #[serde(default)]
    curve: Vec<CurveDef>,
    /// Devices, categories, or channels hidden from the UI, keyed by chip
    /// name. Values are `device`, category keys (`fans`/`temperatures`/
    /// `power`), or `fanN`/`tempN`/`powerN` keys like display-name
    /// overrides. Hiding is display-only — hidden channels keep polling
    /// and stay valid as curve sources.
    #[serde(default)]
    hidden: HashMap<String, Vec<String>>,
    /// Per-channel graph line color overrides (`"#rrggbb"`), keyed by chip
    /// name (or custom-sensor id) then channel key (`tempN`/`fanN`/
    /// `powerN`/`custom`).
    #[serde(default)]
    graph_color: HashMap<String, HashMap<String, String>>,
    /// Per-channel graph line style overrides
    /// (`solid`/`dashed`/`dotted`/`dashdot`), keyed like `graph_color`.
    #[serde(default)]
    graph_style: HashMap<String, HashMap<String, String>>,
    /// Per-channel graph visibility overrides; absent means the kind's
    /// default (everything but fans is shown). Keyed like `graph_color`.
    #[serde(default)]
    graph_shown: HashMap<String, HashMap<String, bool>>,
    /// Last curve picked for a fan, keyed by chip name then `fanN`. This is
    /// separate from the service's active assignment so auto/manual can
    /// temporarily release a fan while curve mode still resumes the same
    /// curve later.
    #[serde(default)]
    fan_curve: HashMap<String, HashMap<String, String>>,
}

impl NamesConfig {
    pub fn temp_unit(&self) -> TempUnit {
        self.units.temperature.unwrap_or_default()
    }

    pub fn fan_unit(&self) -> FanUnit {
        self.units.fan.unwrap_or_default()
    }

    /// Display name for a temp channel: user override → chip-provided
    /// default (CPU/GPU sensors carry their own) → "Temp N".
    pub fn temp_label(&self, chip: &str, index: usize, defaults: &[String]) -> String {
        self.lookup(chip, &format!("temp{}", index + 1))
            .or_else(|| defaults.get(index).cloned())
            .unwrap_or_else(|| format!("Temp {}", index + 1))
    }

    pub fn power_label(&self, chip: &str, index: usize, defaults: &[String]) -> String {
        self.lookup(chip, &format!("power{}", index + 1))
            .or_else(|| defaults.get(index).cloned())
            .unwrap_or_else(|| format!("Power {}", index + 1))
    }

    pub fn fan_label(&self, chip: &str, index: usize) -> String {
        self.lookup(chip, &format!("fan{}", index + 1))
            .unwrap_or_else(|| format!("Fan {}", index + 1))
    }

    pub fn device_label(&self, chip: &str) -> String {
        self.lookup(chip, "name")
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| chip.to_string())
    }

    /// Custom sensor definitions, ids and names filled in.
    pub fn customs(&self) -> &[CustomSensorDef] {
        &self.custom
    }

    /// Fan curve definitions, ids and names filled in.
    pub fn curves(&self) -> &[CurveDef] {
        &self.curve
    }

    /// Dashboard pins, in display order.
    pub fn dashboard_items(&self) -> &[DashboardItem] {
        &self.dashboard.item
    }

    pub fn is_dashboard_pinned(&self, item: &DashboardItem) -> bool {
        self.dashboard
            .item
            .iter()
            .any(|pinned| pinned.matches(item))
    }

    fn has_hidden_key(&self, chip: &str, key: &str) -> bool {
        self.hidden
            .get(chip)
            .is_some_and(|keys| keys.iter().any(|hidden| hidden == key))
    }

    /// Whether an entire chip/device is hidden from the UI.
    pub fn is_device_hidden(&self, chip: &str) -> bool {
        self.has_hidden_key(chip, HIDDEN_DEVICE_KEY)
    }

    /// Whether an entire sensor category on a chip is hidden from the UI.
    pub fn is_category_hidden(&self, chip: &str, category: HiddenCategory) -> bool {
        self.has_hidden_key(chip, category.key())
    }

    /// Whether a channel (`fanN`/`tempN`/`powerN`) is directly hidden from
    /// the UI, ignoring device/category hides.
    pub fn is_channel_hidden(&self, chip: &str, key: &str) -> bool {
        self.has_hidden_key(chip, key)
    }

    /// Whether a channel is effectively hidden from the UI by its own
    /// toggle, its category, or its whole device.
    pub fn is_hidden(&self, chip: &str, key: &str) -> bool {
        self.is_device_hidden(chip)
            || HiddenCategory::from_channel_key(key)
                .is_some_and(|category| self.is_category_hidden(chip, category))
            || self.is_channel_hidden(chip, key)
    }

    /// User color override for a graph line, parsed from `"#rrggbb"`.
    pub fn graph_color(&self, chip: &str, key: &str) -> Option<u32> {
        let raw = self.graph_color.get(chip)?.get(key)?;
        u32::from_str_radix(raw.strip_prefix('#').unwrap_or(raw), 16).ok()
    }

    /// User line-style override name for a graph line.
    pub fn graph_style(&self, chip: &str, key: &str) -> Option<&str> {
        self.graph_style.get(chip)?.get(key).map(String::as_str)
    }

    /// User visibility override for a graph line; `None` means "use the
    /// kind's default".
    pub fn graph_shown(&self, chip: &str, key: &str) -> Option<bool> {
        self.graph_shown.get(chip)?.get(key).copied()
    }

    /// Last curve selected for one fan, if the user has picked one before.
    pub fn fan_curve(&self, chip: &str, index: usize) -> Option<String> {
        self.fan_curve
            .get(chip)?
            .get(&format!("fan{}", index + 1))
            .cloned()
            .filter(|id| !id.trim().is_empty())
    }

    fn lookup(&self, chip: &str, key: &str) -> Option<String> {
        self.chips.get(chip)?.get(key).cloned()
    }
}

/// `id` keys position `i` in the file when absent; entries without a name
/// display their id.
fn fallback_custom_id(index: usize) -> String {
    format!("custom{}", index + 1)
}

fn fallback_curve_id(index: usize) -> String {
    format!("curve{}", index + 1)
}

pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("zugluft").join("config.toml"))
}

pub fn mtime() -> Option<SystemTime> {
    std::fs::metadata(config_path()?).ok()?.modified().ok()
}

/// Parse errors fall back to defaults: a half-edited file mid-save must not
/// crash the app, and the next mtime change re-reads it anyway.
pub fn load() -> NamesConfig {
    let Some(path) = config_path() else {
        return NamesConfig::default();
    };
    let mut config: NamesConfig = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    for (i, custom) in config.custom.iter_mut().enumerate() {
        if custom.id.is_empty() {
            custom.id = fallback_custom_id(i);
        }
        if custom.name.is_empty() {
            custom.name = custom.id.clone();
        }
    }
    for (i, curve) in config.curve.iter_mut().enumerate() {
        if curve.id.is_empty() {
            curve.id = fallback_curve_id(i);
        }
        if curve.name.is_empty() {
            curve.name = curve.id.clone();
        }
        curve.normalize_functions();
        curve.normalize_window();
        curve.normalize_kind();
    }
    let mut dashboard_items = Vec::new();
    for item in config
        .dashboard
        .item
        .drain(..)
        .filter(DashboardItem::is_valid)
    {
        if !dashboard_items
            .iter()
            .any(|existing: &DashboardItem| existing.matches(&item))
        {
            dashboard_items.push(item);
        }
    }
    config.dashboard.item = dashboard_items;
    config
}
