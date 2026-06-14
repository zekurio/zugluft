use super::*;

impl Zugluft {
    /// One fan/port card: editable name, live readout, mode switch, target
    /// slider, and a collapsible tuning section.
    pub(super) fn render_fan_card(
        &self,
        key: FanKey,
        chip_name: &str,
        name: String,
        fan: &FanStatus,
        cx: &mut Context<Self>,
    ) -> Div {
        let fan_id = key.0 * 64 + key.1;
        let has_control = fan.duty.is_some();
        let pending = self.pending.get(&key).copied();

        // What the card shows: optimistic value while dragging, otherwise
        // the last target the service accepted. Older/uncontrolled states
        // fall back to the hardware command readback.
        let (is_manual, percent) = match (pending, fan.duty) {
            (Some(p), _) => (true, Some(p)),
            (None, Some(_)) if fan.target_percent.is_some() => (true, fan.target_percent),
            (None, Some(FanDuty::Manual { percent })) => (true, Some(percent)),
            _ => (false, None),
        };
        let in_curve = self.fan_curve(key, fan).is_some();

        // Header: name, status badge, and a compact action menu.
        let rename_key = SensorKey {
            kind: SensorKind::FanRpm,
            chip: key.0,
            index: key.1,
        };
        let header: Div = {
            let badge: Div = if !has_control {
                div().text_xs().text_color(rgb(TEXT_DIM)).child("sensor")
            } else if fan.max_rpm.is_some() {
                div()
                    .text_xs()
                    .text_color(rgb(ACCENT_OK))
                    .child("calibrated")
            } else {
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_DIM))
                    .child("uncalibrated")
            };
            div()
                .flex()
                .items_center()
                .gap_2()
                .h(px(22.))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(name.clone()),
                )
                .child(div().flex_1())
                .child(badge)
                .child(self.fan_action_menu(key, chip_name, name.clone(), cx))
        };

        // The speed readout follows the fan-unit setting (U/min, or % of
        // the calibrated max). The unit suffix shares the number's font and
        // size — only dimmed — so both sit on the same baseline; a smaller
        // suffix renders visibly sunken next to the mono digits.
        let speed_unit = self.fan_display_unit();
        let speed_text = fan.rpm.map_or_else(
            || "—".to_string(),
            |r| {
                let max_rpm = fan.max_rpm.unwrap_or_else(|| self.fan_max_rpm(rename_key));
                format!("{:.0}", self.convert_fan(r, max_rpm))
            },
        );
        let duty_text = match percent {
            Some(p) => format!("{p:.0} %"),
            None if has_control => "auto".to_string(),
            None => String::new(),
        };
        let readout = div()
            .flex()
            .items_baseline()
            .gap_1()
            .child(div().text_base().font_family(FONT_MONO).child(speed_text))
            .child(
                div()
                    .text_base()
                    .font_family(FONT_MONO)
                    .text_color(rgb(TEXT_DIM))
                    .child(speed_unit.label()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_base()
                    .font_family(FONT_MONO)
                    // Blue marks a curve-driven command, amber a manual one.
                    .text_color(rgb(if in_curve {
                        FILL_MANUAL
                    } else if is_manual {
                        ACCENT_WARN
                    } else {
                        TEXT_DIM
                    }))
                    .child(duty_text),
            );

        let mut card = div()
            .w(px(268.))
            .flex()
            .flex_col()
            .gap_1p5()
            .p_2p5()
            .rounded_lg()
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(subtle_shadow())
            .child(header)
            .child(readout);

        if !has_control {
            return card;
        }

        let has_curves = !self.names.curves().is_empty();
        let curve_segment: Div = if has_curves {
            self.segment(
                ("mode-curve", fan_id),
                "Curve",
                in_curve,
                cx,
                move |this, cx| this.set_curve_mode(key, cx),
            )
        } else {
            // No curves defined yet; the Curves section below explains.
            div().flex_1().child(
                div()
                    .w_full()
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_2()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(TRACK))
                    .bg(rgb(TRACK))
                    .text_xs()
                    .text_color(rgb(TEXT_DIM))
                    .opacity(0.45)
                    .child("Curve"),
            )
        };
        let modes = self.segmented([
            self.segment(
                ("mode-auto", fan_id),
                "UEFI/Firmware",
                !is_manual && !in_curve,
                cx,
                move |this, cx| this.set_auto(key, cx),
            ),
            self.segment(
                ("mode-manual", fan_id),
                "Manual",
                is_manual && !in_curve,
                cx,
                move |this, cx| this.set_manual(key, cx),
            ),
            curve_segment,
        ]);

        // In curve mode the slider gives way to the curve picker; dragging
        // the slider switches to manual implicitly.
        let control: Div = if let Some(assigned) = self.fan_curve(key, fan) {
            let current = self
                .names
                .curves()
                .iter()
                .find(|def| def.id == assigned)
                .map(|def| def.name.clone())
                .unwrap_or_else(|| assigned.clone());
            let mut options: Vec<(String, DropdownAction)> = self
                .names
                .curves()
                .iter()
                .map(|def| {
                    let id = def.id.clone();
                    let action: DropdownAction =
                        Rc::new(move |this: &mut Self, cx: &mut Context<Self>| {
                            this.assign_fan(key, Some(id.clone()), cx);
                        });
                    (def.name.clone(), action)
                })
                .collect();
            options.push((
                "None (UEFI/Firmware)".to_string(),
                Rc::new(move |this: &mut Self, cx: &mut Context<Self>| {
                    this.assign_fan(key, None, cx);
                }),
            ));
            self.render_dropdown(
                ("fan-curve", fan_id),
                Dropdown::FanCurve { fan: key },
                current,
                options,
                cx,
            )
        } else {
            let track_bounds = self.track_bounds.clone();
            div()
                .relative()
                .w_full()
                .h(px(16.))
                .rounded_md()
                .bg(rgb(TRACK))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.begin_drag(key, event.position.x, cx);
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(relative((percent.unwrap_or(0.) / 100.0).clamp(0.0, 1.0)))
                        .rounded_md()
                        .bg(rgb(if is_manual { FILL_MANUAL } else { FILL_HOVER })),
                )
                .child(
                    canvas(
                        move |bounds, _, _| {
                            track_bounds.borrow_mut().insert(key, bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
        };

        let expanded = self.expanded.contains(&key);
        let tuning_header = div().child(
            div()
                .id(("tuning", fan_id))
                .flex()
                .items_center()
                .gap_1()
                .px_1()
                .py_0p5()
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.toggle_tuning(key, cx);
                }))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_DIM))
                        .child(if expanded { "▾" } else { "▸" }),
                )
                .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Tuning")),
        );

        card = card
            .child(div().flex().child(modes))
            // Keep the card the same height whether this row holds the target
            // slider (16 px) or the curve picker (22 px): a fixed-height box
            // centers whichever control it shows.
            .child(div().h(px(22.)).flex().items_center().child(control))
            .child(tuning_header);
        if expanded {
            card = card.child(self.render_tuning(key, fan, cx));
        }
        card
    }

    pub(super) fn fan_action_menu(
        &self,
        key: FanKey,
        chip_name: &str,
        label: String,
        cx: &mut Context<Self>,
    ) -> Div {
        let fan_id = key.0 * 64 + key.1;
        let dropdown = Dropdown::FanActions { fan: key };
        let open = self.open_dropdown.as_ref() == Some(&dropdown);
        let pin_item = self.dashboard_fan_item(chip_name, key.1);
        let pinned = self.names.is_dashboard_pinned(&pin_item);
        let chip = chip_name.to_string();
        let rename_key = SensorKey {
            kind: SensorKind::FanRpm,
            chip: key.0,
            index: key.1,
        };
        let channel = channel_key(rename_key);

        let menu = open.then(|| {
            let pin_item = pin_item.clone();
            let edit_chip = chip.clone();
            let edit_channel = channel.clone();
            let edit_label = label.clone();
            let hide_chip = chip.clone();

            deferred(
                div()
                    .absolute()
                    .top(px(24.))
                    .right(px(0.))
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
                            .id(("fan-menu-pin", fan_id))
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
                            .id(("fan-menu-edit", fan_id))
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
                                this.begin_rename(
                                    rename_key,
                                    edit_label.clone(),
                                    Some((edit_chip.clone(), edit_channel.clone())),
                                    window,
                                    cx,
                                );
                            }))
                            .child(self.menu_icon("icons/pencil.svg", TEXT_DIM))
                            .child(self.menu_label("Edit", TEXT)),
                    )
                    .child(
                        div()
                            .id(("fan-menu-hide", fan_id))
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
                                    &format!("fan{}", key.1 + 1),
                                    true,
                                    cx,
                                );
                            }))
                            .child(self.menu_icon("icons/eye-off.svg", TEXT_DIM))
                            .child(self.menu_label("Hide", TEXT)),
                    ),
            )
        });

        div().relative().flex_none().children(menu).child(
            div()
                .id(("fan-actions", fan_id))
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
                .hover(|s| s.bg(rgb(FILL_HOVER)))
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
}
