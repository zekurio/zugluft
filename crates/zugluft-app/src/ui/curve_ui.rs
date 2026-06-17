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
        let color = self.curve_color(&def.id, index);
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

            popup_menu(
                point(px(20.), px(24.)),
                Corner::TopRight,
                div()
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

    /// Color picker for a curve's line, embedded in the edit dialog. Each
    /// swatch applies (and persists) immediately; the current override is
    /// highlighted, with a reset back to the palette default.
    pub(super) fn render_curve_color_field(
        &self,
        def: &CurveDef,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let selected = self.curve_color(&def.id, index);
        let swatches = SENSOR_COLORS.iter().enumerate().map(|(i, &color)| {
            let id = def.id.clone();
            div()
                .id(("curve-color", index * 32 + i))
                .p(px(2.))
                .rounded_md()
                .border_1()
                .border_color(rgb(if selected == color { TEXT } else { PANEL }))
                .cursor_pointer()
                .hover(|s| s.border_color(rgb(TEXT_DIM)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_curve_color(&id, color, cx);
                }))
                .child(div().w(px(20.)).h(px(20.)).rounded(px(3.)).bg(rgb(color)))
        });

        let reset_id = def.id.clone();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Color"))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id(("curve-color-reset", index))
                            .text_xs()
                            .text_color(rgb(TEXT_DIM))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.reset_curve_color(&reset_id, cx);
                            }))
                            .child("Reset"),
                    ),
            )
            .child(div().flex().flex_wrap().gap_1().children(swatches))
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
        let name_input = self
            .curve_name_edit
            .as_ref()
            .filter(|(edit_id, _)| edit_id == &def.id)
            .map(|(_, input)| input.clone())
            .unwrap_or_else(|| TextEdit::new(def.name.clone()));

        let graph_kind = matches!(def.kind.sanitized(), CurveKind::Graph { .. });
        let name_field = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Name"))
            .child(self.render_dialog_text_field(&name_input, true));
        let source_field = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Source"))
            .child(self.render_source_dropdown(&def, chips, snapshots, customs, cx));

        let body =
            if graph_kind {
                div()
                    .w_full()
                    .min_h(px(0.))
                    .h(px(760.))
                    .flex()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .rounded_md()
                            .overflow_hidden()
                            .child(self.render_curve_editor_graph(
                                index, &def, chips, snapshots, customs, cx,
                            )),
                    )
                    .child(
                        div()
                            .id("curve-dialog-side-scroll")
                            .w(px(300.))
                            .flex_none()
                            .min_w(px(0.))
                            .h_full()
                            .border_l_1()
                            .border_color(rgb(BORDER))
                            .pl_4()
                            .pr_1()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .overflow_y_scroll()
                            .child(name_field)
                            .child(source_field)
                            .child(self.render_curve_color_field(&def, index, cx))
                            .child(self.render_curve_side_panel(&def, index, cx)),
                    )
            } else {
                div()
                    .w_full()
                    .pt_1()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(name_field)
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_end()
                            .gap_2()
                            .child(source_field.flex_1().min_w(px(0.))),
                    )
                    .child(self.render_curve_color_field(&def, index, cx))
                    .child(self.render_curve_side_panel(&def, index, cx))
            };

        let panel_width = if graph_kind { px(1120.) } else { px(620.) };
        let panel = self
            .modal_panel("curve-dialog", panel_width, cx)
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
            .child(body);

        Some(self.modal_backdrop(panel, cx, |this, cx| {
            this.close_curve_dialog(cx);
        }))
    }
}
