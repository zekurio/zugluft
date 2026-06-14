use super::*;

impl Zugluft {
    fn active_curve_edit_id(&self) -> Option<&str> {
        self.curve_dialog
            .as_deref()
            .or(self.selected_curve.as_deref())
    }

    /// The curve a fan is assigned to: the optimistic value while an
    /// assignment is in flight, otherwise what the service published.
    pub(super) fn fan_curve(&self, key: FanKey, fan: &FanStatus) -> Option<String> {
        self.pending_assign
            .get(&key)
            .cloned()
            .unwrap_or_else(|| fan.curve.clone())
    }

    fn fan_chip_name(&self, key: FanKey) -> Option<String> {
        let UiState::Service(ServiceState::Ready { chips, .. }) = &self.state else {
            return None;
        };
        Some(chips.get(key.0)?.name.clone())
    }

    pub(super) fn remember_current_curve(&mut self, key: FanKey) {
        let Some(fan) = self.fan_status(key) else {
            return;
        };
        let Some(curve) = self.fan_curve(key, &fan) else {
            return;
        };
        self.remember_fan_curve(key, curve);
    }

    fn remember_fan_curve(&mut self, key: FanKey, curve: String) {
        self.last_curve.insert(key, curve.clone());
        if let Some(chip) = self.fan_chip_name(key) {
            config::save_fan_curve(&chip, key.1, &curve);
        }
    }

    fn remembered_fan_curve(&self, key: FanKey) -> Option<String> {
        let remembered = self.last_curve.get(&key).cloned().or_else(|| {
            self.fan_chip_name(key)
                .and_then(|chip| self.names.fan_curve(&chip, key.1))
        })?;
        self.names
            .curves()
            .iter()
            .any(|def| def.id == remembered)
            .then_some(remembered)
    }

    /// A curve as currently displayed: the in-flight drag/name copy if there
    /// is one, otherwise the config's version.
    pub(super) fn curve_for_display(&self, id: &str) -> Option<CurveDef> {
        let mut def = if let Some(edit) = &self.curve_edit
            && edit.id == id
        {
            edit.clone()
        } else {
            self.names
                .curves()
                .iter()
                .find(|def| def.id == id)
                .cloned()?
        };
        if let Some((name_id, input)) = &self.curve_name_edit
            && name_id == id
            && !input.text.trim().is_empty()
        {
            def.name = input.text.trim().to_string();
        }
        Some(def)
    }

    /// Persists a curve to config.toml and reloads, which also queues a
    /// resync of definitions to the service.
    pub(super) fn commit_curve(&mut self, def: CurveDef, cx: &mut Context<Self>) {
        config::save_curve(&def);
        self.reload_config(cx);
    }

    pub(super) fn set_curve_primary_function(
        &mut self,
        id: &str,
        function: CurveFunction,
        cx: &mut Context<Self>,
    ) {
        self.end_curve_drag(cx);
        let Some(mut def) = self.curve_for_display(id) else {
            return;
        };
        let function = function.sanitized();
        if let CurveFunction::Standard { hysteresis } = function {
            def.hysteresis = hysteresis;
        }
        def.set_primary_function(function);
        self.commit_curve(def, cx);
    }

    pub(super) fn set_curve_kind(&mut self, id: &str, kind: CurveKind, cx: &mut Context<Self>) {
        let Some(mut def) = self.curve_for_display(id) else {
            return;
        };
        def.kind = kind.sanitized();
        self.curve_drag = None;
        self.curve_edit = None;
        self.commit_curve(def, cx);
    }

    pub(super) fn adjust_curve_kind(
        &mut self,
        id: &str,
        field: CurveKindField,
        delta: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(mut def) = self.curve_for_display(id) else {
            return;
        };
        def.kind = def.kind.sanitized();
        match (&mut def.kind, field) {
            (
                CurveKind::Trigger {
                    threshold,
                    before: _,
                    after: _,
                },
                CurveKindField::TriggerThreshold,
            ) => *threshold += delta,
            (
                CurveKind::Trigger {
                    threshold: _,
                    before,
                    after: _,
                },
                CurveKindField::TriggerBefore,
            ) => *before += delta,
            (
                CurveKind::Trigger {
                    threshold: _,
                    before: _,
                    after,
                },
                CurveKindField::TriggerAfter,
            ) => *after += delta,
            (CurveKind::Linear { start, end }, CurveKindField::LinearStartTemp) => {
                start.0 = (start.0 + delta).clamp(-40.0, end.0 - 0.5);
            }
            (CurveKind::Linear { start, .. }, CurveKindField::LinearStartDuty) => {
                start.1 += delta;
            }
            (CurveKind::Linear { start, end }, CurveKindField::LinearEndTemp) => {
                end.0 = (end.0 + delta).clamp(start.0 + 0.5, 150.0);
            }
            (CurveKind::Linear { end, .. }, CurveKindField::LinearEndDuty) => {
                end.1 += delta;
            }
            _ => return,
        }
        def.normalize_kind();
        self.commit_curve(def, cx);
    }

    pub(super) fn adjust_curve_hysteresis(
        &mut self,
        id: &str,
        degrees_delta: f32,
        delay_delta_ms: i64,
        toggle_direction: bool,
        cx: &mut Context<Self>,
    ) {
        let mut hysteresis = match self
            .curve_for_display(id)
            .map(|def| def.primary_function())
            .unwrap_or(CurveFunction::Standard {
                hysteresis: Default::default(),
            }) {
            CurveFunction::Standard { hysteresis } => hysteresis.sanitized(),
            _ => CurveHysteresis::default(),
        };
        hysteresis.degrees = (hysteresis.degrees + degrees_delta).clamp(0.0, 20.0);
        let delay = hysteresis.delay_ms as i64 + delay_delta_ms;
        hysteresis.delay_ms = delay.clamp(0, 60_000) as u64;
        if toggle_direction {
            hysteresis.only_downward = !hysteresis.only_downward;
        }
        self.set_curve_primary_function(id, CurveFunction::Standard { hysteresis }, cx);
    }

    pub(super) fn adjust_curve_ema(&mut self, id: &str, alpha_delta: f32, cx: &mut Context<Self>) {
        let alpha = match self
            .curve_for_display(id)
            .map(|def| def.primary_function())
            .unwrap_or(CurveFunction::Ema { alpha: 0.25 })
        {
            CurveFunction::Ema { alpha } => alpha,
            _ => 0.25,
        };
        self.set_curve_primary_function(
            id,
            CurveFunction::Ema {
                alpha: (alpha + alpha_delta).clamp(0.01, 1.0),
            },
            cx,
        );
    }

    pub(super) fn adjust_curve_window(
        &mut self,
        id: &str,
        field: CurveWindowField,
        delta: f32,
        cx: &mut Context<Self>,
    ) {
        self.end_curve_drag(cx);
        let Some(mut def) = self.curve_for_display(id) else {
            return;
        };
        let mut window = def.window.sanitized();
        match field {
            CurveWindowField::TempMin => window.temp_min += delta,
            CurveWindowField::TempMax => window.temp_max += delta,
            CurveWindowField::DutyMin => window.duty_min += delta,
            CurveWindowField::DutyMax => window.duty_max += delta,
        }
        def.window = window.sanitized();
        self.commit_curve(def, cx);
    }

    pub(super) fn reload_config(&mut self, cx: &mut Context<Self>) {
        self.names = config::load();
        self.names_mtime = config::mtime();
        self.customs_synced = false; // re-push customs + curves
        cx.notify();
    }

    /// Creates a new curve over the first available temperature source and
    /// opens its editor.
    pub(super) fn add_curve_with_kind(&mut self, kind: CurveKind, cx: &mut Context<Self>) {
        let source = match self.first_temp_source().or_else(|| {
            self.names
                .customs()
                .first()
                .map(|custom| CurveSource::Custom {
                    custom: custom.id.clone(),
                })
        }) {
            Some(source) => source,
            None => return, // nothing to drive a curve from
        };

        let existing = self.names.curves();
        let mut n = existing.len() + 1;
        while existing.iter().any(|def| def.id == format!("curve{n}")) {
            n += 1;
        }
        let def = CurveDef {
            id: format!("curve{n}"),
            name: format!("Curve {n}"),
            source,
            functions: vec![CurveFunction::Standard {
                hysteresis: Default::default(),
            }],
            hysteresis: Default::default(),
            window: Default::default(),
            kind,
        };
        // Straight into the editor for the new curve.
        let id = def.id.clone();
        let name = def.name.clone();
        self.selected_curve = Some(id.clone());
        self.curve_dialog = Some(id.clone());
        self.curve_name_edit = Some((id, TextEdit::new(name)));
        self.custom_dialog = None;
        self.custom_name_edit = None;
        self.renaming = None;
        self.commit_curve(def, cx);
    }

    /// Deletes a curve, first releasing every fan it was driving.
    pub(super) fn delete_curve(&mut self, id: &str, cx: &mut Context<Self>) {
        let mut released = Vec::new();
        if let UiState::Service(ServiceState::Ready { snapshots, .. }) = &self.state {
            for (ci, snapshot) in snapshots.iter().enumerate() {
                for (fi, fan) in snapshot.fans.iter().enumerate() {
                    if self.fan_curve((ci, fi), fan).as_deref() == Some(id) {
                        released.push((ci, fi));
                    }
                }
            }
        }
        for key in released {
            self.assign_fan(key, None, cx);
        }
        if self.curve_dialog.as_deref() == Some(id) {
            self.curve_dialog = None;
        }
        if self
            .curve_name_edit
            .as_ref()
            .is_some_and(|(name_id, _)| name_id == id)
        {
            self.curve_name_edit = None;
        }
        self.curve_edit = None;
        self.curve_drag = None;
        self.open_dropdown = None;
        config::delete_curve(id);
        self.reload_config(cx);
    }

    pub(super) fn open_curve_dialog(&mut self, id: String, cx: &mut Context<Self>) {
        self.curve_name_edit = self
            .names
            .curves()
            .iter()
            .find(|def| def.id == id)
            .map(|def| (def.id.clone(), TextEdit::new(def.name.clone())));
        self.curve_dialog = Some(id);
        self.custom_dialog = None;
        self.custom_name_edit = None;
        self.renaming = None;
        self.open_dropdown = None;
        cx.notify();
    }

    pub(super) fn close_curve_dialog(&mut self, cx: &mut Context<Self>) {
        self.end_curve_drag(cx); // commits any in-flight point drag
        self.commit_curve_dialog_name(cx);
        self.curve_dialog = None;
        self.curve_name_edit = None;
        self.open_dropdown = None;
        cx.notify();
    }

    pub(super) fn handle_curve_name_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() == "enter" {
            self.commit_curve_dialog_name(cx);
            return;
        }
        if let Some((_, input)) = &mut self.curve_name_edit
            && Self::handle_text_key(input, event, 40, |c| !c.is_control(), cx)
        {
            cx.notify();
        }
    }

    fn commit_curve_dialog_name(&mut self, cx: &mut Context<Self>) {
        let Some((id, input)) = self.curve_name_edit.as_ref() else {
            return;
        };
        let name = input.text.trim().to_string();
        if name.is_empty() {
            return;
        }
        let id = id.clone();
        let unchanged = self
            .names
            .curves()
            .iter()
            .any(|def| def.id == id && def.name == name);
        if unchanged {
            return;
        }
        config::save_curve_name(&id, &name);
        self.reload_config(cx);
    }

    /// Hides or shows a channel (`fanN`/`tempN`/`powerN`), persisted in
    /// config.toml.
    pub(super) fn set_channel_hidden(
        &mut self,
        chip: &str,
        key: &str,
        hidden: bool,
        cx: &mut Context<Self>,
    ) {
        config::set_hidden(chip, key, hidden);
        self.reload_config(cx);
    }

    pub(super) fn set_device_hidden(&mut self, chip: &str, hidden: bool, cx: &mut Context<Self>) {
        config::set_device_hidden(chip, hidden);
        self.reload_config(cx);
    }

    pub(super) fn set_category_hidden(
        &mut self,
        chip: &str,
        category: HiddenCategory,
        hidden: bool,
        cx: &mut Context<Self>,
    ) {
        config::set_category_hidden(chip, category, hidden);
        self.reload_config(cx);
    }

    pub(super) fn assign_fan(
        &mut self,
        key: FanKey,
        curve: Option<String>,
        cx: &mut Context<Self>,
    ) {
        match &curve {
            Some(curve) => self.remember_fan_curve(key, curve.clone()),
            None => self.remember_current_curve(key),
        }
        self.pending.remove(&key);
        self.pending_assign.insert(key, curve.clone());
        let _ = self.tx.send(Request::SetFanCurve {
            chip: key.0,
            fan: key.1,
            curve,
        });
        cx.notify();
    }

    /// The fan card's "curve" mode: resume the last curve when possible;
    /// the picker that replaces the slider switches between curves afterwards.
    pub(super) fn set_curve_mode(&mut self, key: FanKey, cx: &mut Context<Self>) {
        let id = self
            .remembered_fan_curve(key)
            .or_else(|| self.names.curves().first().map(|def| def.id.clone()));
        if let Some(id) = id {
            self.assign_fan(key, Some(id), cx);
        }
    }

    /// Plot coordinates (temp °C, target %) for a cursor position.
    pub(super) fn curve_plot_value(&self, position: gpui::Point<Pixels>) -> Option<(f32, f32)> {
        let bounds = (*self.curve_bounds.borrow())?;
        let window = self
            .active_curve_edit_id()
            .and_then(|id| self.curve_for_display(id))
            .map(|def| def.window.sanitized())
            .unwrap_or_else(|| CurveWindow::default().sanitized());
        let temp =
            window.temp_at(((position.x - bounds.origin.x) / bounds.size.width).clamp(0.0, 1.0));
        let target = window
            .duty_at((1.0 - (position.y - bounds.origin.y) / bounds.size.height).clamp(0.0, 1.0));
        Some((temp, target))
    }

    /// Mouse down on the dialog's curve plot: grab a point (double-click
    /// removes it), or add one where the click landed and start dragging.
    pub(super) fn curve_editor_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(bounds) = *self.curve_bounds.borrow() else {
            return;
        };
        let Some(id) = self.active_curve_edit_id().map(str::to_string) else {
            return;
        };
        let Some(mut def) = self.curve_for_display(&id) else {
            return;
        };
        let Some((temp, target)) = self.curve_plot_value(event.position) else {
            return;
        };
        let window = def.window.sanitized();
        let CurveKind::Graph { points } = &mut def.kind else {
            return;
        };

        let hit = points.iter().position(|&(pt, pp)| {
            let x = bounds.origin.x + bounds.size.width * window.temp_fraction(pt);
            let y = bounds.origin.y + bounds.size.height * (1.0 - window.duty_fraction(pp));
            let dx = f32::from(x - event.position.x);
            let dy = f32::from(y - event.position.y);
            dx * dx + dy * dy <= CURVE_HIT_RADIUS * CURVE_HIT_RADIUS
        });

        match hit {
            // Double-click removes; a curve keeps at least one point.
            Some(index) if event.click_count >= 2 => {
                if points.len() > 1 {
                    points.remove(index);
                    self.curve_drag = None;
                    self.curve_edit = None;
                    self.commit_curve(def, cx);
                }
            }
            Some(index) => {
                self.curve_drag = Some(index);
                self.curve_edit = Some(def);
            }
            None => {
                let index = points
                    .iter()
                    .position(|&(pt, _)| pt > temp)
                    .unwrap_or(points.len());
                points.insert(index, (temp, target));
                self.curve_drag = Some(index);
                self.curve_edit = Some(def);
            }
        }
        cx.notify();
    }

    pub(super) fn curve_drag_move(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.curve_drag else {
            return;
        };
        let Some((temp, target)) = self.curve_plot_value(position) else {
            return;
        };
        let Some(def) = &mut self.curve_edit else {
            return;
        };
        let window = def.window.sanitized();
        let CurveKind::Graph { points } = &mut def.kind else {
            return;
        };
        if index >= points.len() {
            return;
        }
        // A dragged point stops at its neighbors, keeping the set ordered.
        let min = if index > 0 {
            points[index - 1].0 + 0.5
        } else {
            window.temp_min
        };
        let max = if index + 1 < points.len() {
            points[index + 1].0 - 0.5
        } else {
            window.temp_max
        };
        points[index] = (temp.clamp(min, max.max(min)), target);
        cx.notify();
    }

    /// Releasing the drag persists the edited curve.
    pub(super) fn end_curve_drag(&mut self, cx: &mut Context<Self>) {
        if self.curve_drag.take().is_some()
            && let Some(def) = self.curve_edit.take()
        {
            self.commit_curve(def, cx);
        }
    }
}
