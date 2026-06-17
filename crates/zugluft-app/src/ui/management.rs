use super::curve_helpers::fmt_axis_value;
use super::*;

impl Zugluft {
    pub(super) fn render_curve_editor_graph(
        &self,
        index: usize,
        def: &CurveDef,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Div {
        let color = self.curve_color(&def.id, index);
        let input = def.source.resolve(chips, snapshots, customs);
        let output = input.and_then(|input| def.kind.evaluate(input));
        let data = CurveEditorData {
            kind: def.kind.clone(),
            window: def.window,
            color,
            live: input.zip(output),
            drag: self.curve_drag,
        };
        let curve_window = def.window.sanitized();
        let y_axis = div()
            .w(px(58.))
            .h_full()
            // Match the plot's inset so the 0 %/100 % labels line up with the
            // gridlines instead of the box edges.
            .py(px(CURVE_PLOT_INSET))
            .flex()
            .flex_col()
            .justify_between()
            .items_end()
            .children((0..=10).map(|i| {
                let value = curve_window.duty_max - curve_window.duty_span() * (i as f32 / 10.0);
                div()
                    .text_xs()
                    .font_family(FONT_MONO)
                    .text_color(rgb(TEXT_DIM))
                    .child(format!("{} %", fmt_axis_value(value)))
            }));
        let drag_readout = self.curve_drag.and_then(|index| {
            let bounds = (*self.curve_bounds.borrow())?;
            let CurveKind::Graph { points } = &def.kind else {
                return None;
            };
            let &(temp, percent) = points.get(index)?;
            // `bounds` is the inset plot region; the readout is positioned in
            // the full canvas area, so offset by the inset to follow the point.
            let x =
                CURVE_PLOT_INSET + f32::from(bounds.size.width) * curve_window.temp_fraction(temp);
            let y = CURVE_PLOT_INSET
                + f32::from(bounds.size.height) * (1.0 - curve_window.duty_fraction(percent));
            const W: f32 = 92.0;
            let left = (x - W / 2.0).clamp(0.0, (f32::from(bounds.size.width) - W).max(0.0));
            let top = if y - 30.0 < 0.0 { y + 16.0 } else { y - 30.0 };
            Some(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(W))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .px_2()
                            .py(px(2.))
                            .rounded_md()
                            .bg(rgb(PANEL))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .shadow(subtle_shadow())
                            .text_xs()
                            .font_family(FONT_MONO)
                            .text_color(rgb(TEXT))
                            .child(format!("{temp:.0} C -> {percent:.0} %")),
                    ),
            )
        });
        let curve_bounds = self.curve_bounds.clone();
        let page_curve_id = def.id.clone();
        let graph_area = div()
            .size_full()
            .min_h(px(0.))
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .min_w(px(0.))
                    .flex()
                    .gap_2()
                    .child(y_axis)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .relative()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                    this.selected_curve = Some(page_curve_id.clone());
                                    this.curve_editor_down(event, cx);
                                }),
                            )
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.end_curve_drag(cx);
                                }),
                            )
                            .child(
                                canvas(
                                    move |bounds, _, _| {
                                        // Hit-testing uses the same inset plot
                                        // region the editor draws into.
                                        *curve_bounds.borrow_mut() =
                                            Some(bounds.inset(px(CURVE_PLOT_INSET)));
                                    },
                                    move |bounds, _, window, _| {
                                        draw_curve_editor(bounds, &data, window);
                                    },
                                )
                                .size_full(),
                            )
                            .children(drag_readout),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap_2()
                    .child(div().w(px(58.)))
                    .child(
                        div()
                            .flex_1()
                            // Match the plot's horizontal inset so the temp
                            // labels line up with the gridlines.
                            .px(px(CURVE_PLOT_INSET))
                            .flex()
                            .justify_between()
                            .children((0..=10).map(|i| {
                                let value = curve_window.temp_min
                                    + curve_window.temp_span() * (i as f32 / 10.0);
                                div()
                                    .text_xs()
                                    .font_family(FONT_MONO)
                                    .text_color(rgb(TEXT_DIM))
                                    .child(format!("{} C", fmt_axis_value(value)))
                            })),
                    ),
            );

        graph_area
    }

    pub(super) fn render_fans_page(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .gap_2()
            .p_2()
            .child(self.render_fan_detail(chips, snapshots))
            .child(self.render_fan_list_panel(chips, snapshots, cx))
    }

    fn render_fan_detail(&self, chips: &[ChipInfo], snapshots: &[ChipSnapshot]) -> Div {
        let graph = self.fan_graph_data(chips, snapshots);
        div()
            .flex_1()
            .min_h(px(0.))
            .min_w(px(0.))
            .flex()
            .child(self.render_sensor_graph(graph))
    }

    fn fan_graph_data(&self, chips: &[ChipInfo], snapshots: &[ChipSnapshot]) -> GraphData {
        let mut fans = self
            .sensor_readings(chips, snapshots, &[])
            .into_iter()
            .filter(|sensor| sensor.key.kind == SensorKind::FanRpm && sensor.value > 0.0)
            .collect::<Vec<_>>();
        for fan in &mut fans {
            fan.enabled = true;
        }

        let mut graph = self.graph_data(&fans);
        if graph.series.is_empty() {
            let unit = self.fan_display_unit();
            let (min, max) = unit.default_range();
            graph.axes = vec![AxisData { unit, min, max }];
        }
        graph
    }

    fn render_fan_list_panel(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut panel = div()
            .id("fans-panel")
            .w(px(260.))
            .h_full()
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
                    .child("Fans"),
            )
            .child(div().px_1().pb_1().child(self.icon_button(
                "fans-page-calibrate",
                "icons/fan.svg",
                "Calibrate all",
                cx,
                |this, cx| {
                    let _ = this.tx.send(Request::Calibrate);
                    cx.notify();
                },
            )));

        for (ci, snapshot) in snapshots.iter().enumerate() {
            let Some(chip) = chips.get(ci) else { continue };
            let rows = snapshot
                .fans
                .iter()
                .enumerate()
                .filter(|(fi, fan)| {
                    (fan.rpm.is_some() || fan.duty.is_some())
                        && !self.names.is_hidden(&chip.name, &format!("fan{}", fi + 1))
                })
                .map(|(fi, fan)| self.render_fan_list_row((ci, fi), chip, fan, cx))
                .collect::<Vec<_>>();
            if rows.is_empty() {
                continue;
            }
            panel = panel
                .child(
                    div()
                        .px_1()
                        .pt_2()
                        .pb_0p5()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(TEXT))
                        .child(self.names.device_label(&chip.name)),
                )
                .children(rows);
        }

        panel
    }

    fn render_fan_list_row(
        &self,
        key: FanKey,
        chip: &ChipInfo,
        fan: &FanStatus,
        cx: &mut Context<Self>,
    ) -> Div {
        let name = self.names.fan_label(&chip.name, key.1);
        let speed = fan
            .rpm
            .map(|rpm| {
                let max = fan.max_rpm.unwrap_or_else(|| {
                    self.fan_max_rpm(SensorKey {
                        kind: SensorKind::FanRpm,
                        chip: key.0,
                        index: key.1,
                    })
                });
                self.fan_display_unit()
                    .format_value(self.convert_fan(rpm, max))
            })
            .unwrap_or_else(|| "--".to_string());
        let duty = self
            .fan_curve(key, fan)
            .and_then(|id| self.names.curves().iter().find(|def| def.id == id))
            .map(|def| def.name.clone())
            .or_else(|| {
                let manual_percent = if let Some(FanDuty::Manual { percent }) = fan.duty {
                    Some(percent)
                } else {
                    None
                };
                fan.target_percent
                    .or(manual_percent)
                    .map(|percent| format!("{percent:.0} %"))
            })
            .unwrap_or_else(|| "Auto".to_string());
        let select_key = key;
        div().child(
            div()
                .id(("fan-list-row", key.0 * 64 + key.1))
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_md()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(PANEL))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.expanded.insert(select_key);
                    cx.notify();
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(div().text_sm().truncate().child(name.clone()))
                        .child(
                            div()
                                .text_xs()
                                .font_family(FONT_MONO)
                                .text_color(rgb(TEXT_DIM))
                                .truncate()
                                .child(speed),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .font_family(FONT_MONO)
                        .text_color(rgb(TEXT_DIM))
                        .truncate()
                        .child(duty),
                )
                .child(self.fan_action_menu(key, &chip.name, name.clone(), cx)),
        )
    }
}
