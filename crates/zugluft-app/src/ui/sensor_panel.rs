use super::*;

impl Zugluft {
    pub(super) fn render_sensor_toggle(
        &self,
        sensor: &SensorReading,
        cx: &mut Context<Self>,
    ) -> Div {
        let key = sensor.key;

        let group: SharedString = format!("sensor-row-{}", sensor_id(key)).into();
        let label = sensor.label.clone();
        // Config keys for this line's persisted visibility / appearance.
        let channel = channel_key(key);
        let default = default_shown(key.kind);
        div().child(
            div()
                .id(("sensor-toggle", sensor_id(key)))
                .group(group.clone())
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_md()
                .bg(rgb(if sensor.enabled { TRACK } else { PANEL }))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)))
                .on_click(cx.listener({
                    let chip = sensor.chip_name.clone();
                    let channel = channel.clone();
                    move |this, _: &ClickEvent, _, cx| {
                        this.toggle_graph_line(chip.clone(), channel.clone(), default, cx)
                    }
                }))
                // Clicking the swatch opens the Edit dialog (rename +
                // color/style), same as the pencil.
                .child({
                    let chip = sensor.chip_name.clone();
                    let channel = channel.clone();
                    let custom_id = sensor.chip_name.clone();
                    let label = label.clone();
                    let color = if sensor.enabled { sensor.color } else { BORDER };
                    div()
                        .id(("sensor-dot", sensor_id(key)))
                        .flex_none()
                        .p(px(2.))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            if key.kind == SensorKind::Custom {
                                this.open_custom_dialog(custom_id.clone(), cx);
                            } else {
                                this.begin_rename(
                                    key,
                                    label.clone(),
                                    Some((chip.clone(), channel.clone())),
                                    window,
                                    cx,
                                );
                            }
                        }))
                        .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(rgb(color)))
                })
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .truncate()
                        .text_color(rgb(if sensor.enabled { TEXT } else { TEXT_DIM }))
                        .child(sensor.label.clone()),
                )
                .child(
                    div()
                        .id(("sensor-rename", sensor_id(key)))
                        .flex_none()
                        .cursor_pointer()
                        .on_click(cx.listener({
                            let chip = sensor.chip_name.clone();
                            let channel = channel.clone();
                            let custom_id = sensor.chip_name.clone();
                            move |this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                if key.kind == SensorKind::Custom {
                                    this.open_custom_dialog(custom_id.clone(), cx);
                                } else {
                                    this.begin_rename(
                                        key,
                                        label.clone(),
                                        Some((chip.clone(), channel.clone())),
                                        window,
                                        cx,
                                    );
                                }
                            }
                        }))
                        .child(
                            svg()
                                .path("icons/pencil.svg")
                                .w(px(12.))
                                .h(px(12.))
                                // svgs paint with their own text color (no
                                // inheritance): invisible until the row is
                                // hovered, bright under the cursor.
                                .text_color(gpui::transparent_black())
                                .group_hover(group.clone(), |s| s.text_color(rgb(TEXT_DIM)))
                                .hover(|s| s.text_color(rgb(TEXT))),
                        ),
                )
                // Hardware channels can be hidden; derived sensors are
                // config entries, so they get deleted instead.
                .children(
                    matches!(
                        key.kind,
                        SensorKind::Temperature | SensorKind::FanRpm | SensorKind::Power
                    )
                    .then(|| {
                        let chip_name = sensor.chip_name.clone();
                        let prefix = match key.kind {
                            SensorKind::Temperature => "temp",
                            SensorKind::Power => "power",
                            _ => "fan",
                        };
                        div()
                            .id(("sensor-hide", sensor_id(key)))
                            .flex_none()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.set_channel_hidden(
                                    &chip_name.clone(),
                                    &format!("{prefix}{}", key.index + 1),
                                    true,
                                    cx,
                                );
                            }))
                            .child(
                                svg()
                                    .path("icons/eye-off.svg")
                                    .w(px(12.))
                                    .h(px(12.))
                                    .text_color(gpui::transparent_black())
                                    .group_hover(group.clone(), |s| s.text_color(rgb(TEXT_DIM)))
                                    .hover(|s| s.text_color(rgb(TEXT))),
                            )
                    }),
                )
                .children((key.kind == SensorKind::Custom).then(|| {
                    let custom_id = sensor.chip_name.clone();
                    div()
                        .id(("sensor-del", sensor_id(key)))
                        .flex_none()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.confirm_delete = Some(ConfirmDelete::Custom(custom_id.clone()));
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .path("icons/trash.svg")
                                .w(px(12.))
                                .h(px(12.))
                                .text_color(gpui::transparent_black())
                                .group_hover(group, |s| s.text_color(rgb(TEXT_DIM)))
                                .hover(|s| s.text_color(rgb(ERROR))),
                        )
                }))
                .child(
                    div()
                        .text_sm()
                        .font_family(FONT_MONO)
                        .text_color(rgb(if sensor.enabled { TEXT } else { TEXT_DIM }))
                        .child(sensor.unit.format_value(sensor.value)),
                ),
        )
    }

    pub(super) fn segment(
        &self,
        id: (&'static str, usize),
        label: &'static str,
        active: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        div().flex_1().child(
            div()
                .id(id)
                .w_full()
                .h(px(22.))
                .min_w(px(58.))
                .flex()
                .items_center()
                .justify_center()
                .px_2()
                .rounded_sm()
                .border_1()
                .border_color(rgb(if active { FILL_MANUAL } else { TRACK }))
                .text_xs()
                .text_color(rgb(if active { TEXT } else { TEXT_DIM }))
                .bg(rgb(if active { FILL_HOVER } else { TRACK }))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)).text_color(rgb(TEXT)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on_click(this, cx)))
                .child(label),
        )
    }

    pub(super) fn segmented(&self, segments: impl IntoIterator<Item = Div>) -> Div {
        div()
            .w_full()
            .flex()
            .items_center()
            .p(px(1.))
            .rounded_md()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(TRACK))
            .children(segments)
    }

    pub(super) fn render_sensor_group(
        &self,
        title: &'static str,
        sensors: &[&SensorReading],
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_0p5()
            .children((!title.is_empty()).then(|| {
                div()
                    .px_2()
                    .pt_0p5()
                    .text_xs()
                    .text_color(rgb(TEXT_DIM))
                    .child(title)
            }))
            .children(
                sensors
                    .iter()
                    .map(|sensor| self.render_sensor_toggle(sensor, cx)),
            )
    }

    pub(super) fn render_sensors(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Div {
        let sensors = self.sensor_readings(chips, snapshots, customs);
        let graph = self.graph_data(&sensors);
        // The filter narrows the list; the graph keeps showing whatever is
        // toggled on.
        let query = self.sensor_search.text.trim().to_lowercase();
        let matches =
            |s: &SensorReading| query.is_empty() || s.label.to_lowercase().contains(&query);

        // One section per chip — its name as the header — holding one
        // sub-group per sensor kind, so the chip name isn't repeated for
        // every kind.
        let mut sections: Vec<PanelSection> = Vec::new();
        for (ci, _) in snapshots.iter().enumerate() {
            let chip_name = chips
                .get(ci)
                .map(|chip| chip.name.clone())
                .unwrap_or_else(|| format!("chip {ci}"));
            let mut kinds: Vec<(&'static str, Vec<&SensorReading>)> = Vec::new();
            for (kind, kind_label) in [
                (SensorKind::Temperature, "Temperatures"),
                (SensorKind::FanRpm, "Fans"),
                (SensorKind::Power, "Power"),
            ] {
                let members: Vec<&SensorReading> = sensors
                    .iter()
                    .filter(|s| s.key.chip == ci && s.key.kind == kind && matches(s))
                    .collect();
                if !members.is_empty() {
                    kinds.push((kind_label, members));
                }
            }
            if !kinds.is_empty() {
                sections.push((chip_name, kinds));
            }
        }
        let custom_members: Vec<&SensorReading> = sensors
            .iter()
            .filter(|s| s.key.kind == SensorKind::Custom && matches(s))
            .collect();
        if !custom_members.is_empty() {
            sections.push(("Custom Sensors".to_string(), vec![("", custom_members)]));
        }

        // No page header — the breadcrumb names the tab, and dropping it
        // lets the graph and list fill the full height, top- and
        // bottom-aligned with the sidebar pill (matching 8 px insets).
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .gap_2()
            .p_2()
            .child(self.render_sensor_graph(graph))
            .child(self.render_sensor_panel(sections, &sensors, cx))
    }

    pub(super) fn render_sensor_panel(
        &self,
        sections: Vec<PanelSection>,
        sensors: &[SensorReading],
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        // The machine's hostname titles the panel — these are this box's
        // sensors, after all.
        let title = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Active Sensors".to_string());
        div()
            .id("sensor-panel")
            .w(px(260.))
            .h_full()
            // A short window scrolls the list rather than clipping it.
            .min_h(px(0.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(floating_shadow())
            .child(
                div()
                    .px_1()
                    .py_1()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT))
                    .child(title),
            )
            .child(div().px_1().pb_1().child(self.render_search_box(cx)))
            .children(sections.into_iter().enumerate().map(|(i, (chip, kinds))| {
                let mut section = div().flex().flex_col().gap_1().child(
                    div()
                        .px_1()
                        .pt_1p5()
                        .pb_0p5()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(TEXT))
                        .child(chip),
                );
                // A hairline between chips keeps the sections scannable.
                if i > 0 {
                    section = section.border_t_1().border_color(rgb(BORDER)).mt_1();
                }
                section.children(
                    kinds
                        .into_iter()
                        .map(|(kind, members)| self.render_sensor_group(kind, &members, cx)),
                )
            }))
            .children(sensors.is_empty().then(|| {
                div()
                    .px_1()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(TEXT_DIM))
                    .child("No temperature sensors")
            }))
            // Derived-sensor creation lives with the sensor list it adds to.
            .child(
                div()
                    .pt_1p5()
                    .mt_1()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(div().pt_1p5().child(self.button(
                        "add-custom",
                        "Add sensor",
                        cx,
                        |this, cx| this.add_custom(cx),
                    ))),
            )
    }

    pub(super) fn render_ready(
        &self,
        chips: Vec<ChipInfo>,
        snapshots: Vec<ChipSnapshot>,
        notes: Vec<String>,
        customs: Vec<CustomSensorValue>,
        cx: &mut Context<Self>,
    ) -> Div {
        let page = match self.active_view {
            AppView::Controls => self.render_controls(&chips, &snapshots, &notes, &customs, cx),
            AppView::Sensors => self.render_sensors(&chips, &snapshots, &customs, cx),
            AppView::Settings => self.render_settings(&chips, &snapshots, cx),
        };

        div()
            .flex_1()
            // Shrink (and clip) below content size in short windows rather
            // than pushing the layout past the window edges.
            .min_h(px(0.))
            .overflow_hidden()
            .flex()
            .child(self.render_sidebar(cx))
            .child(page)
    }
}
