use super::*;

impl Zugluft {
    pub(super) fn render_controls(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        notes: &[String],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Div {
        let header_controls = [self.icon_button(
            "calibrate-fans-ctl",
            "icons/fan.svg",
            "Calibrate",
            cx,
            |this, cx| {
                let _ = this.tx.send(Request::Calibrate);
                cx.notify();
            },
        )];

        // The page scrolls; without this an overflowing page squeezes the
        // shrinkable parts of the layout instead (and curves + tuning make
        // it overflow easily in short windows).
        div().flex_1().min_h(px(0.)).overflow_hidden().child(
            div()
                .id("controls-scroll")
                .relative()
                .size_full()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_3()
                .p_2()
                .child(
                    div()
                        .absolute()
                        .top(px(10.))
                        .right(px(12.))
                        .children(header_controls),
                )
                .children(
                    chips
                        .iter()
                        .enumerate()
                        .filter_map(|(ci, info)| self.render_chip(ci, info, snapshots.get(ci), cx)),
                )
                .child(self.render_curve_section(chips, snapshots, customs, cx))
                .children((!notes.is_empty()).then(|| {
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(notes.iter().map(|note| {
                            div()
                                .text_xs()
                                .text_color(rgb(TEXT_DIM))
                                .child(format!("· {}", note))
                        }))
                })),
        )
    }

    /// A minimal dropdown: a trigger showing the current value and, while
    /// open, a deferred popup listing the options (deferred so it paints
    /// over everything; a root-level overlay closes it on click-away).
    pub(super) fn render_dropdown(
        &self,
        id: (&'static str, usize),
        dropdown: Dropdown,
        current: String,
        options: Vec<(String, DropdownAction)>,
        cx: &mut Context<Self>,
    ) -> Div {
        self.render_dropdown_sized(id, dropdown, current, options, px(22.), cx)
    }

    pub(super) fn render_dropdown_sized(
        &self,
        id: (&'static str, usize),
        dropdown: Dropdown,
        current: String,
        options: Vec<(String, DropdownAction)>,
        height: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let open = self.open_dropdown.as_ref() == Some(&dropdown);
        let toggle = dropdown.clone();
        let trigger = div()
            .id(id)
            .flex()
            .items_center()
            .gap_1()
            .h(height)
            .px_1p5()
            .rounded_md()
            .bg(rgb(TRACK))
            .border_1()
            .border_color(rgb(if open { FILL_MANUAL } else { BORDER }))
            .text_xs()
            .text_color(rgb(TEXT))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(FILL_HOVER)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.open_dropdown = if this.open_dropdown.as_ref() == Some(&toggle) {
                    None
                } else {
                    Some(toggle.clone())
                };
                cx.notify();
            }))
            .child(div().flex_1().truncate().child(current))
            .child(div().text_color(rgb(TEXT_DIM)).child("▾"));

        let mut wrap = div().relative().w_full().child(trigger);
        if open {
            wrap = wrap.child(deferred(
                div()
                    .absolute()
                    .top(height + px(4.))
                    .left_0()
                    .right_0()
                    .flex()
                    .flex_col()
                    .p_1()
                    .rounded_md()
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .shadow(floating_shadow())
                    // Mouse downs inside the popup must not reach the
                    // click-away overlay, or the popup vanishes before the
                    // option's click completes on mouse up.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                    )
                    .children(options.into_iter().enumerate().map(|(i, (label, action))| {
                        div().child(
                            div()
                                .id(("dropdown-option", i))
                                .px_1p5()
                                .py_1()
                                .rounded_sm()
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(FILL_HOVER)))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.open_dropdown = None;
                                    action(this, cx);
                                    cx.notify();
                                }))
                                .child(label),
                        )
                    })),
            ));
        }
        wrap
    }

    /// Display name of a curve's temperature source.
    pub(super) fn source_label(&self, source: &CurveSource) -> String {
        match source {
            CurveSource::Temp { chip, temp } => self.temp_label(chip, temp - 1),
            CurveSource::Custom { custom } => self
                .names
                .customs()
                .iter()
                .find(|def| &def.id == custom)
                .map(|def| def.name.clone())
                .unwrap_or_else(|| custom.clone()),
        }
    }

    /// Source picker for one curve: every live hardware temperature plus
    /// the custom sensors, with their current readings.
    pub(super) fn render_source_dropdown(
        &self,
        def: &CurveDef,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Div {
        let mut options: Vec<(String, DropdownAction)> = Vec::new();
        let mut push = |label: String, source: CurveSource, def: CurveDef| {
            options.push((
                label,
                Rc::new(move |this: &mut Self, cx: &mut Context<Self>| {
                    let mut def = def.clone();
                    def.source = source.clone();
                    this.commit_curve(def, cx);
                }),
            ));
        };
        for (ci, snapshot) in snapshots.iter().enumerate() {
            let Some(info) = chips.get(ci) else { continue };
            for (ti, temp) in snapshot.temps.iter().enumerate() {
                let Some(value) = temp else { continue };
                push(
                    format!("{} · {value:.0} °C", self.temp_label(&info.name, ti)),
                    CurveSource::Temp {
                        chip: info.name.clone(),
                        temp: ti + 1,
                    },
                    def.clone(),
                );
            }
        }
        for custom in self.names.customs() {
            let value = customs
                .iter()
                .find(|published| published.id == custom.id)
                .and_then(|published| published.value);
            let label = match value {
                Some(value) => format!("{} · {value:.0} °C", custom.name),
                None => custom.name.clone(),
            };
            push(
                label,
                CurveSource::Custom {
                    custom: custom.id.clone(),
                },
                def.clone(),
            );
        }

        let index = self
            .names
            .curves()
            .iter()
            .position(|other| other.id == def.id)
            .unwrap_or(0);
        self.render_dropdown_sized(
            ("curve-source-dialog", index),
            Dropdown::CurveSource {
                curve: def.id.clone(),
            },
            self.source_label(&def.source),
            options,
            px(30.),
            cx,
        )
    }

    /// The Curves row on the Controls page: one card per curve, plus the
    /// add button.
    pub(super) fn render_curve_section(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Div {
        let defs = self.names.curves().to_vec();
        let cards: Vec<Div> = defs
            .iter()
            .enumerate()
            .map(|(i, def)| self.render_curve_card(i, def, chips, snapshots, customs, cx))
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Curves"),
                    )
                    .child(div().flex_1())
                    .child(self.icon_button(
                        "add-curve",
                        "icons/plus.svg",
                        "Add",
                        cx,
                        |this, cx| this.add_curve(cx),
                    )),
            )
            .child(if cards.is_empty() {
                div().text_xs().text_color(rgb(TEXT_DIM)).child(
                    "No curves yet — a curve maps a temperature source to fan duties. \
                     Add one, then switch fans to “curve”.",
                )
            } else {
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap_2()
                    .children(cards)
            })
    }

    fn visibility_toggle(
        &self,
        id: (&'static str, usize),
        label: String,
        hidden: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        div().child(
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_1p5()
                .px_2()
                .py_0p5()
                .rounded_md()
                .bg(rgb(if hidden { PANEL } else { TRACK }))
                .border_1()
                .border_color(rgb(BORDER))
                .text_xs()
                .text_color(rgb(if hidden { TEXT_DIM } else { TEXT }))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on_click(this, cx)))
                .child(
                    svg()
                        .path(if hidden {
                            "icons/eye-off.svg"
                        } else {
                            "icons/eye.svg"
                        })
                        .w(px(12.))
                        .h(px(12.))
                        .flex_none()
                        .text_color(rgb(if hidden { TEXT_DIM } else { TEXT })),
                )
                .child(label),
        )
    }

    /// One visibility toggle in Settings: the channel with an eye state;
    /// clicking flips hidden. Hidden channels render dimmed.
    pub(super) fn visibility_tag(
        &self,
        id: (&'static str, usize),
        chip_name: String,
        key: String,
        label: String,
        hidden: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        self.visibility_toggle(id, label, hidden, cx, move |this, cx| {
            this.set_channel_hidden(&chip_name, &key, !hidden, cx);
        })
    }

    pub(super) fn visibility_device_tag(
        &self,
        id: (&'static str, usize),
        chip_name: String,
        hidden: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        self.visibility_toggle(id, "Device".to_string(), hidden, cx, move |this, cx| {
            this.set_device_hidden(&chip_name, !hidden, cx);
        })
    }

    pub(super) fn visibility_category_tag(
        &self,
        id: (&'static str, usize),
        chip_name: String,
        category: HiddenCategory,
        label: &'static str,
        hidden: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        self.visibility_toggle(id, label.to_string(), hidden, cx, move |this, cx| {
            this.set_category_hidden(&chip_name, category, !hidden, cx);
        })
    }
}
