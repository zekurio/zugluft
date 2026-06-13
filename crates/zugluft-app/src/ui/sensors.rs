use super::*;

impl Zugluft {
    pub(super) fn set_temp_unit(&mut self, unit: TempUnit, cx: &mut Context<Self>) {
        self.temp_unit = unit;
        self.persist_units();
        cx.notify();
    }

    pub(super) fn set_fan_unit(&mut self, unit: FanUnit, cx: &mut Context<Self>) {
        self.fan_unit = unit;
        self.persist_units();
        cx.notify();
    }

    pub(super) fn persist_units(&mut self) {
        config::save_units(self.temp_unit, self.fan_unit);
        // Our own write must not trigger the hot-reload path.
        self.names_mtime = config::mtime();
    }

    /// Chip-provided default channel labels, when the chip is known.
    pub(super) fn chip_labels(&self, chip_name: &str) -> (&[String], &[String]) {
        if let UiState::Service(ServiceState::Ready { chips, .. }) = &self.state
            && let Some(info) = chips.iter().find(|info| info.name == chip_name)
        {
            (&info.temp_labels, &info.power_labels)
        } else {
            (&[], &[])
        }
    }

    /// Display name for a temp channel: user override → chip default →
    /// "Temp N".
    pub(super) fn temp_label(&self, chip_name: &str, index: usize) -> String {
        self.names
            .temp_label(chip_name, index, self.chip_labels(chip_name).0)
    }

    pub(super) fn power_label(&self, chip_name: &str, index: usize) -> String {
        self.names
            .power_label(chip_name, index, self.chip_labels(chip_name).1)
    }

    pub(super) fn temp_display_unit(&self) -> SensorUnit {
        match self.temp_unit {
            TempUnit::Celsius => SensorUnit::Celsius,
            TempUnit::Fahrenheit => SensorUnit::Fahrenheit,
        }
    }

    pub(super) fn fan_display_unit(&self) -> SensorUnit {
        match self.fan_unit {
            FanUnit::Rpm => SensorUnit::Rpm,
            FanUnit::Percent => SensorUnit::Percent,
        }
    }

    pub(super) fn convert_temp(&self, celsius: f32) -> f32 {
        match self.temp_unit {
            TempUnit::Celsius => celsius,
            TempUnit::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
        }
    }

    /// Highest RPM this fan reached in the recorded history. Only a
    /// stopgap reference for the percent view while no calibrated max is
    /// available — a steady fan always sits at ~100 % of its own recent
    /// peak.
    pub(super) fn fan_max_rpm(&self, key: SensorKey) -> f32 {
        self.sensor_history
            .iter()
            .filter_map(|frame| frame.readings.get(&key))
            .fold(0.0, |max, value| value.max(max))
    }

    pub(super) fn convert_fan(&self, rpm: f32, max_rpm: f32) -> f32 {
        match self.fan_unit {
            FanUnit::Rpm => rpm,
            FanUnit::Percent if max_rpm > 0.0 => rpm / max_rpm * 100.0,
            FanUnit::Percent => 0.0,
        }
    }

    /// A line's `(color, style, shown)`: the auto-assigned defaults for its
    /// display slot, overlaid with any per-channel config overrides.
    fn channel_appearance(
        &self,
        chip: &str,
        key: SensorKey,
        slot: usize,
    ) -> (u32, LineStyle, bool) {
        let channel = channel_key(key);
        let (mut color, mut style) = sensor_style(slot);
        if let Some(custom) = self.names.graph_color(chip, &channel) {
            color = custom;
        }
        if let Some(custom) = self
            .names
            .graph_style(chip, &channel)
            .and_then(parse_line_style)
        {
            style = custom;
        }
        let shown = self
            .names
            .graph_shown(chip, &channel)
            .unwrap_or_else(|| default_shown(key.kind));
        (color, style, shown)
    }

    pub(super) fn sensor_readings(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
    ) -> Vec<SensorReading> {
        let mut readings = Vec::new();
        // Colors follow display order across every chip and kind, so the
        // palette runs out once before any repeat. Slots advance even for
        // skipped channels: a sensor dropping out (or being hidden) must
        // not recolor everything after it.
        let mut slot = 0usize;
        let mut next_slot = || {
            slot += 1;
            slot - 1
        };
        for (ci, snapshot) in snapshots.iter().enumerate() {
            let chip_name = chips
                .get(ci)
                .map(|chip| chip.name.clone())
                .unwrap_or_else(|| format!("chip {ci}"));
            for (ti, temperature) in snapshot.temps.iter().enumerate() {
                let slot = next_slot();
                let Some(value) = temperature else {
                    continue;
                };
                if self.names.is_hidden(&chip_name, &format!("temp{}", ti + 1)) {
                    continue;
                }
                let key = SensorKey {
                    kind: SensorKind::Temperature,
                    chip: ci,
                    index: ti,
                };
                let (color, line_style, enabled) = self.channel_appearance(&chip_name, key, slot);
                readings.push(SensorReading {
                    key,
                    chip_name: chip_name.clone(),
                    label: self.temp_label(&chip_name, ti),
                    unit: self.temp_display_unit(),
                    value: self.convert_temp(*value),
                    color,
                    line_style,
                    enabled,
                    fan_max_rpm: 0.0,
                });
            }
            for (fi, fan) in snapshot.fans.iter().enumerate() {
                let slot = next_slot();
                let Some(rpm) = fan.rpm else {
                    continue;
                };
                if self.names.is_hidden(&chip_name, &format!("fan{}", fi + 1)) {
                    continue;
                }
                let key = SensorKey {
                    kind: SensorKind::FanRpm,
                    chip: ci,
                    index: fi,
                };
                let (color, line_style, enabled) = self.channel_appearance(&chip_name, key, slot);
                let max_rpm = fan.max_rpm.unwrap_or_else(|| self.fan_max_rpm(key));
                readings.push(SensorReading {
                    key,
                    chip_name: chip_name.clone(),
                    label: self.names.fan_label(&chip_name, fi),
                    unit: self.fan_display_unit(),
                    value: self.convert_fan(rpm, max_rpm),
                    color,
                    line_style,
                    enabled,
                    fan_max_rpm: max_rpm,
                });
            }
            for (pi, power) in snapshot.powers.iter().enumerate() {
                let slot = next_slot();
                let Some(value) = power else {
                    continue;
                };
                if self
                    .names
                    .is_hidden(&chip_name, &format!("power{}", pi + 1))
                {
                    continue;
                }
                let key = SensorKey {
                    kind: SensorKind::Power,
                    chip: ci,
                    index: pi,
                };
                let (color, line_style, enabled) = self.channel_appearance(&chip_name, key, slot);
                readings.push(SensorReading {
                    key,
                    chip_name: chip_name.clone(),
                    label: self.power_label(&chip_name, pi),
                    unit: SensorUnit::Watts,
                    value: *value,
                    color,
                    line_style,
                    enabled,
                    fan_max_rpm: 0.0,
                });
            }
        }
        for (i, custom) in customs.iter().enumerate() {
            let slot = next_slot();
            let Some(value) = custom.value else { continue };
            let key = SensorKey {
                kind: SensorKind::Custom,
                chip: 0,
                index: i,
            };
            let (color, line_style, enabled) = self.channel_appearance(&custom.id, key, slot);
            // Local config name wins so a rename shows up without a
            // service round-trip.
            let label = self
                .names
                .customs()
                .iter()
                .find(|def| def.id == custom.id)
                .map(|def| def.name.clone())
                .unwrap_or_else(|| custom.name.clone());
            readings.push(SensorReading {
                key,
                chip_name: custom.id.clone(),
                label,
                unit: self.temp_display_unit(),
                value: self.convert_temp(value),
                color,
                line_style,
                enabled,
                fan_max_rpm: 0.0,
            });
        }
        readings
    }

    pub(super) fn graph_data(&self, sensors: &[SensorReading]) -> GraphData {
        let mut ranges: HashMap<SensorUnit, (f32, f32)> = HashMap::new();
        let mut series = Vec::new();

        for sensor in sensors.iter().filter(|sensor| sensor.enabled) {
            let mut values = Vec::new();
            for (i, frame) in self.sensor_history.iter().enumerate() {
                if let Some(raw) = frame.readings.get(&sensor.key) {
                    let value = match sensor.key.kind {
                        SensorKind::Temperature | SensorKind::Custom => self.convert_temp(*raw),
                        SensorKind::FanRpm => self.convert_fan(*raw, sensor.fan_max_rpm),
                        SensorKind::Power => *raw,
                    };
                    ranges
                        .entry(sensor.unit)
                        .and_modify(|(min, max)| {
                            *min = min.min(value);
                            *max = max.max(value);
                        })
                        .or_insert((value, value));
                    values.push((i, value));
                }
            }
            if !values.is_empty() {
                series.push(GraphSeries {
                    key: sensor.key,
                    label: sensor.label.clone(),
                    unit: sensor.unit,
                    color: sensor.color,
                    line_style: sensor.line_style,
                    values,
                });
            }
        }

        let mut axes = SensorUnit::ALL
            .into_iter()
            .filter_map(|unit| {
                let (min, max) = ranges.remove(&unit)?;
                let (min, max) = normalize_axis_range(unit, min, max);
                Some(AxisData { unit, min, max })
            })
            .collect::<Vec<_>>();

        if axes.is_empty() {
            let unit = self.temp_display_unit();
            let (min, max) = unit.default_range();
            axes.push(AxisData { unit, min, max });
        }

        GraphData {
            history_len: self.sensor_history.len().max(1),
            axes,
            series,
            hover_index: None,
            hovered: None,
        }
    }

    /// Resolve the cursor position to a sample index and the nearest series
    /// within grab distance, producing crosshair state and a tooltip.
    pub(super) fn apply_graph_hover(&self, graph: &mut GraphData) -> Option<HoverTooltip> {
        let bounds = (*self.graph_bounds.borrow())?;
        let pos = self.graph_hover.filter(|pos| bounds.contains(pos))?;

        let denom = graph.history_len.saturating_sub(1).max(1) as f32;
        let fraction = ((pos.x - bounds.origin.x) / bounds.size.width).clamp(0.0, 1.0);
        let index = (fraction * denom).round() as usize;
        graph.hover_index = Some(index);

        let mut best: Option<(f32, &GraphSeries, f32)> = None;
        for series in &graph.series {
            let Some(&(_, value)) = series.values.iter().find(|(i, _)| *i == index) else {
                continue;
            };
            let Some(axis) = graph.axes.iter().find(|axis| axis.unit == series.unit) else {
                continue;
            };
            let range = (axis.max - axis.min).max(1.0);
            let y_fraction = ((value - axis.min) / range).clamp(0.0, 1.0);
            let y = bounds.origin.y + bounds.size.height * (1.0 - y_fraction);
            let distance = f32::from((y - pos.y).abs());
            if distance < 20.0 && best.is_none_or(|(d, _, _)| distance < d) {
                best = Some((distance, series, value));
            }
        }

        let (_, series, value) = best?;
        graph.hovered = Some(series.key);

        // Keep the tooltip inside the plot near the right edge.
        let flip = pos.x + px(160.) > bounds.origin.x + bounds.size.width;
        let left = pos.x - bounds.origin.x + if flip { px(-150.) } else { px(14.) };
        Some(HoverTooltip {
            label: series.label.clone(),
            value: series.unit.format_value(value),
            color: series.color,
            left,
            top: pos.y - bounds.origin.y + px(14.),
        })
    }
}
