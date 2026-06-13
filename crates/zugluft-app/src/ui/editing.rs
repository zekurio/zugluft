use super::*;

impl Zugluft {
    /// Caret on/off for blinking text inputs (~500 ms each phase), driven by
    /// the 100 ms refresh tick. `render` keeps re-rendering while an input is
    /// open so the blink advances.
    pub(super) fn caret_on(&self) -> bool {
        self.refresh_ticks % 10 < 5
    }

    pub(super) fn render_text_edit_contents(
        &self,
        input: &TextEdit,
        caret_height: f32,
        focused: bool,
    ) -> Div {
        let caret = || div().w(px(2.)).h(px(caret_height)).bg(rgb(FILL_MANUAL));
        let text_piece = |text: &str| {
            div()
                .flex_none()
                .whitespace_nowrap()
                .child(text.to_string())
        };
        let range = input.selected_range();
        let mut row = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .items_center()
            .overflow_hidden();

        if range.is_empty() {
            row = row
                .child(text_piece(&input.text[..input.cursor]))
                .children((focused && self.caret_on()).then(caret))
                .child(text_piece(&input.text[input.cursor..]));
        } else {
            row = row
                .child(text_piece(&input.text[..range.start]))
                .child(
                    div()
                        .flex_none()
                        .px(px(1.))
                        .rounded_sm()
                        .bg(rgb(FILL_MANUAL))
                        .text_color(rgb(BG))
                        .whitespace_nowrap()
                        .child(input.text[range.clone()].to_string()),
                )
                .child(text_piece(&input.text[range.end..]));
        }
        row
    }

    pub(super) fn render_dialog_text_field(&self, input: &TextEdit, focused: bool) -> Div {
        div()
            .flex()
            .items_center()
            .h(px(30.))
            .px_2()
            .rounded_md()
            .bg(rgb(TRACK))
            .border_1()
            .border_color(rgb(if focused { FILL_MANUAL } else { BORDER }))
            .text_sm()
            .text_color(rgb(TEXT))
            .child(self.render_text_edit_contents(input, 16., focused))
    }

    pub(super) fn handle_text_key(
        input: &mut TextEdit,
        event: &KeyDownEvent,
        max_chars: usize,
        allow: impl Fn(char) -> bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let secondary = modifiers.secondary();
        match key {
            "a" if secondary => input.select_all(),
            "c" if secondary => {
                if let Some(text) = input.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                true
            }
            "x" if secondary => {
                if let Some(text) = input.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    input.delete_forward(false);
                }
                true
            }
            "v" if secondary => cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .is_some_and(|text| {
                    let text = text.replace(['\r', '\n'], " ");
                    input.insert_filtered(&text, max_chars, &allow)
                }),
            "backspace" => input.delete_backward(secondary),
            "delete" => input.delete_forward(secondary),
            "left" => input.move_left(modifiers.shift, secondary),
            "right" => input.move_right(modifiers.shift, secondary),
            "home" => input.move_home(modifiers.shift),
            "end" => input.move_end(modifiers.shift),
            _ if secondary || modifiers.platform || modifiers.function => false,
            _ if modifiers.alt && event.keystroke.key_char.is_none() => false,
            "space" => input.insert_filtered(" ", max_chars, &allow),
            _ => event
                .keystroke
                .key_char
                .as_deref()
                .filter(|text| !text.chars().any(char::is_control))
                .is_some_and(|text| input.insert_filtered(text, max_chars, &allow)),
        }
    }

    pub(super) fn begin_edit(
        &mut self,
        key: FanKey,
        field: SettingField,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.renaming = None;
        self.editing = Some(FieldEdit {
            key,
            field,
            input: TextEdit::new(current),
        });
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// Keyboard input for a tuning field.
    pub(super) fn handle_edit_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "enter" => self.commit_edit(cx),
            "escape" => {
                self.editing = None;
                cx.notify();
            }
            _ => {
                if let Some(edit) = &mut self.editing
                    && Self::handle_text_key(
                        &mut edit.input,
                        event,
                        6,
                        |c| c.is_ascii_digit() || c == '.' || c == '-',
                        cx,
                    )
                {
                    cx.notify();
                }
            }
        }
    }

    /// Parses the edited field and sends the fan's full settings to the
    /// service. An empty field clears the override (start/stop fall back to
    /// the calibrated value, step limits to instant, offset/minimum to 0);
    /// unparsable text leaves the old value alone. Clamps mirror the
    /// service so the optimistic value matches the echo.
    pub(super) fn commit_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.editing.take() else {
            return;
        };
        cx.notify();
        let text = edit.input.text.trim().to_string();
        let parsed = text.parse::<f32>().ok();
        if !text.is_empty() && parsed.is_none() {
            return;
        }
        let Some(fan) = self.fan_status(edit.key) else {
            return;
        };

        let mut settings = self.fan_settings(edit.key, &fan);
        let percent = parsed.map(|v| v.clamp(0.0, 100.0));
        let rate = parsed.filter(|v| *v > 0.0).map(|v| v.min(100.0));
        match edit.field {
            SettingField::StepUp => settings.step_up = rate,
            SettingField::StepDown => settings.step_down = rate,
            SettingField::Start => settings.start_percent = percent,
            SettingField::Stop => settings.stop_percent = percent,
            SettingField::Offset => {
                settings.offset = parsed.unwrap_or(0.0).clamp(-100.0, 100.0);
            }
            SettingField::Minimum => {
                settings.minimum_percent = percent.unwrap_or(0.0);
            }
        }
        self.pending_settings.insert(edit.key, settings);
        let _ = self.tx.send(Request::SetFanSettings {
            chip: edit.key.0,
            fan: edit.key.1,
            settings,
        });
    }

    pub(super) fn select_view(&mut self, view: AppView, cx: &mut Context<Self>) {
        self.active_view = view;
        self.search_active = false;
        cx.notify();
    }

    pub(super) fn begin_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.renaming = None;
        self.search_active = true;
        self.sensor_search.move_end(false);
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// Keyboard input for the sensor filter.
    pub(super) fn handle_search_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "enter" => self.search_active = false,
            "escape" => {
                if self.sensor_search.text.is_empty() {
                    self.search_active = false;
                } else {
                    self.sensor_search.clear();
                }
            }
            _ => {
                if Self::handle_text_key(
                    &mut self.sensor_search,
                    event,
                    40,
                    |c| !c.is_control(),
                    cx,
                ) {
                    cx.notify();
                }
            }
        }
        cx.notify();
    }

    /// The Sensors page header search box; filters the sensor list live.
    pub(super) fn render_search_box(&self, cx: &mut Context<Self>) -> Div {
        let active = self.search_active;
        let query = self.sensor_search.text.clone();
        div().child(
            div()
                .id("sensor-search")
                .flex()
                .items_center()
                .gap_1p5()
                .w_full()
                .h(px(24.))
                .px_2()
                .rounded_md()
                .bg(rgb(TRACK))
                .border_1()
                .border_color(rgb(if active { FILL_MANUAL } else { BORDER }))
                .cursor_pointer()
                // Clicking the box (not elsewhere) keeps focus here.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.begin_search(window, cx);
                }))
                .child(
                    svg()
                        .path("icons/search.svg")
                        .w(px(12.))
                        .h(px(12.))
                        .flex_none()
                        .text_color(rgb(TEXT_DIM)),
                )
                .child(if query.is_empty() && !active {
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(rgb(TEXT_DIM))
                        .child("Search sensors…")
                } else {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .text_xs()
                        .text_color(rgb(TEXT))
                        .child(self.render_text_edit_contents(&self.sensor_search, 12., active))
                })
                .children((!query.is_empty()).then(|| {
                    div()
                        .id("sensor-search-clear")
                        .flex_none()
                        .px_0p5()
                        .text_xs()
                        .text_color(rgb(TEXT_DIM))
                        .cursor_pointer()
                        .hover(|s| s.text_color(rgb(TEXT)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.sensor_search.clear();
                            cx.notify();
                        }))
                        .child("✕")
                })),
        )
    }

    /// Flips a graph line's visibility, persisted in config. The override is
    /// dropped when it matches the kind's default, keeping the file tidy.
    pub(super) fn toggle_graph_line(
        &mut self,
        chip: String,
        channel: String,
        default: bool,
        cx: &mut Context<Self>,
    ) {
        let shown = self.names.graph_shown(&chip, &channel).unwrap_or(default);
        let next = !shown;
        config::set_graph_shown(&chip, &channel, (next != default).then_some(next));
        self.reload_config(cx);
    }

    pub(super) fn begin_rename(
        &mut self,
        key: SensorKey,
        current: String,
        appearance: Option<(String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing = None;
        self.curve_dialog = None;
        self.curve_name_edit = None;
        self.custom_dialog = None;
        self.custom_name_edit = None;
        self.renaming = Some(Rename {
            key,
            input: TextEdit::new(current),
            appearance,
        });
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// The Edit modal: a name field (driven by the window-level key handler)
    /// plus, for graph lines, color and line-style controls. One dialog for
    /// renaming a sensor, fan or curve and restyling its graph line.
    pub(super) fn render_rename_dialog(&self, rename: &Rename, cx: &mut Context<Self>) -> Div {
        let mut panel = div()
            .w(px(480.))
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
            .child(div().font_weight(FontWeight::MEDIUM).child("Edit"))
            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Name"))
            .child(self.render_dialog_text_field(&rename.input, true));

        // Graph lines also get color / line-style controls.
        if let Some((chip, channel)) = &rename.appearance {
            panel = panel.child(self.render_appearance_controls(chip, channel, cx));
        }

        panel = panel.child(
            div()
                .flex()
                .items_center()
                .child(div().flex_1())
                .child(self.button("rename-cancel", "Cancel", cx, |this, cx| {
                    this.renaming = None;
                    cx.notify();
                }))
                .child(div().w(px(8.)))
                .child(self.button("rename-save", "Save", cx, |this, cx| {
                    this.commit_rename(cx);
                })),
        );

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
                    this.renaming = None;
                    cx.notify();
                }),
            )
            .child(panel)
    }

    /// Keyboard input for the rename dialog.
    pub(super) fn handle_rename_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.renaming.is_none() {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter" => self.commit_rename(cx),
            "escape" => {
                self.renaming = None;
                cx.notify();
            }
            _ => {
                if let Some(rename) = &mut self.renaming
                    && Self::handle_text_key(&mut rename.input, event, 40, |c| !c.is_control(), cx)
                {
                    cx.notify();
                }
            }
        }
    }

    /// Persists the new name to config.toml: hardware channels as
    /// `tempN`/`fanN` overrides (empty text removes the override), custom
    /// sensors as their `[[custom]]` entry's `name`.
    pub(super) fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(rename) = self.renaming.take() else {
            return;
        };
        let text = rename.input.text.trim().to_string();
        if let UiState::Service(ServiceState::Ready { chips, customs, .. }) = &self.state {
            match rename.key.kind {
                SensorKind::Temperature | SensorKind::FanRpm | SensorKind::Power => {
                    if let Some(chip) = chips.get(rename.key.chip) {
                        let prefix = match rename.key.kind {
                            SensorKind::Temperature => "temp",
                            SensorKind::Power => "power",
                            _ => "fan",
                        };
                        config::save_chip_name(
                            &chip.name,
                            &format!("{prefix}{}", rename.key.index + 1),
                            (!text.is_empty()).then_some(text.as_str()),
                        );
                    }
                }
                SensorKind::Custom => {
                    if !text.is_empty()
                        && let Some(custom) = customs.get(rename.key.index)
                    {
                        config::save_custom_name(&custom.id, &text);
                        // The service shows the new name too (graph data
                        // it publishes carries it after the resync).
                        self.customs_synced = false;
                    }
                }
            }
        }
        self.names = config::load();
        self.names_mtime = config::mtime();
        cx.notify();
    }
}
