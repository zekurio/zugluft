use super::*;

impl Zugluft {
    /// The expanded tuning section: calibration status plus the six
    /// editable settings, start/stop pre-filled from calibration.
    pub(super) fn render_tuning(
        &self,
        key: FanKey,
        fan: &FanStatus,
        cx: &mut Context<Self>,
    ) -> Div {
        let settings = self.fan_settings(key, fan);

        let cal_note = match (fan.max_rpm, fan.min_percent.filter(|min| *min > 0.5)) {
            (Some(max), Some(min)) => format!("calibrated · min {min:.0}% · max {max:.0} rpm"),
            (Some(max), None) => format!("calibrated · max {max:.0} rpm"),
            (None, _) => "not calibrated — run Calibrate to measure start/stop".to_string(),
        };

        // (display, dimmed, edit seed) per field. Start/stop fall back to
        // the calibrated values, shown dimmed until overridden.
        let rate = |value: Option<f32>| match value {
            Some(v) => (fmt_setting(v), false, fmt_setting(v)),
            None => ("instant".to_string(), true, String::new()),
        };
        let cal = |user: Option<f32>, calibrated: Option<f32>| match (user, calibrated) {
            (Some(v), _) => (fmt_setting(v), false, fmt_setting(v)),
            (None, Some(v)) => (format!("{v:.0}"), true, String::new()),
            (None, None) => ("—".to_string(), true, String::new()),
        };
        let plain = |v: f32| (fmt_setting(v), false, fmt_setting(v));

        let field = |this: &Self,
                     fieldkind: SettingField,
                     label: &'static str,
                     unit: &'static str,
                     (display, dim, seed): (String, bool, String),
                     cx: &mut Context<Self>| {
            this.render_setting_field(key, fieldkind, label, unit, display, dim, seed, cx)
        };

        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .p_1p5()
            .rounded_md()
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(BORDER))
            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child(cal_note))
            .child(
                div()
                    .flex()
                    .gap_1p5()
                    .child(field(
                        self,
                        SettingField::StepUp,
                        "Step up",
                        "%/s",
                        rate(settings.step_up),
                        cx,
                    ))
                    .child(field(
                        self,
                        SettingField::StepDown,
                        "Step down",
                        "%/s",
                        rate(settings.step_down),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_1p5()
                    .child(field(
                        self,
                        SettingField::Start,
                        "Start",
                        "%",
                        cal(settings.start_percent, fan.start_percent),
                        cx,
                    ))
                    .child(field(
                        self,
                        SettingField::Stop,
                        "Stop",
                        "%",
                        cal(settings.stop_percent, fan.stop_percent),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_1p5()
                    .child(field(
                        self,
                        SettingField::Offset,
                        "Offset",
                        "%",
                        plain(settings.offset),
                        cx,
                    ))
                    .child(field(
                        self,
                        SettingField::Minimum,
                        "Minimum",
                        "%",
                        plain(settings.minimum_percent),
                        cx,
                    )),
            )
    }

    /// One labelled tuning value; clicking it opens the inline editor.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_setting_field(
        &self,
        key: FanKey,
        field: SettingField,
        label: &'static str,
        unit: &'static str,
        display: String,
        dim: bool,
        seed: String,
        cx: &mut Context<Self>,
    ) -> Div {
        let fan_id = key.0 * 64 + key.1;
        let editing = self
            .editing
            .as_ref()
            .filter(|edit| edit.key == key && edit.field == field);

        let value_box: Div = if let Some(edit) = editing {
            div()
                .flex()
                .items_center()
                .h(px(20.))
                .px(px(5.))
                .rounded_md()
                .bg(rgb(TRACK))
                .border_1()
                .border_color(rgb(FILL_MANUAL))
                // Clicking inside the active field keeps it open; clicking
                // away (handled at the root) commits it.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .text_xs()
                        .font_family(FONT_MONO)
                        .child(self.render_text_edit_contents(&edit.input, 10., true)),
                )
                .child(div().text_xs().text_color(rgb(TEXT_DIM)).child(unit))
        } else {
            div().child(
                div()
                    .id(("fld", fan_id * 8 + field.id()))
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(20.))
                    .px_1p5()
                    .rounded_md()
                    .bg(rgb(TRACK))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(FILL_HOVER)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.begin_edit(key, field, seed.clone(), window, cx);
                    }))
                    .child(
                        div()
                            .text_xs()
                            .font_family(FONT_MONO)
                            .text_color(rgb(if dim { TEXT_DIM } else { TEXT }))
                            .child(display),
                    )
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child(unit)),
            )
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_0p5()
            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child(label))
            .child(value_box)
    }

    /// One chip: a wrapping grid of fan cards (live temperatures have their
    /// own tab; curves their own section below).
    pub(super) fn render_chip(
        &self,
        ci: usize,
        info: &ChipInfo,
        snapshot: Option<&ChipSnapshot>,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let mut cards = Vec::new();
        if let Some(snapshot) = snapshot {
            for (fi, fan) in snapshot.fans.iter().enumerate() {
                // Headers that are disabled and have no control are noise.
                if fan.rpm.is_none() && fan.duty.is_none() {
                    continue;
                }
                if self.names.is_hidden(&info.name, &format!("fan{}", fi + 1)) {
                    continue;
                }
                let name = self.names.fan_label(&info.name, fi);
                cards.push(self.render_fan_card((ci, fi), &info.name, name, fan, cx));
            }
        }

        // Sensor-only devices (CPU, fanless GPUs) live on the Sensors page;
        // a fan section with nothing in it is noise here.
        if cards.is_empty() {
            return None;
        }

        let device_label = self.names.device_label(&info.name);
        let raw_chip = info.name.clone();
        let group: SharedString = format!("fan-chip-{ci}").into();
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .group(group.clone())
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(TEXT_DIM))
                                .child(device_label.clone()),
                        )
                        .child(
                            div()
                                .id(("chip-rename", ci))
                                .flex_none()
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    cx.stop_propagation();
                                    this.begin_device_rename(
                                        raw_chip.clone(),
                                        device_label.clone(),
                                        window,
                                        cx,
                                    );
                                }))
                                .child(
                                    svg()
                                        .path("icons/pencil.svg")
                                        .w(px(12.))
                                        .h(px(12.))
                                        .text_color(gpui::transparent_black())
                                        .group_hover(group, |s| s.text_color(rgb(TEXT_DIM)))
                                        .hover(|s| s.text_color(rgb(TEXT))),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_start()
                        .gap_2()
                        .children(cards),
                ),
        )
    }
}
