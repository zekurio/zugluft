use super::*;

impl Zugluft {
    /// A custom sensor as currently stored, by id.
    pub(super) fn custom_for_display(&self, id: &str) -> Option<CustomSensorDef> {
        self.names
            .customs()
            .iter()
            .find(|def| def.id == id)
            .cloned()
    }

    /// Persists a custom sensor to config.toml and reloads, which re-pushes
    /// the definitions to the service for evaluation.
    pub(super) fn commit_custom(&mut self, def: CustomSensorDef, cx: &mut Context<Self>) {
        config::save_custom(&def);
        self.reload_config(cx);
    }

    /// Creates a new, empty custom sensor and opens its editor; the user
    /// picks the inputs there.
    pub(super) fn add_custom(&mut self, cx: &mut Context<Self>) {
        let existing = self.names.customs();
        let mut n = existing.len() + 1;
        while existing.iter().any(|def| def.id == format!("custom{n}")) {
            n += 1;
        }
        let def = CustomSensorDef {
            id: format!("custom{n}"),
            name: format!("Sensor {n}"),
            kind: CustomKind::Average,
            inputs: Vec::new(),
        };
        self.custom_dialog = Some(def.id.clone());
        self.custom_name_edit = Some((def.id.clone(), TextEdit::new(def.name.clone())));
        self.curve_dialog = None;
        self.curve_name_edit = None;
        self.renaming = None;
        self.open_dropdown = None;
        self.commit_custom(def, cx);
    }

    pub(super) fn open_custom_dialog(&mut self, id: String, cx: &mut Context<Self>) {
        let name = self
            .custom_for_display(&id)
            .map(|def| def.name)
            .unwrap_or_else(|| id.clone());
        self.custom_dialog = Some(id.clone());
        self.custom_name_edit = Some((id, TextEdit::new(name)));
        self.curve_dialog = None;
        self.curve_name_edit = None;
        self.renaming = None;
        self.open_dropdown = None;
        cx.notify();
    }

    pub(super) fn close_custom_dialog(&mut self, cx: &mut Context<Self>) {
        self.custom_dialog = None;
        self.custom_name_edit = None;
        self.open_dropdown = None;
        cx.notify();
    }

    pub(super) fn commit_custom_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some((id, input)) = self.custom_name_edit.take() {
            let text = input.text.trim().to_string();
            if !text.is_empty() {
                config::save_custom_name(&id, &text);
                self.customs_synced = false;
            }
        }
        self.custom_dialog = None;
        self.open_dropdown = None;
        self.names = config::load();
        self.names_mtime = config::mtime();
        cx.notify();
    }

    pub(super) fn handle_custom_name_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "enter" => self.commit_custom_dialog(cx),
            "escape" => {
                if self.open_dropdown.take().is_none() {
                    self.close_custom_dialog(cx);
                } else {
                    cx.notify();
                }
            }
            _ => {
                if let Some((_, input)) = &mut self.custom_name_edit
                    && Self::handle_text_key(input, event, 40, |c| !c.is_control(), cx)
                {
                    cx.notify();
                }
            }
        }
    }

    pub(super) fn set_custom_kind(&mut self, id: &str, kind: CustomKind, cx: &mut Context<Self>) {
        if let Some(mut def) = self.custom_for_display(id)
            && def.kind != kind
        {
            def.kind = kind;
            self.commit_custom(def, cx);
        }
    }

    pub(super) fn add_custom_input(
        &mut self,
        id: &str,
        chip: String,
        temp: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(mut def) = self.custom_for_display(id) {
            def.inputs.push(CustomInput {
                chip,
                temp,
                weight: 1.0,
            });
            self.commit_custom(def, cx);
        }
    }

    /// Removes one input; a sensor keeps at least one (like a curve keeps a
    /// point), so the only input can't be dropped — swap by adding first.
    pub(super) fn remove_custom_input(&mut self, id: &str, index: usize, cx: &mut Context<Self>) {
        if let Some(mut def) = self.custom_for_display(id)
            && index < def.inputs.len()
            && def.inputs.len() > 1
        {
            def.inputs.remove(index);
            self.commit_custom(def, cx);
        }
    }

    pub(super) fn adjust_custom_weight(
        &mut self,
        id: &str,
        index: usize,
        delta: f32,
        cx: &mut Context<Self>,
    ) {
        if let Some(mut def) = self.custom_for_display(id)
            && let Some(input) = def.inputs.get_mut(index)
        {
            input.weight = (input.weight + delta).clamp(0.5, 10.0);
            self.commit_custom(def, cx);
        }
    }

    /// The modal custom-sensor editor: pick how inputs combine
    /// (average/min/max), add or drop temperature channels, and weight them.
    pub(super) fn render_custom_dialog(
        &self,
        id: &str,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        _customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let def = self.custom_for_display(id)?;
        let unit = self.temp_display_unit();
        let fmt = |celsius: f32| unit.format_value(self.convert_temp(celsius));
        let value_text = def
            .evaluate(chips, snapshots)
            .map_or_else(|| "—".to_string(), &fmt);
        let is_avg = def.kind == CustomKind::Average;
        let fallback_name = TextEdit::new(def.name.clone());
        let name_input = self
            .custom_name_edit
            .as_ref()
            .filter(|(edit_id, _)| edit_id == id)
            .map(|(_, input)| input)
            .unwrap_or(&fallback_name);

        let kind_seg = self.segmented([
            self.segment(
                ("custom-kind", 0),
                "Average",
                def.kind == CustomKind::Average,
                cx,
                {
                    let id = id.to_string();
                    move |this, cx| this.set_custom_kind(&id, CustomKind::Average, cx)
                },
            ),
            self.segment(
                ("custom-kind", 1),
                "Min",
                def.kind == CustomKind::Min,
                cx,
                {
                    let id = id.to_string();
                    move |this, cx| this.set_custom_kind(&id, CustomKind::Min, cx)
                },
            ),
            self.segment(
                ("custom-kind", 2),
                "Max",
                def.kind == CustomKind::Max,
                cx,
                {
                    let id = id.to_string();
                    move |this, cx| this.set_custom_kind(&id, CustomKind::Max, cx)
                },
            ),
        ]);

        // One row per input. A for-loop keeps the mutable `cx` borrow simple.
        let removable = def.inputs.len() > 1;
        let mut rows: Vec<Div> = Vec::new();
        for (index, input) in def.inputs.iter().enumerate() {
            let reading = chips
                .iter()
                .position(|c| c.name == input.chip)
                .and_then(|ci| snapshots.get(ci))
                .and_then(|snap| {
                    input
                        .temp
                        .checked_sub(1)
                        .and_then(|i| snap.temps.get(i))
                        .copied()
                        .flatten()
                });
            let label = self.temp_label(&input.chip, input.temp.saturating_sub(1));
            let reading_text = reading.map_or_else(|| "—".to_string(), &fmt);

            let mut row = div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_md()
                .bg(rgb(TRACK))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_baseline()
                        .gap_2()
                        .overflow_hidden()
                        .child(div().truncate().text_sm().child(label))
                        .child(
                            div()
                                .flex_none()
                                .text_xs()
                                .font_family(FONT_MONO)
                                .text_color(rgb(TEXT_DIM))
                                .child(reading_text),
                        ),
                );
            if is_avg {
                row = row.child(self.weight_stepper(id, index, input.weight, cx));
            }
            if removable {
                let del_id = id.to_string();
                row = row.child(
                    div()
                        .id(("custom-input-del", index))
                        .flex_none()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.remove_custom_input(&del_id, index, cx);
                        }))
                        .child(
                            svg()
                                .path("icons/trash.svg")
                                .w(px(13.))
                                .h(px(13.))
                                .text_color(rgb(TEXT_DIM))
                                .hover(|s| s.text_color(rgb(ERROR))),
                        ),
                );
            }
            rows.push(row);
        }

        let summary = match def.kind {
            CustomKind::Average => "Reports the weighted average of its inputs.",
            CustomKind::Min => "Reports the lowest of its inputs.",
            CustomKind::Max => "Reports the highest of its inputs.",
        };

        let panel = div()
            .w(px(440.))
            .flex()
            .flex_col()
            .gap_3()
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
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child("Edit sensor"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_base()
                            .font_family(FONT_MONO)
                            .text_color(rgb(ACCENT_OK))
                            .child(value_text),
                    ),
            )
            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Name"))
            .child(self.render_dialog_text_field(name_input, true))
            .child(self.render_appearance_controls(&def.id, "custom", cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Combine"))
                    .child(kind_seg),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .children(rows)
                    .children(def.inputs.is_empty().then(|| {
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_DIM))
                            .child("No inputs yet — add a temperature channel below.")
                    }))
                    .child(self.render_custom_add_dropdown(&def, chips, snapshots, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child(summary))
                    .child(div().flex_1())
                    .child(self.button("custom-dialog-done", "Done", cx, |this, cx| {
                        this.commit_custom_dialog(cx)
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
                    cx.listener(|this, _: &MouseDownEvent, _, cx| this.close_custom_dialog(cx)),
                )
                .child(panel),
        )
    }

    /// A compact `[−] weight [+]` stepper for one Average input.
    fn weight_stepper(&self, id: &str, index: usize, weight: f32, cx: &mut Context<Self>) -> Div {
        let dec = id.to_string();
        let inc = id.to_string();
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .child(
                self.weight_button(("custom-w-dec", index), "−", cx, move |this, cx| {
                    this.adjust_custom_weight(&dec, index, -0.5, cx)
                }),
            )
            .child(
                div()
                    .w(px(26.))
                    .text_center()
                    .text_xs()
                    .font_family(FONT_MONO)
                    .text_color(rgb(TEXT))
                    .child(format!("{weight:.1}")),
            )
            .child(
                self.weight_button(("custom-w-inc", index), "+", cx, move |this, cx| {
                    this.adjust_custom_weight(&inc, index, 0.5, cx)
                }),
            )
    }

    fn weight_button(
        &self,
        id: (&'static str, usize),
        glyph: &'static str,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        div().child(
            div()
                .id(id)
                .w(px(18.))
                .h(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .text_xs()
                .text_color(rgb(TEXT_DIM))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)).text_color(rgb(TEXT)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on_click(this, cx)))
                .child(glyph),
        )
    }

    /// "Add input" picker: every live hardware temperature not already in
    /// the sensor, with its current reading.
    fn render_custom_add_dropdown(
        &self,
        def: &CustomSensorDef,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        cx: &mut Context<Self>,
    ) -> Div {
        let mut options: Vec<(String, DropdownAction)> = Vec::new();
        for (ci, snapshot) in snapshots.iter().enumerate() {
            let Some(info) = chips.get(ci) else { continue };
            for (ti, temp) in snapshot.temps.iter().enumerate() {
                let Some(value) = temp else { continue };
                let present = def
                    .inputs
                    .iter()
                    .any(|inp| inp.chip == info.name && inp.temp == ti + 1);
                if present {
                    continue;
                }
                let id = def.id.clone();
                let chip = info.name.clone();
                let temp_index = ti + 1;
                options.push((
                    format!(
                        "{} · {}",
                        self.temp_label(&info.name, ti),
                        self.temp_display_unit()
                            .format_value(self.convert_temp(*value)),
                    ),
                    Rc::new(move |this: &mut Self, cx: &mut Context<Self>| {
                        this.add_custom_input(&id, chip.clone(), temp_index, cx);
                    }),
                ));
            }
        }

        if options.is_empty() {
            return div()
                .text_xs()
                .text_color(rgb(TEXT_DIM))
                .child("Every temperature channel is already an input.");
        }

        let index = self
            .names
            .customs()
            .iter()
            .position(|other| other.id == def.id)
            .unwrap_or(0);
        self.render_dropdown(
            ("custom-add-input", index),
            Dropdown::CustomInput {
                custom: def.id.clone(),
            },
            "Add input…".to_string(),
            options,
            cx,
        )
    }
}
