use super::*;

/// One channel in the Settings visibility tree.
struct ChannelRow {
    /// Index within its category, used only to key the row's toggle.
    index: usize,
    /// Config channel key (`fanN`/`tempN`/`powerN`).
    key: String,
    label: String,
    /// Whether this channel is individually hidden, ignoring parent hides.
    hidden: bool,
    /// Whether the channel currently has a live reading.
    active: bool,
    /// Formatted current reading, if any.
    value: Option<String>,
}

impl Zugluft {
    /// The Settings page: units plus device/category/channel visibility.
    pub(super) fn render_settings(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        cx: &mut Context<Self>,
    ) -> Div {
        let section_title = |title: &'static str| {
            div()
                .text_base()
                .font_weight(FontWeight::MEDIUM)
                .child(title.to_string())
        };
        let row_label = |label: &'static str| {
            div()
                .w(px(110.))
                .flex_none()
                .text_sm()
                .text_color(rgb(TEXT_DIM))
                .child(label)
        };
        let unit_control = |control: Div| div().w(px(260.)).flex_none().child(control);

        let units = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section_title("Units"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(row_label("Temperature"))
                    .child(unit_control(self.segmented([
                        self.segment(
                            ("set-unit-c", 0),
                            "°C",
                            self.temp_unit == TempUnit::Celsius,
                            cx,
                            |this, cx| this.set_temp_unit(TempUnit::Celsius, cx),
                        ),
                        self.segment(
                            ("set-unit-f", 0),
                            "°F",
                            self.temp_unit == TempUnit::Fahrenheit,
                            cx,
                            |this, cx| this.set_temp_unit(TempUnit::Fahrenheit, cx),
                        ),
                    ]))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(row_label("Fan speed"))
                    .child(unit_control(self.segmented([
                        self.segment(
                            ("set-unit-rpm", 0),
                            "U/min",
                            self.fan_unit == FanUnit::Rpm,
                            cx,
                            |this, cx| this.set_fan_unit(FanUnit::Rpm, cx),
                        ),
                        self.segment(
                            ("set-unit-pct", 0),
                            "%",
                            self.fan_unit == FanUnit::Percent,
                            cx,
                            |this, cx| this.set_fan_unit(FanUnit::Percent, cx),
                        ),
                    ]))),
            );

        let hardware = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section_title("Hardware"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(row_label("Fans"))
                    .child(self.icon_button(
                        "calibrate-fans-settings",
                        "icons/fan.svg",
                        "Calibrate",
                        cx,
                        |this, cx| {
                            let _ = this.tx.send(Request::Calibrate);
                            cx.notify();
                        },
                    )),
            );

        // Visibility: a device → category → channel tree. Every channel is
        // listed (active or not) with a status dot so it's clear which
        // sensors are reporting. Hiding a device or category visibly toggles
        // every channel beneath it off as well.
        let mut chip_sections: Vec<Div> = Vec::new();
        for (ci, info) in chips.iter().enumerate() {
            let Some(snapshot) = snapshots.get(ci) else {
                continue;
            };
            let chip_name = info.name.clone();
            let device_hidden = self.names.is_device_hidden(&chip_name);

            let mut fan_rows = Vec::new();
            for (fi, fan) in snapshot.fans.iter().enumerate() {
                let active = fan.rpm.is_some() || fan.duty.is_some();
                let value = fan.rpm.map(|rpm| {
                    self.fan_display_unit()
                        .format_value(self.convert_fan(rpm, fan.max_rpm.unwrap_or(0.0)))
                });
                fan_rows.push(ChannelRow {
                    index: fi,
                    key: format!("fan{}", fi + 1),
                    label: self.names.fan_label(&chip_name, fi),
                    hidden: self
                        .names
                        .is_channel_hidden(&chip_name, &format!("fan{}", fi + 1)),
                    active,
                    value,
                });
            }
            let mut temp_rows = Vec::new();
            for (ti, temp) in snapshot.temps.iter().enumerate() {
                let value =
                    temp.map(|v| self.temp_display_unit().format_value(self.convert_temp(v)));
                temp_rows.push(ChannelRow {
                    index: ti,
                    key: format!("temp{}", ti + 1),
                    label: self.temp_label(&chip_name, ti),
                    hidden: self
                        .names
                        .is_channel_hidden(&chip_name, &format!("temp{}", ti + 1)),
                    active: temp.is_some(),
                    value,
                });
            }
            let mut power_rows = Vec::new();
            for (pi, power) in snapshot.powers.iter().enumerate() {
                let value = power.map(|v| SensorUnit::Watts.format_value(v));
                power_rows.push(ChannelRow {
                    index: pi,
                    key: format!("power{}", pi + 1),
                    label: self.power_label(&chip_name, pi),
                    hidden: self
                        .names
                        .is_channel_hidden(&chip_name, &format!("power{}", pi + 1)),
                    active: power.is_some(),
                    value,
                });
            }

            let active_total = fan_rows.iter().filter(|r| r.active).count()
                + temp_rows.iter().filter(|r| r.active).count()
                + power_rows.iter().filter(|r| r.active).count();
            let sensor_total = fan_rows.len() + temp_rows.len() + power_rows.len();

            let header = div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_baseline()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(if device_hidden { TEXT_DIM } else { TEXT }))
                                .child(self.names.device_label(&chip_name)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_DIM))
                                .child(format!("{active_total}/{sensor_total} active")),
                        ),
                )
                .child(
                    self.eye_toggle(("vis-device", ci), device_hidden, true, cx, {
                        let chip = chip_name.clone();
                        move |this, cx| this.set_device_hidden(&chip, !device_hidden, cx)
                    }),
                );

            let mut section = div()
                .flex()
                .flex_col()
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(PANEL))
                .overflow_hidden()
                .child(header);

            let groups = [
                (HiddenCategory::Fans, fan_rows),
                (HiddenCategory::Temperatures, temp_rows),
                (HiddenCategory::Power, power_rows),
            ];
            for (category, rows) in groups {
                if rows.is_empty() {
                    continue;
                }
                section =
                    section
                        .child(div().h(px(1.)).bg(rgb(BORDER)))
                        .child(self.visibility_group(
                            &chip_name,
                            ci,
                            device_hidden,
                            category,
                            rows,
                            cx,
                        ));
            }
            chip_sections.push(section);
        }

        let visibility = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(section_title("Visibility"))
            .children(chip_sections);

        div().flex_1().min_h(px(0.)).overflow_hidden().child(
            div()
                .id("settings-scroll")
                .size_full()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_4()
                .p_3()
                .child(units)
                .child(hardware)
                .child(visibility),
        )
    }

    /// One sensor-category group in the visibility tree: a category header
    /// with its own toggle, followed by an indented row per channel. The
    /// category toggle is inert while the device is hidden, and each channel
    /// toggle is inert while the device or category is hidden — so hiding a
    /// parent visibly switches the whole subtree off.
    fn visibility_group(
        &self,
        chip_name: &str,
        ci: usize,
        device_hidden: bool,
        category: HiddenCategory,
        channels: Vec<ChannelRow>,
        cx: &mut Context<Self>,
    ) -> Div {
        let (cat_key, chan_id, title) = match category {
            HiddenCategory::Fans => ("vis-cat-fans", "vis-fan", "Fans"),
            HiddenCategory::Temperatures => ("vis-cat-temps", "vis-temp", "Sensors"),
            HiddenCategory::Power => ("vis-cat-power", "vis-power", "Power"),
        };
        let cat_id = (cat_key, ci);
        let base = ci * 64;
        let category_hidden = self.names.is_category_hidden(chip_name, category);
        let group_off = device_hidden || category_hidden;

        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1p5()
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(if group_off { TEXT_DIM } else { TEXT }))
                    .child(title),
            )
            .child(self.eye_toggle(cat_id, group_off, !device_hidden, cx, {
                let chip = chip_name.to_string();
                move |this, cx| this.set_category_hidden(&chip, category, !category_hidden, cx)
            }));

        let mut rows: Vec<Div> = Vec::new();
        for ch in channels {
            let channel_hidden = ch.hidden;
            let effective_off = group_off || channel_hidden;
            let dot_color = if effective_off {
                BORDER
            } else if ch.active {
                ACCENT_OK
            } else {
                TRACK
            };
            let label_color = if effective_off { TEXT_DIM } else { TEXT };
            let trailing = match ch.value {
                Some(value) if !effective_off => value,
                _ if !ch.active => "no signal".to_string(),
                _ => String::new(),
            };
            let chip = chip_name.to_string();
            let key = ch.key.clone();
            rows.push(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl_5()
                    .pr_3()
                    .py_0p5()
                    .child(
                        div()
                            .w(px(7.))
                            .h(px(7.))
                            .flex_none()
                            .rounded_full()
                            .bg(rgb(dot_color)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(rgb(label_color))
                            .child(ch.label),
                    )
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child(trailing))
                    .child(self.eye_toggle(
                        (chan_id, base + ch.index),
                        effective_off,
                        !group_off,
                        cx,
                        move |this, cx| this.set_channel_hidden(&chip, &key, !channel_hidden, cx),
                    )),
            );
        }

        div().flex().flex_col().pb_1().child(header).children(rows)
    }

    /// The first live hardware temperature, as a curve source — the
    /// default for new curves and the fallback when a custom sensor that
    /// curves read from is deleted.
    pub(super) fn first_temp_source(&self) -> Option<CurveSource> {
        let UiState::Service(ServiceState::Ready {
            chips, snapshots, ..
        }) = &self.state
        else {
            return None;
        };
        for (ci, snapshot) in snapshots.iter().enumerate() {
            let Some(info) = chips.get(ci) else { continue };
            for (ti, temp) in snapshot.temps.iter().enumerate() {
                if temp.is_some() {
                    return Some(CurveSource::Temp {
                        chip: info.name.clone(),
                        temp: ti + 1,
                    });
                }
            }
        }
        None
    }

    /// Deletes a custom sensor; curves that read it are repointed to the
    /// first hardware temperature so they keep working.
    pub(super) fn delete_custom_sensor(&mut self, id: &str, cx: &mut Context<Self>) {
        let fallback = self.first_temp_source();
        for def in self.names.curves().to_vec() {
            let reads_it = matches!(&def.source, CurveSource::Custom { custom } if custom == id);
            if reads_it && let Some(source) = fallback.clone() {
                let mut def = def;
                def.source = source;
                config::save_curve(&def);
            }
        }
        config::delete_custom(id);
        self.reload_config(cx);
    }

    /// Small confirmation modal for deleting a curve or a custom sensor.
    pub(super) fn render_confirm_delete(
        &self,
        target: &ConfirmDelete,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let (name, message) = match target {
            ConfirmDelete::Curve(id) => (
                self.names
                    .curves()
                    .iter()
                    .find(|def| &def.id == id)?
                    .name
                    .clone(),
                "Fans driven by this curve return to UEFI/Firmware control.",
            ),
            ConfirmDelete::Custom(id) => (
                self.names
                    .customs()
                    .iter()
                    .find(|def| &def.id == id)?
                    .name
                    .clone(),
                "Curves using this sensor switch to a hardware temperature.",
            ),
        };
        let target = target.clone();
        let panel = self
            .modal_panel("confirm-delete-dialog", px(380.), cx)
            .overflow_y_scroll()
            .gap_3()
            .p_4()
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(format!("Delete “{name}”?")),
            )
            .child(div().text_sm().text_color(rgb(TEXT_DIM)).child(message))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        self.button("confirm-delete-cancel", "Cancel", cx, |this, cx| {
                            this.confirm_delete = None;
                            cx.notify();
                        }),
                    )
                    .child(
                        div().child(
                            div()
                                .id("confirm-delete-go")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(rgb(ERROR))
                                .text_sm()
                                .text_color(rgb(BG))
                                .cursor_pointer()
                                .hover(|s| s.opacity(0.85))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.confirm_delete = None;
                                    match &target {
                                        ConfirmDelete::Curve(id) => {
                                            this.delete_curve(&id.clone(), cx)
                                        }
                                        ConfirmDelete::Custom(id) => {
                                            this.delete_custom_sensor(&id.clone(), cx)
                                        }
                                    }
                                }))
                                .child("Delete"),
                        ),
                    ),
            );

        Some(self.modal_backdrop(panel, cx, |this, cx| {
            this.confirm_delete = None;
            cx.notify();
        }))
    }
}
