use super::*;

// Palette
pub(super) const BG: u32 = 0x14161b;
pub(super) const PANEL: u32 = 0x1c1f26;
pub(super) const BORDER: u32 = 0x2a2e38;
pub(super) const TRACK: u32 = 0x272b34;
pub(super) const GRID_CELL: u32 = 0x191c22;
pub(super) const GRID_LINE: u32 = 0x242935;
pub(super) const FILL_MANUAL: u32 = 0x4f9cf9;
pub(super) const FILL_HOVER: u32 = 0x2f3440;
pub(super) const TEXT: u32 = 0xe8eaf0;
pub(super) const TEXT_DIM: u32 = 0x8d93a3;
pub(super) const ACCENT_WARN: u32 = 0xe5c07b;
pub(super) const ACCENT_OK: u32 = 0x98c379;
pub(super) const ERROR: u32 = 0xe06c75;
pub(super) const HISTORY_LIMIT: usize = 180;
pub(super) const SAMPLE_INTERVAL_SECONDS: f32 = 0.75;
/// Distance between vertical gridlines, anchored to the right edge.
pub(super) const GRID_SPACING: f32 = 96.0;
pub(super) const CROSSHAIR: u32 = 0x3d4660;
pub(super) const SENSOR_COLORS: [u32; 10] = [
    0x5aa0f2, 0x7ec860, 0xe6b85c, 0xe06c75, 0x4fb6c8, 0xb98ae8, 0xe88c5a, 0xd96aa8, 0x9fd84f,
    0xe0d24f,
];
/// Grab distance for curve points, in pixels.
pub(super) const CURVE_HIT_RADIUS: f32 = 10.0;

pub(super) type FanKey = (usize, usize); // (chip index, fan index)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum SensorKind {
    Temperature,
    FanRpm,
    #[allow(dead_code)]
    Power,
    /// User-defined derived sensor; `chip` is unused, `index` is the
    /// position in the service's published customs list.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct SensorKey {
    pub(super) kind: SensorKind,
    pub(super) chip: usize,
    pub(super) index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum SensorUnit {
    Celsius,
    Fahrenheit,
    Rpm,
    Percent,
    Watts,
}

impl SensorUnit {
    /// Display order of axes around the graph.
    pub(super) const ALL: [Self; 5] = [
        Self::Celsius,
        Self::Fahrenheit,
        Self::Rpm,
        Self::Percent,
        Self::Watts,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Celsius => "°C",
            Self::Fahrenheit => "°F",
            Self::Rpm => "U/min",
            Self::Percent => "%",
            Self::Watts => "W",
        }
    }

    pub(super) fn format_value(self, value: f32) -> String {
        match self {
            Self::Celsius => format!("{value:.0} °C"),
            Self::Fahrenheit => format!("{value:.0} °F"),
            Self::Rpm => format!("{value:.0} U/min"),
            Self::Percent => format!("{value:.0} %"),
            Self::Watts => format!("{value:.0} W"),
        }
    }

    pub(super) fn default_range(self) -> (f32, f32) {
        match self {
            Self::Celsius => (20.0, 80.0),
            Self::Fahrenheit => (68.0, 176.0),
            Self::Rpm => (0.0, 5_000.0),
            Self::Percent => (0.0, 100.0),
            Self::Watts => (0.0, 300.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    DashDot,
}

impl LineStyle {
    /// Config token, also used to match the user's saved override.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Dashed => "dashed",
            Self::Dotted => "dotted",
            Self::DashDot => "dashdot",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
            Self::DashDot => "Dash-dot",
        }
    }
}

/// Line styles offered in the appearance editor, in display order.
pub(super) const LINE_STYLES: [LineStyle; 4] = [
    LineStyle::Solid,
    LineStyle::Dashed,
    LineStyle::Dotted,
    LineStyle::DashDot,
];

pub(super) fn parse_line_style(name: &str) -> Option<LineStyle> {
    LINE_STYLES.into_iter().find(|style| style.name() == name)
}

/// The config channel key for a sensor (`tempN`/`fanN`/`powerN`/`custom`),
/// matching the display-name and hidden-channel keys.
pub(super) fn channel_key(key: SensorKey) -> String {
    match key.kind {
        SensorKind::Temperature => format!("temp{}", key.index + 1),
        SensorKind::FanRpm => format!("fan{}", key.index + 1),
        SensorKind::Power => format!("power{}", key.index + 1),
        SensorKind::Custom => "custom".to_string(),
    }
}

/// Whether a sensor's line is shown on the graph by default: everything but
/// fans (those are usually noise next to temperatures).
pub(super) fn default_shown(kind: SensorKind) -> bool {
    !matches!(kind, SensorKind::FanRpm)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AppView {
    Dashboard,
    Curves,
    Fans,
    Telemetry,
    /// Units and channel visibility.
    Settings,
}

impl AppView {
    pub(super) fn id(self) -> usize {
        match self {
            Self::Dashboard => 0,
            Self::Curves => 1,
            Self::Fans => 2,
            Self::Telemetry => 3,
            Self::Settings => 4,
        }
    }

    pub(super) fn icon(self) -> &'static str {
        match self {
            Self::Dashboard => "icons/wind.svg",
            Self::Curves => "icons/spline.svg",
            Self::Fans => "icons/fan.svg",
            Self::Telemetry => "icons/thermometer.svg",
            Self::Settings => "icons/settings.svg",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Curves => "Curves",
            Self::Fans => "Fans",
            Self::Telemetry => "Telemetry",
            Self::Settings => "Settings",
        }
    }
}

/// Which dropdown popup is open (at most one across the app).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Dropdown {
    /// Source picker in the curve edit dialog.
    CurveSource { curve: String },
    /// Curve picker on a fan card (shown in place of the target slider).
    FanCurve { fan: FanKey },
    /// Floating creation menu on the Dashboard page.
    CurveQuickOpen,
    /// "Add input" picker in the custom-sensor editor, by sensor id.
    CustomInput { custom: String },
}

/// What picking a dropdown option does.
pub(super) type DropdownAction = Rc<dyn Fn(&mut Zugluft, &mut Context<Zugluft>)>;

/// A pending delete, awaiting the confirmation modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConfirmDelete {
    /// A `[[curve]]`, by id.
    Curve(String),
    /// A `[[custom]]` sensor, by id.
    Custom(String),
}

#[derive(Clone)]
pub(super) struct SensorFrame {
    pub(super) readings: HashMap<SensorKey, f32>,
}

#[derive(Clone)]
pub(super) struct SensorReading {
    pub(super) key: SensorKey,
    /// Chip the channel belongs to; for derived sensors, the custom
    /// sensor's id instead (used by the row's delete affordance).
    pub(super) chip_name: String,
    pub(super) label: String,
    pub(super) unit: SensorUnit,
    pub(super) value: f32,
    pub(super) color: u32,
    pub(super) line_style: LineStyle,
    pub(super) enabled: bool,
    /// Reference for the percent view (fans only): calibrated max RPM, or
    /// the rolling history max as a stopgap.
    pub(super) fan_max_rpm: f32,
}

/// One sensor-panel section: a display label plus its sensors. Hardware
/// sections keep the raw chip id so the header can rename the device.
pub(super) type PanelSection<'a> = (
    String,
    Option<String>,
    Vec<(&'static str, Vec<&'a SensorReading>)>,
);

#[derive(Clone)]
pub(super) struct GraphSeries {
    pub(super) key: SensorKey,
    pub(super) label: String,
    pub(super) unit: SensorUnit,
    pub(super) color: u32,
    pub(super) line_style: LineStyle,
    pub(super) values: Vec<(usize, f32)>,
}

#[derive(Clone)]
pub(super) struct AxisData {
    pub(super) unit: SensorUnit,
    pub(super) min: f32,
    pub(super) max: f32,
}

#[derive(Clone)]
pub(super) struct GraphData {
    pub(super) history_len: usize,
    pub(super) axes: Vec<AxisData>,
    pub(super) series: Vec<GraphSeries>,
    /// Sample index under the cursor (crosshair position).
    pub(super) hover_index: Option<usize>,
    /// Series under the cursor, drawn emphasized.
    pub(super) hovered: Option<SensorKey>,
}

/// Tooltip for the series under the cursor, positioned relative to the plot.
pub(super) struct HoverTooltip {
    pub(super) label: String,
    pub(super) value: String,
    pub(super) color: u32,
    pub(super) left: Pixels,
    pub(super) top: Pixels,
}

/// The open Edit dialog: rename plus, for graph lines, color/style. The key
/// identifies what's renamed (`commit_rename` routes by its kind); the
/// optional `(chip, channel)` is the config key for its graph appearance
/// (`None` for rename targets without graph appearance).
#[derive(Clone)]
pub(super) struct Rename {
    pub(super) key: SensorKey,
    pub(super) input: TextEdit,
    /// Chip name (or custom id) and `tempN`/`fanN`/`powerN`/`custom`.
    pub(super) appearance: Option<(String, String)>,
    pub(super) device: Option<String>,
}

/// A tiny single-line text editor state. GPUI 0.2.2 exposes lower-level
/// platform input hooks, but the app only needs short fields and predictable
/// Windows editing shortcuts.
#[derive(Clone, Debug)]
pub(super) struct TextEdit {
    pub(super) text: String,
    pub(super) cursor: usize,
    selection_anchor: Option<usize>,
}

impl TextEdit {
    pub(super) fn new(text: String) -> Self {
        let cursor = text.len();
        Self {
            text,
            cursor,
            selection_anchor: None,
        }
    }

    pub(super) fn selected_range(&self) -> std::ops::Range<usize> {
        match self.selection_anchor {
            Some(anchor) if anchor < self.cursor => anchor..self.cursor,
            Some(anchor) if anchor > self.cursor => self.cursor..anchor,
            _ => self.cursor..self.cursor,
        }
    }

    pub(super) fn has_selection(&self) -> bool {
        !self.selected_range().is_empty()
    }

    pub(super) fn selected_text(&self) -> Option<String> {
        let range = self.selected_range();
        (!range.is_empty()).then(|| self.text[range].to_string())
    }

    pub(super) fn select_all(&mut self) -> bool {
        if self.text.is_empty() {
            return false;
        }
        self.cursor = self.text.len();
        self.selection_anchor = Some(0);
        true
    }

    pub(super) fn clear(&mut self) -> bool {
        if self.text.is_empty() {
            self.cursor = 0;
            self.selection_anchor = None;
            return false;
        }
        self.text.clear();
        self.cursor = 0;
        self.selection_anchor = None;
        true
    }

    pub(super) fn move_left(&mut self, selecting: bool, by_word: bool) -> bool {
        if self.has_selection() && !selecting {
            let start = self.selected_range().start;
            return self.move_to(start, false);
        }
        let target = if by_word {
            self.prev_word_boundary()
        } else {
            self.prev_char_boundary(self.cursor)
        };
        self.move_to(target, selecting)
    }

    pub(super) fn move_right(&mut self, selecting: bool, by_word: bool) -> bool {
        if self.has_selection() && !selecting {
            let end = self.selected_range().end;
            return self.move_to(end, false);
        }
        let target = if by_word {
            self.next_word_boundary()
        } else {
            self.next_char_boundary(self.cursor)
        };
        self.move_to(target, selecting)
    }

    pub(super) fn move_home(&mut self, selecting: bool) -> bool {
        self.move_to(0, selecting)
    }

    pub(super) fn move_end(&mut self, selecting: bool) -> bool {
        self.move_to(self.text.len(), selecting)
    }

    pub(super) fn insert_filtered(
        &mut self,
        text: &str,
        max_chars: usize,
        allow: impl Fn(char) -> bool,
    ) -> bool {
        let range = self.selected_range();
        let selected_chars = self.text[range.clone()].chars().count();
        let current_chars = self.text.chars().count();
        let available = max_chars.saturating_sub(current_chars - selected_chars);
        let insert: String = text
            .chars()
            .filter(|ch| allow(*ch))
            .take(available)
            .collect();
        if insert.is_empty() {
            return false;
        }
        let cursor = range.start + insert.len();
        self.text.replace_range(range, &insert);
        self.cursor = cursor;
        self.selection_anchor = None;
        true
    }

    pub(super) fn delete_backward(&mut self, by_word: bool) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.cursor == 0 {
            return false;
        }
        let start = if by_word {
            self.prev_word_boundary()
        } else {
            self.prev_char_boundary(self.cursor)
        };
        self.delete_range(start..self.cursor)
    }

    pub(super) fn delete_forward(&mut self, by_word: bool) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.cursor == self.text.len() {
            return false;
        }
        let end = if by_word {
            self.next_word_boundary()
        } else {
            self.next_char_boundary(self.cursor)
        };
        self.delete_range(self.cursor..end)
    }

    fn move_to(&mut self, target: usize, selecting: bool) -> bool {
        let target = self.boundary_at_or_before(target.min(self.text.len()));
        let changed = self.cursor != target || self.selection_anchor.is_some() != selecting;
        if selecting {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor = target;
        changed
    }

    fn delete_selection(&mut self) -> bool {
        let range = self.selected_range();
        if range.is_empty() {
            return false;
        }
        self.delete_range(range)
    }

    fn delete_range(&mut self, range: std::ops::Range<usize>) -> bool {
        if range.is_empty() {
            return false;
        }
        self.cursor = range.start;
        self.text.replace_range(range, "");
        self.selection_anchor = None;
        true
    }

    fn prev_char_boundary(&self, pos: usize) -> usize {
        self.text[..pos]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next_char_boundary(&self, pos: usize) -> usize {
        self.text[pos..]
            .char_indices()
            .nth(1)
            .map_or(self.text.len(), |(index, _)| pos + index)
    }

    fn prev_word_boundary(&self) -> usize {
        let mut pos = self.cursor;
        let mut seen_word = false;
        for (index, ch) in self.text[..self.cursor].char_indices().rev() {
            if !seen_word {
                pos = index;
                if !ch.is_whitespace() {
                    seen_word = true;
                }
            } else if ch.is_whitespace() {
                break;
            } else {
                pos = index;
            }
        }
        pos
    }

    fn next_word_boundary(&self) -> usize {
        let mut pos = self.cursor;
        let mut seen_word = false;
        for (offset, ch) in self.text[self.cursor..].char_indices() {
            let end = self.cursor + offset + ch.len_utf8();
            if !seen_word {
                pos = end;
                if !ch.is_whitespace() {
                    seen_word = true;
                }
            } else if ch.is_whitespace() {
                break;
            } else {
                pos = end;
            }
        }
        pos
    }

    fn boundary_at_or_before(&self, mut pos: usize) -> usize {
        while pos > 0 && !self.text.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    }
}

/// One editable number in a fan card's tuning section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingField {
    StepUp,
    StepDown,
    Start,
    Stop,
    Offset,
    Minimum,
}

impl SettingField {
    pub(super) fn id(self) -> usize {
        match self {
            Self::StepUp => 0,
            Self::StepDown => 1,
            Self::Start => 2,
            Self::Stop => 3,
            Self::Offset => 4,
            Self::Minimum => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CurveWindowField {
    TempMin,
    TempMax,
    DutyMin,
    DutyMax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CurveKindField {
    TriggerThreshold,
    TriggerBefore,
    TriggerAfter,
    LinearStartTemp,
    LinearStartDuty,
    LinearEndTemp,
    LinearEndDuty,
}

/// In-progress inline edit of a tuning field on a fan card.
pub(super) struct FieldEdit {
    pub(super) key: FanKey,
    pub(super) field: SettingField,
    pub(super) input: TextEdit,
}

/// The service binary ships next to the GUI.
pub(super) fn service_exe() -> Option<PathBuf> {
    let path = std::env::current_exe()
        .ok()?
        .with_file_name("zugluft-service.exe");
    path.exists().then_some(path)
}

pub(super) fn floating_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: hsla(0.0, 0.0, 0.0, 0.28),
        offset: point(px(0.), px(12.)),
        blur_radius: px(24.),
        spread_radius: px(-4.),
    }]
}

pub(super) fn subtle_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: hsla(0.0, 0.0, 0.0, 0.18),
        offset: point(px(0.), px(5.)),
        blur_radius: px(12.),
        spread_radius: px(-4.),
    }]
}

/// Compact number for the tuning fields: whole numbers without a decimal,
/// everything else with one.
pub(super) fn fmt_setting(value: f32) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

pub(super) fn sensor_id(key: SensorKey) -> usize {
    let kind_offset = match key.kind {
        SensorKind::Temperature => 0,
        SensorKind::FanRpm => 1,
        SensorKind::Power => 2,
        SensorKind::Custom => 3,
    };
    kind_offset * 1024 + key.chip * 64 + key.index
}

/// Default style for the n-th sensor in display order: cycle the palette,
/// always solid. Solid strokes read far better on a busy graph than dashes;
/// a line that needs distinguishing can be restyled per channel.
pub(super) fn sensor_style(ordinal: usize) -> (u32, LineStyle) {
    (
        SENSOR_COLORS[ordinal % SENSOR_COLORS.len()],
        LineStyle::Solid,
    )
}

pub(super) fn normalize_axis_range(unit: SensorUnit, min: f32, max: f32) -> (f32, f32) {
    if !min.is_finite() || !max.is_finite() {
        return unit.default_range();
    }

    // A fixed axis keeps the percent view readable while max-RPM references
    // shift as new peaks come in.
    if unit == SensorUnit::Percent {
        return (0.0, 100.0);
    }

    let step = match unit {
        SensorUnit::Celsius => 5.0,
        SensorUnit::Fahrenheit => 10.0,
        SensorUnit::Rpm => 250.0,
        SensorUnit::Percent => 10.0,
        SensorUnit::Watts => 25.0,
    };

    if (max - min).abs() < step {
        let center = (min + max) / 2.0;
        return (
            ((center - step * 2.0) / step).floor() * step,
            ((center + step * 2.0) / step).ceil() * step,
        );
    }

    let padding = ((max - min) * 0.12).max(step);
    (
        ((min - padding) / step).floor() * step,
        ((max + padding) / step).ceil() * step,
    )
}

pub(super) fn axis_ticks(axis: &AxisData) -> Vec<f32> {
    (0..=4)
        .map(|i| axis.max - (axis.max - axis.min) * (i as f32 / 4.0))
        .collect()
}
