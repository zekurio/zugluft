use super::curve_helpers::fmt_axis_value;
use super::*;

impl Zugluft {
    pub(super) fn render_curves_page(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Div {
        let curves = self.names.curves();
        let selected = self
            .selected_curve
            .as_deref()
            .and_then(|id| curves.iter().position(|def| def.id == id))
            .or_else(|| (!curves.is_empty()).then_some(0));

        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .gap_2()
            .p_2()
            .child(self.render_curve_detail(selected, chips, snapshots, customs, cx))
            .child(self.render_curve_list_panel(selected, chips, snapshots, customs, cx))
    }

    fn render_curve_detail(
        &self,
        selected: Option<usize>,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Div {
        let Some((index, def)) = selected.and_then(|index| {
            self.names
                .curves()
                .get(index)
                .and_then(|def| self.curve_for_display(&def.id).map(|def| (index, def)))
        }) else {
            return div()
                .flex_1()
                .min_w(px(0.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded_lg()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .shadow(floating_shadow())
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .child(div().text_lg().child("No curves"))
                        .child(self.button("curve-empty-add", "New curve", cx, |this, cx| {
                            this.add_curve_with_kind(
                                CurveKind::Graph {
                                    points: vec![(30.0, 20.0), (50.0, 40.0), (70.0, 100.0)],
                                },
                                cx,
                            );
                        })),
                );
        };

        let color = SENSOR_COLORS[index % SENSOR_COLORS.len()];
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
        let live_text = match (input, output) {
            (Some(input), Some(output)) => format!("{input:.1} C -> {output:.0} %"),
            _ => "--".to_string(),
        };
        let y_axis = div()
            .w(px(44.))
            .h_full()
            .flex()
            .flex_col()
            .justify_between()
            .items_end()
            .children((0..=4).map(|i| {
                let value = curve_window.duty_max - curve_window.duty_span() * (i as f32 / 4.0);
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
            let x = f32::from(bounds.size.width) * curve_window.temp_fraction(temp);
            let y = f32::from(bounds.size.height) * (1.0 - curve_window.duty_fraction(percent));
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
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
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
                            .rounded_lg()
                            .overflow_hidden()
                            .bg(rgb(GRID_CELL))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                    this.selected_curve = Some(page_curve_id.clone());
                                    this.curve_editor_down(event, cx);
                                }),
                            )
                            .child(
                                canvas(
                                    move |bounds, _, _| {
                                        *curve_bounds.borrow_mut() = Some(bounds);
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
                    .child(div().w(px(44.)))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .justify_between()
                            .children((0..=4).map(|i| {
                                let value = curve_window.temp_min
                                    + curve_window.temp_span() * (i as f32 / 4.0);
                                div()
                                    .text_xs()
                                    .font_family(FONT_MONO)
                                    .text_color(rgb(TEXT_DIM))
                                    .child(format!("{} C", fmt_axis_value(value)))
                            })),
                    ),
            );

        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .rounded_lg()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(floating_shadow())
            .child(graph_area)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Source"))
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(FONT_MONO)
                                    .child(self.source_label(&def.source)),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .font_family(FONT_MONO)
                            .text_color(rgb(TEXT_DIM))
                            .child(live_text),
                    ),
            )
    }

    fn render_curve_list_panel(
        &self,
        selected: Option<usize>,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        div()
            .id("curves-panel")
            .w(px(320.))
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
                    .child("Curves"),
            )
            .children(self.names.curves().iter().enumerate().map(|(index, def)| {
                let input = def.source.resolve(chips, snapshots, customs);
                let output = input.and_then(|input| def.kind.evaluate(input));
                let live_text = match (input, output) {
                    (Some(input), Some(output)) => format!("{input:.0} C -> {output:.0} %"),
                    _ => "--".to_string(),
                };
                self.render_curve_list_row(index, def, selected == Some(index), live_text, cx)
            }))
            .child(
                div()
                    .pt_1p5()
                    .mt_1()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(div().pt_1p5().child(self.button(
                        "curve-page-add",
                        "New curve",
                        cx,
                        |this, cx| {
                            this.add_curve_with_kind(
                                CurveKind::Graph {
                                    points: vec![(30.0, 20.0), (50.0, 40.0), (70.0, 100.0)],
                                },
                                cx,
                            );
                        },
                    ))),
            )
    }

    fn render_curve_list_row(
        &self,
        index: usize,
        def: &CurveDef,
        selected: bool,
        live_text: String,
        cx: &mut Context<Self>,
    ) -> Div {
        let color = SENSOR_COLORS[index % SENSOR_COLORS.len()];
        let select_id = def.id.clone();
        let edit_id = def.id.clone();
        let delete_id = def.id.clone();

        div().child(
            div()
                .id(("curve-list-row", index))
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_md()
                .border_1()
                .border_color(rgb(if selected { FILL_MANUAL } else { BORDER }))
                .bg(rgb(if selected { TRACK } else { PANEL }))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.selected_curve = Some(select_id.clone());
                    cx.notify();
                }))
                .child(
                    div()
                        .w(px(8.))
                        .h(px(8.))
                        .flex_none()
                        .rounded_full()
                        .bg(rgb(color)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(div().text_sm().truncate().child(def.name.clone()))
                        .child(
                            div()
                                .text_xs()
                                .font_family(FONT_MONO)
                                .text_color(rgb(TEXT_DIM))
                                .truncate()
                                .child(live_text),
                        ),
                )
                .child(
                    div()
                        .id(("curve-list-edit", index))
                        .flex_none()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.open_curve_dialog(edit_id.clone(), cx);
                        }))
                        .child(
                            svg()
                                .path("icons/pencil.svg")
                                .w(px(13.))
                                .h(px(13.))
                                .text_color(rgb(TEXT_DIM))
                                .hover(|s| s.text_color(rgb(TEXT))),
                        ),
                )
                .child(
                    div()
                        .id(("curve-list-delete", index))
                        .flex_none()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.confirm_delete = Some(ConfirmDelete::Curve(delete_id.clone()));
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .path("icons/trash.svg")
                                .w(px(13.))
                                .h(px(13.))
                                .text_color(rgb(TEXT_DIM))
                                .hover(|s| s.text_color(rgb(ERROR))),
                        ),
                ),
        )
    }

    pub(super) fn render_fans_page(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        cx: &mut Context<Self>,
    ) -> Div {
        let selected = self
            .selected_fan
            .filter(|key| self.visible_fan(chips, snapshots, *key).is_some())
            .or_else(|| self.first_visible_fan(chips, snapshots));

        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .gap_2()
            .p_2()
            .child(self.render_fan_detail(chips, snapshots))
            .child(self.render_fan_list_panel(selected, chips, snapshots, cx))
    }

    fn first_visible_fan(&self, chips: &[ChipInfo], snapshots: &[ChipSnapshot]) -> Option<FanKey> {
        snapshots.iter().enumerate().find_map(|(ci, snapshot)| {
            let chip = chips.get(ci)?;
            snapshot.fans.iter().enumerate().find_map(|(fi, fan)| {
                ((fan.rpm.is_some() || fan.duty.is_some())
                    && !self.names.is_hidden(&chip.name, &format!("fan{}", fi + 1)))
                .then_some((ci, fi))
            })
        })
    }

    fn visible_fan<'a>(
        &self,
        chips: &'a [ChipInfo],
        snapshots: &'a [ChipSnapshot],
        key: FanKey,
    ) -> Option<(&'a ChipInfo, &'a FanStatus)> {
        let chip = chips.get(key.0)?;
        if self
            .names
            .is_hidden(&chip.name, &format!("fan{}", key.1 + 1))
        {
            return None;
        }
        let fan = snapshots.get(key.0)?.fans.get(key.1)?;
        (fan.rpm.is_some() || fan.duty.is_some()).then_some((chip, fan))
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
        selected: Option<FanKey>,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut panel = div()
            .id("fans-panel")
            .w(px(320.))
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
                .map(|(fi, fan)| {
                    self.render_fan_list_row((ci, fi), chip, fan, selected == Some((ci, fi)), cx)
                })
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
        selected: bool,
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
        let rename_key = SensorKey {
            kind: SensorKind::FanRpm,
            chip: key.0,
            index: key.1,
        };
        let chip_name = chip.name.clone();
        let label = name.clone();

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
                .border_color(rgb(if selected { FILL_MANUAL } else { BORDER }))
                .bg(rgb(if selected { TRACK } else { PANEL }))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.selected_fan = Some(select_key);
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
                        .child(div().text_sm().truncate().child(name))
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
                .child(
                    div()
                        .id(("fan-list-rename", key.0 * 64 + key.1))
                        .flex_none()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.begin_rename(
                                rename_key,
                                label.clone(),
                                Some((chip_name.clone(), channel_key(rename_key))),
                                window,
                                cx,
                            );
                        }))
                        .child(
                            svg()
                                .path("icons/pencil.svg")
                                .w(px(13.))
                                .h(px(13.))
                                .text_color(rgb(TEXT_DIM))
                                .hover(|s| s.text_color(rgb(TEXT))),
                        ),
                ),
        )
    }
}
