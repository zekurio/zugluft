use super::curve_helpers::{curve_function_label, curve_kind_label, fmt_axis_value};
use super::*;

impl Zugluft {
    /// One curve card: name, hover edit/delete actions, a shape preview,
    /// source summary, and the live evaluation.
    pub(super) fn render_curve_card(
        &self,
        index: usize,
        def: &CurveDef,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Div {
        let color = SENSOR_COLORS[index % SENSOR_COLORS.len()];
        let input = def.source.resolve(chips, snapshots, customs);
        let output = input.and_then(|input| def.kind.evaluate(input));

        let dot = div()
            .w(px(8.))
            .h(px(8.))
            .flex_none()
            .rounded_full()
            .bg(rgb(color));
        let header: Div = {
            let group: SharedString = format!("curve-card-{index}").into();
            let edit_id = def.id.clone();
            let delete_id = def.id.clone();
            div()
                .group(group.clone())
                .flex()
                .items_center()
                .gap_2()
                .h(px(22.))
                .child(dot)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(def.name.clone()),
                )
                .child(
                    div()
                        .id(("curve-edit", index))
                        .flex_none()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.open_curve_dialog(edit_id.clone(), cx);
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
                )
                .child(div().flex_1())
                .child(
                    div()
                        .id(("curve-delete", index))
                        .flex_none()
                        .px_1()
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
                )
        };

        let data = CurveEditorData {
            kind: def.kind.clone(),
            window: def.window,
            color,
            live: input.zip(output),
            drag: None,
        };
        let preview = div()
            .id(("curve-preview", index))
            .h(px(72.))
            .w_full()
            .rounded_md()
            .overflow_hidden()
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        draw_curve_preview(bounds, &data, window);
                    },
                )
                .size_full(),
            );

        let live_text = match (input, output) {
            (Some(input), Some(output)) => format!("{input:.1} °C → {output:.0} %"),
            _ => "—".to_string(),
        };
        let function_text = format!(
            "{} · {}",
            curve_kind_label(&def.kind),
            curve_function_label(def.primary_function())
        );
        let footer = div()
            .flex()
            .items_center()
            .child(
                div()
                    .text_xs()
                    .font_family(FONT_MONO)
                    .text_color(rgb(TEXT_DIM))
                    .child(live_text),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .font_family(FONT_MONO)
                    .text_color(rgb(TEXT_DIM))
                    .truncate()
                    .child(function_text),
            );

        div()
            .w(px(268.))
            .flex()
            .flex_col()
            .gap_1p5()
            .p_2p5()
            .rounded_lg()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(subtle_shadow())
            .child(header)
            .child(preview)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Source"))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .child(self.source_label(&def.source)),
                    ),
            )
            .child(footer)
    }

    /// The modal curve editor: the interactive plot plus everything that
    /// doesn't fit inline on the card.
    pub(super) fn render_curve_dialog(
        &self,
        id: &str,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let def = self.curve_for_display(id)?;
        let index = self
            .names
            .curves()
            .iter()
            .position(|other| other.id == def.id)
            .unwrap_or(0);
        let color = SENSOR_COLORS[index % SENSOR_COLORS.len()];
        let input = def.source.resolve(chips, snapshots, customs);
        let output = input.and_then(|input| def.kind.evaluate(input));
        let live_text = match (input, output) {
            (Some(input), Some(output)) => format!("{input:.1} °C → {output:.0} %"),
            _ => "source unavailable".to_string(),
        };

        let data = CurveEditorData {
            kind: def.kind.clone(),
            window: def.window,
            color,
            live: input.zip(output),
            drag: self.curve_drag,
        };

        let curve_window = def.window.sanitized();
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
                            .child(format!("{temp:.0} °C → {percent:.0} %")),
                    ),
            )
        });

        let curve_bounds = self.curve_bounds.clone();
        let plot = div()
            .flex_1()
            .h_full()
            .relative()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
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
            .children(drag_readout);

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
        let x_axis = div().flex().gap_2().child(div().w(px(44.))).child(
            div()
                .flex_1()
                .flex()
                .justify_between()
                .children((0..=4).map(|i| {
                    let value = curve_window.temp_min + curve_window.temp_span() * (i as f32 / 4.0);
                    div()
                        .text_xs()
                        .font_family(FONT_MONO)
                        .text_color(rgb(TEXT_DIM))
                        .child(format!("{} °C", fmt_axis_value(value)))
                })),
        );
        let name_input = self
            .curve_name_edit
            .as_ref()
            .filter(|(edit_id, _)| edit_id == &def.id)
            .map(|(_, input)| input.clone())
            .unwrap_or_else(|| TextEdit::new(def.name.clone()));
        let graph_area = div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
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
                    .child(plot),
            )
            .child(x_axis);
        let body = div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .gap_4()
            .child(graph_area)
            .child(self.render_curve_side_panel(&def, index, cx));

        let panel = div()
            .w(px(980.))
            .h(px(640.))
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .rounded_lg()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(floating_shadow())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap_4()
                    .child(
                        div()
                            .w(px(260.))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Name"))
                            .child(self.render_dialog_text_field(&name_input, true)),
                    )
                    .child(
                        div()
                            .w(px(260.))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Source"))
                            .child(
                                self.render_source_dropdown(&def, chips, snapshots, customs, cx),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_end()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Live"))
                            .child(
                                div()
                                    .h(px(30.))
                                    .flex()
                                    .items_center()
                                    .text_xs()
                                    .font_family(FONT_MONO)
                                    .text_color(rgb(TEXT_DIM))
                                    .child(live_text),
                            ),
                    ),
            )
            .child(body)
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child(
                        if matches!(&def.kind, CurveKind::Graph { .. }) {
                            "click: add point · drag: move · double-click: remove"
                        } else {
                            ""
                        },
                    ))
                    .child(div().flex_1())
                    .child(self.button("curve-dialog-done", "Done", cx, |this, cx| {
                        this.close_curve_dialog(cx);
                    })),
            );

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0.0, 0.0, 0.0, 0.55))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        this.close_curve_dialog(cx);
                    }),
                )
                .child(panel),
        )
    }
}
