use super::*;

impl Zugluft {
    pub(super) fn set_channel_color(
        &mut self,
        chip: &str,
        channel: &str,
        color: u32,
        cx: &mut Context<Self>,
    ) {
        config::set_graph_color(chip, channel, Some(&format!("#{color:06x}")));
        self.reload_config(cx);
    }

    pub(super) fn set_channel_style(
        &mut self,
        chip: &str,
        channel: &str,
        style: LineStyle,
        cx: &mut Context<Self>,
    ) {
        config::set_graph_style(chip, channel, Some(style.name()));
        self.reload_config(cx);
    }

    pub(super) fn reset_channel_appearance(
        &mut self,
        chip: &str,
        channel: &str,
        cx: &mut Context<Self>,
    ) {
        config::set_graph_color(chip, channel, None);
        config::set_graph_style(chip, channel, None);
        self.reload_config(cx);
    }

    /// The color palette + line-style picker for one graph line, embedded in
    /// the Edit dialog. Each choice applies immediately (and persists); the
    /// current override is highlighted, with a reset back to the auto style.
    pub(super) fn render_appearance_controls(
        &self,
        chip: &str,
        channel: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let selected_color = self.names.graph_color(chip, channel);
        let selected_style = self
            .names
            .graph_style(chip, channel)
            .and_then(parse_line_style);

        let swatches = SENSOR_COLORS.iter().enumerate().map(|(i, &color)| {
            let chip = chip.to_string();
            let channel = channel.to_string();
            let selected = selected_color == Some(color);
            div()
                .id(("appearance-color", i))
                .p(px(2.))
                .rounded_md()
                .border_1()
                .border_color(rgb(if selected { TEXT } else { PANEL }))
                .cursor_pointer()
                .hover(|s| s.border_color(rgb(TEXT_DIM)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_channel_color(&chip, &channel, color, cx);
                }))
                .child(div().w(px(20.)).h(px(20.)).rounded(px(3.)).bg(rgb(color)))
        });

        let styles = LINE_STYLES.iter().enumerate().map(|(i, &style)| {
            let chip = chip.to_string();
            let channel = channel.to_string();
            let selected = selected_style.unwrap_or(LineStyle::Solid) == style;
            div()
                .id(("appearance-style", i))
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(if selected { TRACK } else { PANEL }))
                .border_1()
                .border_color(rgb(if selected { FILL_MANUAL } else { BORDER }))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_channel_style(&chip, &channel, style, cx);
                }))
                .child(self.style_sample(style))
                .child(div().text_xs().text_color(rgb(TEXT)).child(style.label()))
        });

        let reset_chip = chip.to_string();
        let reset_channel = channel.to_string();
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Color"))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("appearance-reset")
                            .text_xs()
                            .text_color(rgb(TEXT_DIM))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.reset_channel_appearance(&reset_chip, &reset_channel, cx);
                            }))
                            .child("Reset"),
                    ),
            )
            .child(div().flex().flex_wrap().gap_1().children(swatches))
            .child(
                div()
                    .mt_1()
                    .text_xs()
                    .text_color(rgb(TEXT_DIM))
                    .child("Line"),
            )
            .child(div().flex().gap_1p5().children(styles))
    }

    /// A short line drawn in the given style, for the style buttons.
    fn style_sample(&self, style: LineStyle) -> Div {
        div().w(px(30.)).h(px(12.)).flex_none().child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let y = bounds.origin.y + bounds.size.height / 2.;
                    let mut builder =
                        super::graph::apply_line_style(PathBuilder::stroke(px(2.)), style);
                    builder.move_to(point(bounds.origin.x, y));
                    builder.line_to(point(bounds.origin.x + bounds.size.width, y));
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, rgb(TEXT));
                    }
                },
            )
            .size_full(),
        )
    }
}
