use super::curve_helpers::{curve_function_label, curve_kind_label};
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
        let header: Div = div()
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
            .child(div().flex_1())
            .child(self.curve_action_menu(index, def, cx));

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
            .bg(rgb(BG))
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

    pub(super) fn curve_action_menu(
        &self,
        index: usize,
        def: &CurveDef,
        cx: &mut Context<Self>,
    ) -> Div {
        let dropdown = Dropdown::CurveActions {
            curve: def.id.clone(),
        };
        let open = self.open_dropdown.as_ref() == Some(&dropdown);
        let curve_id = def.id.clone();

        let menu = open.then(|| {
            let edit_id = curve_id.clone();
            let delete_id = curve_id.clone();

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
                            .id(("curve-menu-edit", index))
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
                                this.open_curve_dialog(edit_id.clone(), cx);
                            }))
                            .child(self.menu_icon("icons/pencil.svg", TEXT_DIM))
                            .child(self.menu_label("Edit", TEXT)),
                    )
                    .child(
                        div()
                            .id(("curve-menu-delete", index))
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
                                this.confirm_delete = Some(ConfirmDelete::Curve(delete_id.clone()));
                                cx.notify();
                            }))
                            .child(self.menu_icon("icons/trash.svg", ERROR))
                            .child(self.menu_label("Delete", ERROR)),
                    ),
            )
        });

        div().relative().flex_none().children(menu).child(
            div()
                .id(("curve-actions", index))
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

    /// The modal curve editor for name, source, and curve parameters.
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
        let input = def.source.resolve(chips, snapshots, customs);
        let output = input.and_then(|input| def.kind.evaluate(input));
        let live_text = match (input, output) {
            (Some(input), Some(output)) => format!("{input:.1} °C → {output:.0} %"),
            _ => "source unavailable".to_string(),
        };

        let name_input = self
            .curve_name_edit
            .as_ref()
            .filter(|(edit_id, _)| edit_id == &def.id)
            .map(|(_, input)| input.clone())
            .unwrap_or_else(|| TextEdit::new(def.name.clone()));

        let panel = self
            .modal_panel("curve-dialog", px(620.), cx)
            .overflow_y_scroll()
            .gap_3()
            .p_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().font_weight(FontWeight::MEDIUM).child("Edit curve"))
                    .child(div().flex_1())
                    .child(div().flex_none().child(self.button(
                        "curve-dialog-done",
                        "Done",
                        cx,
                        |this, cx| {
                            this.close_curve_dialog(cx);
                        },
                    ))),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Name"))
                    .child(self.render_dialog_text_field(&name_input, true)),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_end()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Source"))
                            .child(
                                self.render_source_dropdown(&def, chips, snapshots, customs, cx),
                            ),
                    )
                    .child(
                        div()
                            .w(px(128.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Live"))
                            .child(
                                div()
                                    .h(px(30.))
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .px_2()
                                    .rounded_md()
                                    .bg(rgb(TRACK))
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .text_xs()
                                    .font_family(FONT_MONO)
                                    .text_color(rgb(TEXT))
                                    .truncate()
                                    .child(live_text),
                            ),
                    ),
            )
            .child(
                div().h(px(230.)).flex_none().child(
                    self.render_curve_editor_graph(index, &def, chips, snapshots, customs, cx),
                ),
            )
            .child(
                div()
                    .pt_1()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(self.render_curve_side_panel(&def, index, cx)),
            );

        Some(self.modal_backdrop(panel, cx, |this, cx| {
            this.close_curve_dialog(cx);
        }))
    }
}
