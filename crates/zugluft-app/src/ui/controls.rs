use super::*;

impl Zugluft {
    pub(super) const ACTION_MENU_WIDTH: Pixels = px(188.);

    pub(super) fn menu_icon(&self, path: &'static str, color: u32) -> Div {
        div()
            .w(px(16.))
            .h(px(16.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path(path)
                    .w(px(14.))
                    .h(px(14.))
                    .flex_none()
                    .text_color(rgb(color)),
            )
    }

    pub(super) fn menu_label(&self, label: &'static str, color: u32) -> Div {
        div()
            .flex_1()
            .min_w(px(0.))
            .truncate()
            .text_sm()
            .text_color(rgb(color))
            .child(label)
    }

    pub(super) fn render_controls(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        notes: &[String],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Div {
        let sections = self.render_dashboard_sections(chips, snapshots, customs, cx);

        // The page scrolls; without this an overflowing page squeezes the
        // shrinkable parts of the layout instead (and curves + tuning make
        // it overflow easily in short windows).
        div().flex_1().min_h(px(0.)).overflow_hidden().p_2().child(
            div()
                .id("controls-scroll")
                .relative()
                .size_full()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_3()
                .p_3()
                .rounded_lg()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .shadow(floating_shadow())
                .child(if sections.is_empty() {
                    div()
                        .min_h(px(180.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(rgb(TEXT_DIM))
                        .child("No pinned items")
                } else {
                    div().flex().flex_col().gap_4().children(sections)
                })
                .child(self.render_curve_fab(cx))
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
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseUpEvent, _, cx| cx.stop_propagation()),
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
                                    cx.stop_propagation();
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

    pub(super) fn render_dashboard_sensor_card(
        &self,
        sensor: &SensorReading,
        cx: &mut Context<Self>,
    ) -> Div {
        let label = sensor.label.clone();

        div()
            .w(px(188.))
            .flex()
            .flex_col()
            .gap_1()
            .p_2p5()
            .rounded_lg()
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(subtle_shadow())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .child(
                        div()
                            .w(px(8.))
                            .h(px(8.))
                            .flex_none()
                            .rounded_full()
                            .bg(rgb(sensor.color)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(label.clone()),
                    )
                    .child(self.sensor_action_menu(sensor, cx)),
            )
            .child(
                div()
                    .text_base()
                    .font_family(FONT_MONO)
                    .child(sensor.unit.format_value(sensor.value)),
            )
    }

    pub(super) fn render_curve_fab(&self, cx: &mut Context<Self>) -> Div {
        let open = self.open_dropdown == Some(Dropdown::CurveQuickOpen);
        let toggle = Dropdown::CurveQuickOpen;
        let menu = open.then(|| {
            deferred(
                div()
                    .absolute()
                    .right(px(0.))
                    .bottom(px(62.))
                    .w(px(158.))
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
                            .px_1()
                            .pb_1()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(TEXT_DIM))
                            .child("Create"),
                    )
                    .child(
                        div()
                            .id("quick-create-curve")
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .px_1p5()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(FILL_HOVER)))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.open_dropdown = None;
                                this.add_curve_with_kind(
                                    CurveKind::Graph {
                                        points: vec![(30.0, 20.0), (50.0, 40.0), (70.0, 100.0)],
                                    },
                                    cx,
                                );
                            }))
                            .child(
                                svg()
                                    .path("icons/spline.svg")
                                    .w(px(14.))
                                    .h(px(14.))
                                    .text_color(rgb(FILL_MANUAL)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .child(div().text_sm().truncate().child("New curve")),
                            ),
                    )
                    .child(
                        div()
                            .id("quick-create-sensor")
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .px_1p5()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(FILL_HOVER)))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.open_dropdown = None;
                                this.add_custom(cx);
                            }))
                            .child(
                                svg()
                                    .path("icons/thermometer.svg")
                                    .w(px(14.))
                                    .h(px(14.))
                                    .text_color(rgb(0x8bd17c)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .child(div().text_sm().truncate().child("Sensor")),
                            ),
                    ),
            )
        });

        div().absolute().right(px(18.)).bottom(px(18.)).child(
            div().relative().children(menu).child(
                div()
                    .id("curve-fab")
                    .w(px(46.))
                    .h(px(46.))
                    .rounded_full()
                    .bg(rgb(FILL_MANUAL))
                    .border_1()
                    .border_color(rgb(0x75b7ff))
                    .shadow(floating_shadow())
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.open_dropdown = if this.open_dropdown.as_ref() == Some(&toggle) {
                            None
                        } else {
                            Some(toggle.clone())
                        };
                        cx.notify();
                    }))
                    .child(
                        svg()
                            .path("icons/plus.svg")
                            .w(px(20.))
                            .h(px(20.))
                            .text_color(rgb(BG)),
                    ),
            ),
        )
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
