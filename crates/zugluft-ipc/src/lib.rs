//! IPC protocol between the zugluft service and its clients (GUI, CLI).
//!
//! Transport: two single-direction named pipes carrying newline-delimited
//! JSON. The service pushes [`Event`]s on the events pipe (the first event
//! after connecting is always the current state); clients send [`Request`]s
//! on the control pipe.
//!
//! Two pipes because synchronous pipe handles serialize I/O on the file
//! object: a blocking read parks a concurrent write on the same instance
//! forever. One direction per connection sidesteps that without overlapped
//! I/O.

pub mod pipe;

use std::io::{self, BufRead, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Service → client event stream.
pub const EVENTS_PIPE: &str = r"\\.\pipe\zugluft.events";
/// Client → service request stream.
pub const CONTROL_PIPE: &str = r"\\.\pipe\zugluft.control";

/// Client → service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    /// Pin a fan to a fixed target (`Some(round(percent * 255 / 100))`) or
    /// hand it back to automatic control (`None`). Calibrated services
    /// interpret the target as speed percent and map it to a hardware
    /// command; uncalibrated services write the equivalent command percent.
    SetTarget {
        chip: usize,
        fan: usize,
        target: Option<u8>,
    },
    /// Measure every controllable fan's RPM response: full command first (the
    /// max-RPM reference for percent displays), then stepping down. Takes
    /// about a minute; previous duties are restored afterwards.
    Calibrate,
    /// Replace the set of user-defined derived sensors. The service
    /// persists them and recomputes their values every poll, so they work
    /// (and can later drive curves) with no GUI running.
    SetCustomSensors(Vec<CustomSensorDef>),
    /// Replace one fan's tuning settings. The service persists them keyed
    /// by chip identity and applies them to every target it drives.
    SetFanSettings {
        chip: usize,
        fan: usize,
        settings: FanSettings,
    },
    /// Replace the set of user-defined fan curves. The service persists
    /// them and re-evaluates every poll, so curve control keeps working
    /// with no GUI running.
    SetCurves(Vec<CurveDef>),
    /// Drive a fan from a curve (`Some(id)`) or release it back to the
    /// chip's automatic control (`None`). A `SetTarget` for the fan also
    /// releases it.
    SetFanCurve {
        chip: usize,
        fan: usize,
        curve: Option<String>,
    },
    /// Drop the hardware session and re-detect (e.g. after installing the
    /// LHM bridge or fixing permissions).
    Redetect,
}

/// Service → client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    State(ServiceState),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceState {
    /// Hardware detection is in progress.
    Detecting,
    /// The hardware session could not be opened (bridge missing, no supported
    /// hardware, ...). The service retries periodically.
    Failed { error: String },
    /// A calibration run is stepping the fans; normal polling is paused.
    Calibrating { message: String },
    Ready {
        chips: Vec<ChipInfo>,
        snapshots: Vec<ChipSnapshot>,
        notes: Vec<String>,
        /// Current values of the user-defined derived sensors.
        #[serde(default)]
        customs: Vec<CustomSensorValue>,
        /// Live evaluation of every user-defined fan curve.
        #[serde(default)]
        curves: Vec<CurveStatus>,
    },
}

/// A user-defined fan curve: maps a temperature source to a fan target.
/// Doubles as the `[[curve]]` entry format in the GUI's config.toml.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurveDef {
    /// Stable identity; display names can change freely.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Temperature input driving the curve.
    pub source: CurveSource,
    /// Processing functions applied by the service around the base graph.
    /// Empty means "use the legacy `hysteresis` field as a Standard
    /// function", so older configs retain their behavior.
    #[serde(default)]
    pub functions: Vec<CurveFunction>,
    /// Legacy hysteresis applied by the service after evaluating the graph.
    /// New configs should use `functions = [{ kind = "standard", ... }]`;
    /// this stays for compatibility with configs written before functions.
    #[serde(default)]
    pub hysteresis: CurveHysteresis,
    /// Editor display range for the graph. This does not change evaluation:
    /// points still store absolute °C and target % values.
    #[serde(default)]
    pub window: CurveWindow,
    /// The temp→target mapping; flattened so config entries read
    /// `kind = "graph"` next to the kind's own fields.
    #[serde(flatten)]
    pub kind: CurveKind,
}

impl CurveDef {
    pub fn normalize_functions(&mut self) {
        if self.functions.is_empty() {
            self.functions.push(CurveFunction::Standard {
                hysteresis: self.hysteresis,
            });
        }
        for function in &mut self.functions {
            *function = function.sanitized();
        }
        if let Some(CurveFunction::Standard { hysteresis }) = self.functions.first().copied() {
            self.hysteresis = hysteresis;
        }
    }

    pub fn normalize_window(&mut self) {
        self.window = self.window.sanitized();
    }

    pub fn normalize_kind(&mut self) {
        self.kind = self.kind.sanitized();
    }

    pub fn processing_functions(&self) -> Vec<CurveFunction> {
        if self.functions.is_empty() {
            vec![CurveFunction::Standard {
                hysteresis: self.hysteresis,
            }]
        } else {
            self.functions
                .iter()
                .copied()
                .map(CurveFunction::sanitized)
                .collect()
        }
    }

    pub fn primary_function(&self) -> CurveFunction {
        self.functions
            .first()
            .copied()
            .unwrap_or(CurveFunction::Standard {
                hysteresis: self.hysteresis,
            })
            .sanitized()
    }

    pub fn set_primary_function(&mut self, function: CurveFunction) {
        if self.functions.is_empty() {
            self.functions.push(function.sanitized());
        } else {
            self.functions[0] = function.sanitized();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurveWindow {
    #[serde(default = "default_curve_temp_min")]
    pub temp_min: f32,
    #[serde(default = "default_curve_temp_max")]
    pub temp_max: f32,
    #[serde(default = "default_curve_duty_min")]
    pub duty_min: f32,
    #[serde(default = "default_curve_duty_max")]
    pub duty_max: f32,
}

impl Default for CurveWindow {
    fn default() -> Self {
        Self {
            temp_min: default_curve_temp_min(),
            temp_max: default_curve_temp_max(),
            duty_min: default_curve_duty_min(),
            duty_max: default_curve_duty_max(),
        }
    }
}

impl CurveWindow {
    pub fn sanitized(self) -> Self {
        let mut temp_min = self.temp_min.clamp(-40.0, 150.0);
        let mut temp_max = self.temp_max.clamp(-40.0, 150.0);
        if temp_max < temp_min {
            std::mem::swap(&mut temp_min, &mut temp_max);
        }
        if temp_max - temp_min < 5.0 {
            temp_max = (temp_min + 5.0).min(150.0);
            temp_min = temp_min.min(temp_max - 5.0);
        }

        let mut duty_min = self.duty_min.clamp(0.0, 100.0);
        let mut duty_max = self.duty_max.clamp(0.0, 100.0);
        if duty_max < duty_min {
            std::mem::swap(&mut duty_min, &mut duty_max);
        }
        if duty_max - duty_min < 5.0 {
            duty_max = (duty_min + 5.0).min(100.0);
            duty_min = duty_min.min(duty_max - 5.0);
        }

        Self {
            temp_min,
            temp_max,
            duty_min,
            duty_max,
        }
    }

    pub fn temp_span(self) -> f32 {
        let this = self.sanitized();
        this.temp_max - this.temp_min
    }

    pub fn duty_span(self) -> f32 {
        let this = self.sanitized();
        this.duty_max - this.duty_min
    }

    pub fn temp_fraction(self, temp: f32) -> f32 {
        let this = self.sanitized();
        ((temp - this.temp_min) / this.temp_span()).clamp(0.0, 1.0)
    }

    pub fn duty_fraction(self, duty: f32) -> f32 {
        let this = self.sanitized();
        ((duty - this.duty_min) / this.duty_span()).clamp(0.0, 1.0)
    }

    pub fn temp_at(self, fraction: f32) -> f32 {
        let this = self.sanitized();
        this.temp_min + this.temp_span() * fraction.clamp(0.0, 1.0)
    }

    pub fn duty_at(self, fraction: f32) -> f32 {
        let this = self.sanitized();
        this.duty_min + this.duty_span() * fraction.clamp(0.0, 1.0)
    }
}

fn default_curve_temp_min() -> f32 {
    0.0
}

fn default_curve_temp_max() -> f32 {
    100.0
}

fn default_curve_duty_min() -> f32 {
    0.0
}

fn default_curve_duty_max() -> f32 {
    100.0
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CurveFunction {
    /// Pass the graph output through unchanged.
    Identity,
    /// Hold output changes until the temperature has moved far enough for
    /// long enough. This is the default for existing curves.
    Standard {
        #[serde(default)]
        hysteresis: CurveHysteresis,
    },
    /// Smooth the curve input temperature with an exponential moving
    /// average, then evaluate the graph from the smoothed input.
    Ema {
        #[serde(default = "default_curve_ema_alpha")]
        alpha: f32,
    },
}

impl CurveFunction {
    pub fn sanitized(self) -> Self {
        match self {
            Self::Identity => Self::Identity,
            Self::Standard { hysteresis } => Self::Standard {
                hysteresis: hysteresis.sanitized(),
            },
            Self::Ema { alpha } => Self::Ema {
                alpha: alpha.clamp(0.01, 1.0),
            },
        }
    }
}

fn default_curve_ema_alpha() -> f32 {
    0.25
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurveHysteresis {
    /// Temperature delta required before a held target can change.
    #[serde(default = "default_curve_hysteresis_degrees")]
    pub degrees: f32,
    /// How long the temperature must remain beyond `degrees` before the
    /// service applies the new target.
    #[serde(default = "default_curve_hysteresis_delay_ms")]
    pub delay_ms: u64,
    /// When true, heat-up is immediate and hysteresis only delays cool-down.
    #[serde(default = "default_curve_hysteresis_only_downward")]
    pub only_downward: bool,
}

impl Default for CurveHysteresis {
    fn default() -> Self {
        Self {
            degrees: default_curve_hysteresis_degrees(),
            delay_ms: default_curve_hysteresis_delay_ms(),
            only_downward: default_curve_hysteresis_only_downward(),
        }
    }
}

impl CurveHysteresis {
    pub fn sanitized(self) -> Self {
        Self {
            degrees: self.degrees.clamp(0.0, 20.0),
            delay_ms: self.delay_ms.min(60_000),
            only_downward: self.only_downward,
        }
    }

    pub fn is_disabled(self) -> bool {
        let this = self.sanitized();
        this.degrees <= f32::EPSILON && this.delay_ms == 0
    }
}

fn default_curve_hysteresis_degrees() -> f32 {
    2.0
}

fn default_curve_hysteresis_delay_ms() -> u64 {
    2_000
}

fn default_curve_hysteresis_only_downward() -> bool {
    true
}

/// Where a curve reads its input temperature from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CurveSource {
    /// Hardware temperature channel (1-based, like `temp1` config keys).
    Temp { chip: String, temp: usize },
    /// A user-defined derived sensor, by its `[[custom]]` id.
    Custom { custom: String },
}

impl CurveSource {
    /// The source's current reading, looked up in published state. Both
    /// the service and the GUI resolve through this so they always agree.
    pub fn resolve(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
    ) -> Option<f32> {
        match self {
            Self::Temp { chip, temp } => {
                let ci = chips.iter().position(|info| &info.name == chip)?;
                snapshots
                    .get(ci)?
                    .temps
                    .get(temp.checked_sub(1)?)
                    .copied()
                    .flatten()
            }
            Self::Custom { custom } => customs.iter().find(|value| &value.id == custom)?.value,
        }
    }
}

/// The temp→target mapping itself. New curve kinds (mix, flat, trigger, …)
/// are added as variants; the `kind` tag keeps configs and the wire format
/// stable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CurveKind {
    /// Point graph: linear interpolation between `(°C, target %)` points,
    /// clamped to the first/last point outside their span.
    Graph { points: Vec<(f32, f32)> },
    /// Breakpoint: hold `before` through `threshold`, then switch to `after`.
    Trigger {
        #[serde(default = "default_curve_trigger_threshold")]
        threshold: f32,
        #[serde(default = "default_curve_trigger_before")]
        before: f32,
        #[serde(default = "default_curve_trigger_after")]
        after: f32,
    },
    /// Two-point line: hold the first duty before `start`, interpolate to
    /// `end`, then hold the second duty after it.
    Linear {
        #[serde(default = "default_curve_linear_start")]
        start: (f32, f32),
        #[serde(default = "default_curve_linear_end")]
        end: (f32, f32),
    },
}

impl CurveKind {
    pub fn sanitized(&self) -> Self {
        match self {
            Self::Graph { points } => Self::Graph {
                points: sanitize_graph_points(points),
            },
            Self::Trigger {
                threshold,
                before,
                after,
            } => Self::Trigger {
                threshold: finite_or(*threshold, default_curve_trigger_threshold())
                    .clamp(-40.0, 150.0),
                before: sanitize_percent(*before, default_curve_trigger_before()),
                after: sanitize_percent(*after, default_curve_trigger_after()),
            },
            Self::Linear { start, end } => {
                let mut start = sanitize_point(*start, default_curve_linear_start());
                let mut end = sanitize_point(*end, default_curve_linear_end());
                if end.0 < start.0 {
                    std::mem::swap(&mut start, &mut end);
                }
                if (end.0 - start.0).abs() < 0.5 {
                    end.0 = (start.0 + 0.5).min(150.0);
                    start.0 = start.0.min(end.0 - 0.5);
                }
                Self::Linear { start, end }
            }
        }
    }

    /// Target fan speed (%) for an input temperature; `None` if the curve cannot
    /// produce a value (e.g. no points).
    pub fn evaluate(&self, input: f32) -> Option<f32> {
        if !input.is_finite() {
            return None;
        }
        match self {
            Self::Graph { points } => {
                // Hand-edited configs may hold unsorted or junk points.
                let points = sanitize_graph_points(points);
                if points.is_empty() {
                    return None;
                }

                let (first, last) = (points[0], points[points.len() - 1]);
                if input <= first.0 {
                    return Some(first.1);
                }
                if input >= last.0 {
                    return Some(last.1);
                }
                let segment = points
                    .windows(2)
                    .find(|pair| pair[0].0 <= input && input <= pair[1].0)?;
                let (from, to) = (segment[0], segment[1]);
                let fraction = (input - from.0) / (to.0 - from.0).max(f32::EPSILON);
                Some(sanitize_percent(
                    from.1 + (to.1 - from.1) * fraction,
                    from.1,
                ))
            }
            Self::Trigger { .. } => {
                let Self::Trigger {
                    threshold,
                    before,
                    after,
                } = self.sanitized()
                else {
                    unreachable!();
                };
                Some(if input <= threshold { before } else { after })
            }
            Self::Linear { .. } => {
                let Self::Linear { start, end } = self.sanitized() else {
                    unreachable!();
                };
                if input <= start.0 {
                    Some(start.1)
                } else if input >= end.0 {
                    Some(end.1)
                } else {
                    let fraction = (input - start.0) / (end.0 - start.0).max(f32::EPSILON);
                    Some(sanitize_percent(
                        start.1 + (end.1 - start.1) * fraction,
                        start.1,
                    ))
                }
            }
        }
    }
}

fn sanitize_graph_points(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut points: Vec<(f32, f32)> = points
        .iter()
        .copied()
        .filter(|(temp, percent)| temp.is_finite() && percent.is_finite())
        .map(|(temp, percent)| (temp.clamp(-40.0, 150.0), percent.clamp(0.0, 100.0)))
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    points
}

fn sanitize_point(point: (f32, f32), fallback: (f32, f32)) -> (f32, f32) {
    (
        finite_or(point.0, fallback.0).clamp(-40.0, 150.0),
        sanitize_percent(point.1, fallback.1),
    )
}

fn sanitize_percent(value: f32, fallback: f32) -> f32 {
    finite_or(value, fallback).clamp(0.0, 100.0)
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn default_curve_trigger_threshold() -> f32 {
    60.0
}

fn default_curve_trigger_before() -> f32 {
    30.0
}

fn default_curve_trigger_after() -> f32 {
    100.0
}

fn default_curve_linear_start() -> (f32, f32) {
    (30.0, 20.0)
}

fn default_curve_linear_end() -> (f32, f32) {
    (70.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_curve_switches_at_threshold() {
        let curve = CurveKind::Trigger {
            threshold: 60.0,
            before: 30.0,
            after: 90.0,
        };

        assert_eq!(curve.evaluate(59.0), Some(30.0));
        assert_eq!(curve.evaluate(60.0), Some(30.0));
        assert_eq!(curve.evaluate(60.1), Some(90.0));
    }

    #[test]
    fn trigger_curve_is_a_simple_breakpoint() {
        let curve = CurveKind::Trigger {
            threshold: 60.0,
            before: 30.0,
            after: 90.0,
        };

        assert_eq!(curve.evaluate(60.0), Some(30.0));
        assert_eq!(curve.evaluate(60.001), Some(90.0));
    }

    #[test]
    fn linear_curve_clamps_outside_its_points() {
        let curve = CurveKind::Linear {
            start: (40.0, 40.0),
            end: (60.0, 60.0),
        };

        assert_eq!(curve.evaluate(30.0), Some(40.0));
        assert_eq!(curve.evaluate(50.0), Some(50.0));
        assert_eq!(curve.evaluate(70.0), Some(60.0));
    }

    #[test]
    fn graph_curve_still_clamps_to_endpoint_duties() {
        let curve = CurveKind::Graph {
            points: vec![(40.0, 40.0), (60.0, 60.0)],
        };

        assert_eq!(curve.evaluate(30.0), Some(40.0));
        assert_eq!(curve.evaluate(70.0), Some(60.0));
    }
}

/// A curve's live evaluation, published with every snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurveStatus {
    pub id: String,
    pub name: String,
    /// Source temperature (°C); `None` while the source is unavailable.
    pub input: Option<f32>,
    /// Target fan speed (%) the curve maps `input` to.
    pub output: Option<f32>,
}

/// A user-defined sensor derived from hardware temperature channels.
/// Doubles as the `[[custom]]` entry format in the GUI's config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSensorDef {
    /// Stable identity; display names can change freely.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub kind: CustomKind,
    pub inputs: Vec<CustomInput>,
}

impl CustomSensorDef {
    /// The sensor's current reading from live snapshots. Unavailable inputs
    /// are skipped; with no available input it reads `None`. Both the
    /// service and the GUI evaluate through this so the editor preview and
    /// the published value always agree.
    pub fn evaluate(&self, chips: &[ChipInfo], snapshots: &[ChipSnapshot]) -> Option<f32> {
        let inputs = self.inputs.iter().filter_map(|input| {
            let ci = chips.iter().position(|chip| chip.name == input.chip)?;
            let value = snapshots
                .get(ci)?
                .temps
                .get(input.temp.checked_sub(1)?)
                .copied()
                .flatten()?;
            Some((value, input.weight))
        });
        match self.kind {
            CustomKind::Average => {
                let (sum, weights) = inputs.fold((0.0f32, 0.0f32), |(s, w), (v, weight)| {
                    (s + v * weight, w + weight)
                });
                (weights > 0.0).then(|| sum / weights)
            }
            CustomKind::Min => inputs.map(|(v, _)| v).reduce(f32::min),
            CustomKind::Max => inputs.map(|(v, _)| v).reduce(f32::max),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomKind {
    /// Weighted arithmetic mean; inputs default to weight 1.0.
    Average,
    Min,
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomInput {
    /// Chip name as shown in the UI, e.g. "ITE IT8688E".
    pub chip: String,
    /// 1-based temperature channel, matching `temp1`-style config keys.
    pub temp: usize,
    /// Only meaningful for [`CustomKind::Average`].
    #[serde(default = "default_weight")]
    pub weight: f32,
}

fn default_weight() -> f32 {
    1.0
}

/// A derived sensor's computed reading, published with every snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSensorValue {
    pub id: String,
    pub name: String,
    /// °C; `None` while every input is unavailable.
    pub value: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChipInfo {
    pub name: String,
    pub address: u16,
    pub version: u8,
    /// Super I/O slot (0/1); pseudo slots for sensor-only devices:
    /// 2 = CPU, 3 = GPU.
    pub slot: u8,
    pub fan_count: usize,
    pub control_count: usize,
    pub temp_count: usize,
    /// Default display names per temperature channel; empty when the
    /// mapping is board-dependent and unknown ("Temp N" in UIs).
    #[serde(default)]
    pub temp_labels: Vec<String>,
    /// Display names per power channel (`ChipSnapshot::powers`).
    #[serde(default)]
    pub power_labels: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChipSnapshot {
    pub fans: Vec<FanStatus>,
    pub temps: Vec<Option<f32>>,
    /// Power readings in W, aligned with [`ChipInfo::power_labels`].
    #[serde(default)]
    pub powers: Vec<Option<f32>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FanStatus {
    pub rpm: Option<f32>,
    /// Last target (%) zugluft asked this fan to hold. For calibrated fans
    /// this is a speed target; the hardware command in `duty` may differ.
    #[serde(default)]
    pub target_percent: Option<f32>,
    pub duty: Option<FanDuty>,
    /// RPM this fan reached at full duty in the last calibration run.
    #[serde(default)]
    pub max_rpm: Option<f32>,
    /// Lowest speed (%) calibration could make this fan hold. Some pumps
    /// and headers keep spinning even at a 0 % hardware command.
    #[serde(default)]
    pub min_percent: Option<f32>,
    /// Highest duty (%) at which the fan stood still while calibration
    /// stepped downwards. `None` if it never stopped (or wasn't probed).
    #[serde(default)]
    pub stop_percent: Option<f32>,
    /// Lowest duty (%) that restarted the fan from a standstill. Higher
    /// than `stop_percent` because of motor hysteresis.
    #[serde(default)]
    pub start_percent: Option<f32>,
    /// User tuning for this fan, persisted by the service.
    #[serde(default)]
    pub settings: FanSettings,
    /// Id of the curve driving this fan, if one is assigned.
    #[serde(default)]
    pub curve: Option<String>,
}

/// Per-fan tuning the service applies to every target it drives. Calibration
/// measures start/stop command defaults; these fields exist for what
/// calibration cannot know or the user wants to override.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct FanSettings {
    /// Max ramp-up rate in %/s; `None` applies increases instantly.
    #[serde(default)]
    pub step_up: Option<f32>,
    /// Max ramp-down rate in %/s; `None` applies decreases instantly.
    #[serde(default)]
    pub step_down: Option<f32>,
    /// Duty (%) needed to spin the fan up from a standstill; overrides the
    /// calibrated value in [`FanStatus::start_percent`].
    #[serde(default)]
    pub start_percent: Option<f32>,
    /// Duty (%) at or below which the fan stalls; overrides the calibrated
    /// value in [`FanStatus::stop_percent`].
    #[serde(default)]
    pub stop_percent: Option<f32>,
    /// Added to every requested target (%) before clamping.
    #[serde(default)]
    pub offset: f32,
    /// Floor (%) for every driven target — keeps pumps and CPU fans from
    /// being stopped by a low request.
    #[serde(default)]
    pub minimum_percent: f32,
}

impl FanSettings {
    /// Offset/minimum shaping for a requested target (%).
    pub fn effective_percent(&self, requested: f32) -> f32 {
        (requested + self.offset).clamp(self.minimum_percent.clamp(0.0, 100.0), 100.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FanDuty {
    Auto,
    Manual { percent: f32 },
}

/// Writes one newline-delimited JSON message.
pub fn send<T: Serialize>(writer: &mut impl Write, msg: &T) -> io::Result<()> {
    let mut line = serde_json::to_vec(msg).map_err(io::Error::other)?;
    line.push(b'\n');
    writer.write_all(&line)
}

/// Reads one message; `Ok(None)` on a clean EOF (peer disconnected).
pub fn recv<T: DeserializeOwned>(reader: &mut impl BufRead) -> io::Result<Option<T>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    serde_json::from_str(&line)
        .map(Some)
        .map_err(io::Error::other)
}
