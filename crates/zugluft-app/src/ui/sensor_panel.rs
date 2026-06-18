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
                .relative()
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
                .child(self.sensor_action_menu(sensor, cx))
                .child(
                    div()
                        .text_sm()
                        .font_family(FONT_MONO)
                        .text_color(rgb(if sensor.enabled { TEXT } else { TEXT_DIM }))
                        .child(sensor.unit.format_value(sensor.value)),
                ),
        )
    }

    pub(super) fn sensor_action_menu(&self, sensor: &SensorReading, cx: &mut Context<Self>) -> Div {
        let key = sensor.key;
        let id = sensor_id(key);
        let dropdown = Dropdown::SensorActions { sensor: id };
        let open = self.open_dropdown.as_ref() == Some(&dropdown);
        let dashboard_item = self.dashboard_sensor_item(sensor);
        let pinned = self.names.is_dashboard_pinned(&dashboard_item);
        let chip = sensor.chip_name.clone();
        let channel = channel_key(key);
        let label = sensor.label.clone();
        let is_hardware = matches!(
            key.kind,
            SensorKind::Temperature | SensorKind::FanRpm | SensorKind::Power
        );

        let menu = open.then(|| {
            let pin_item = dashboard_item.clone();
            let edit_chip = chip.clone();
            let edit_channel = channel.clone();
            let edit_label = label.clone();
            let custom_id = chip.clone();
            let hide_chip = chip.clone();
            let hide_channel = channel.clone();
            let delete_id = chip.clone();

            let mut popup = div()
                .w(Self::ACTION_MENU_WIDTH)
                .flex()
                .flex_col()
                .gap_0p5()
                .p_1()
                .rounded_lg()
                .bg(rgb(BG))
                .border_1()
                .border_color(rgb(BORDER))
                .shadow(floating_shadow())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .id(("sensor-menu-pin", id))
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_1p5()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(FILL_HOVER)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.open_dropdown = None;
                            this.set_dashboard_pinned(pin_item.clone(), !pinned, cx);
                        }))
                        .child(self.menu_icon(
                            "icons/pin.svg",
                            if pinned { FILL_MANUAL } else { TEXT_DIM },
                        ))
                        .child(self.menu_label(
                            if pinned {
                                "Unpin from dashboard"
                            } else {
                                "Pin to dashboard"
                            },
                            TEXT,
                        )),
                )
                .child(
                    div()
                        .id(("sensor-menu-edit", id))
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_1p5()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(FILL_HOVER)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_dropdown = None;
                            if key.kind == SensorKind::Custom {
                                this.open_custom_dialog(custom_id.clone(), cx);
                            } else {
                                this.begin_rename(
                                    key,
                                    edit_label.clone(),
                                    Some((edit_chip.clone(), edit_channel.clone())),
                                    window,
                                    cx,
                                );
                            }
                        }))
                        .child(self.menu_icon("icons/pencil.svg", TEXT_DIM))
                        .child(self.menu_label("Edit", TEXT)),
                );

            if is_hardware {
                popup = popup.child(
                    div()
                        .id(("sensor-menu-hide", id))
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_1p5()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(FILL_HOVER)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.open_dropdown = None;
                            this.set_channel_hidden(
                                &hide_chip.clone(),
                                &hide_channel.clone(),
                                true,
                                cx,
                            );
                        }))
                        .child(self.menu_icon("icons/eye-off.svg", TEXT_DIM))
                        .child(self.menu_label("Hide", TEXT)),
                );
            } else {
                popup = popup.child(
                    div()
                        .id(("sensor-menu-delete", id))
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_1p5()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(FILL_HOVER)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.open_dropdown = None;
                            this.confirm_delete = Some(ConfirmDelete::Custom(delete_id.clone()));
                            cx.notify();
                        }))
                        .child(self.menu_icon("icons/trash.svg", ERROR))
                        .child(self.menu_label("Delete", ERROR)),
                );
            }

            popup_menu(point(px(20.), px(24.)), Corner::TopRight, popup)
        });

        div().relative().flex_none().children(menu).child(
            div()
                .id(("sensor-actions", id))
                .w(px(20.))
                .h(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgb(if open { FILL_HOVER } else { TRACK }))
                .border_1()
                .border_color(rgb(if open { FILL_MANUAL } else { BORDER }))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)).text_color(rgb(TEXT)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.open_dropdown = if this.open_dropdown.as_ref() == Some(&dropdown) {
                        None
                    } else {
                        Some(dropdown.clone())
                    };
                    cx.notify();
                }))
                .child(
                    svg()
                        .path("icons/more-vertical.svg")
                        .w(px(13.))
                        .h(px(13.))
                        .text_color(rgb(TEXT_DIM)),
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
            // Gap + a touch more padding so an active segment's accent border
            // is fully surrounded by the track instead of being flush against
            // its neighbors and the container edge (which clipped it).
            .gap_0p5()
            .p(px(2.))
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
        let sensors = self
            .sensor_readings(chips, snapshots, customs)
            .into_iter()
            .filter(|sensor| {
                matches!(
                    sensor.key.kind,
                    SensorKind::Temperature | SensorKind::Power | SensorKind::Custom
                )
            })
            .collect::<Vec<_>>();
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
                sections.push((self.names.device_label(&chip_name), Some(chip_name), kinds));
            }
        }
        let custom_members: Vec<&SensorReading> = sensors
            .iter()
            .filter(|s| s.key.kind == SensorKind::Custom && matches(s))
            .collect();
        if !custom_members.is_empty() {
            sections.push((
                "Custom Sensors".to_string(),
                None,
                vec![("", custom_members)],
            ));
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
        let title = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Telemetry".to_string());
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
            .children(
                sections
                    .into_iter()
                    .enumerate()
                    .map(|(i, (chip, raw_chip, kinds))| {
                        let group: SharedString = format!("sensor-section-{i}").into();
                        let chip_for_rename = raw_chip.clone();
                        let chip_label = chip.clone();
                        let header = div()
                            .group(group.clone())
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .px_1()
                            .pt_1p5()
                            .pb_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(TEXT))
                                    .child(chip),
                            )
                            .children(chip_for_rename.map(|raw_chip| {
                                div()
                                    .id(("telemetry-device-rename", i))
                                    .flex_none()
                                    .cursor_pointer()
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, window, cx| {
                                            cx.stop_propagation();
                                            this.begin_device_rename(
                                                raw_chip.clone(),
                                                chip_label.clone(),
                                                window,
                                                cx,
                                            );
                                        },
                                    ))
                                    .child(
                                        svg()
                                            .path("icons/pencil.svg")
                                            .w(px(12.))
                                            .h(px(12.))
                                            .text_color(gpui::transparent_black())
                                            .group_hover(group, |s| s.text_color(rgb(TEXT_DIM)))
                                            .hover(|s| s.text_color(rgb(TEXT))),
                                    )
                            }));
                        let mut section = div().flex().flex_col().gap_1().child(header);
                        // A hairline between chips keeps the sections scannable.
                        if i > 0 {
                            section = section.border_t_1().border_color(rgb(BORDER)).mt_1();
                        }
                        section.children(
                            kinds.into_iter().map(|(kind, members)| {
                                self.render_sensor_group(kind, &members, cx)
                            }),
                        )
                    }),
            )
            .children(sensors.is_empty().then(|| {
                div()
                    .px_1()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(TEXT_DIM))
                    .child("No telemetry")
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
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        notes: &[String],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Div {
        let page = match self.active_view {
            AppView::Dashboard => self.render_controls(chips, snapshots, customs, cx),
            AppView::Fans => self.render_fans_page(chips, snapshots, cx),
            AppView::Telemetry => self.render_sensors(chips, snapshots, customs, cx),
            AppView::Settings => self.render_settings(chips, snapshots, notes, cx),
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
