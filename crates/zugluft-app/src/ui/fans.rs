use super::*;

impl Zugluft {
    pub(super) fn send_duty(&self, key: FanKey, duty: Option<u8>) {
        let _ = self.tx.send(Request::SetDuty {
            chip: key.0,
            fan: key.1,
            duty,
        });
    }

    pub(super) fn percent_at(&self, key: FanKey, x: Pixels) -> Option<f32> {
        let bounds = self.track_bounds.borrow().get(&key).copied()?;
        let fraction = ((x - bounds.origin.x) / bounds.size.width).clamp(0.0, 1.0);
        Some(fraction * 100.0)
    }

    pub(super) fn apply_percent(&mut self, key: FanKey, percent: f32, cx: &mut Context<Self>) {
        let percent = self
            .fan_status(key)
            .map(|fan| {
                let settings = self.fan_settings(key, &fan);
                let floor = settings
                    .minimum_percent
                    .max(fan.min_percent.unwrap_or(0.0))
                    .clamp(0.0, 100.0);
                percent.clamp(floor, 100.0)
            })
            .unwrap_or_else(|| percent.clamp(0.0, 100.0));
        self.pending.insert(key, percent);
        // A manual target takes the fan off its curve (the service does the
        // same); reflect that immediately.
        self.remember_current_curve(key);
        self.pending_assign.insert(key, None);
        self.send_duty(key, Some((percent * 255.0 / 100.0).round() as u8));
        cx.notify();
    }

    pub(super) fn begin_drag(&mut self, key: FanKey, x: Pixels, cx: &mut Context<Self>) {
        if let Some(percent) = self.percent_at(key, x) {
            self.dragging = Some(key);
            self.apply_percent(key, percent, cx);
        }
    }

    pub(super) fn drag_move(&mut self, x: Pixels, cx: &mut Context<Self>) {
        if let Some(key) = self.dragging
            && let Some(percent) = self.percent_at(key, x)
        {
            self.apply_percent(key, percent, cx);
        }
    }

    pub(super) fn update_graph_hover(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        // A modal owns the cursor; the graph behind it must not react.
        let blocked = self.curve_dialog.is_some()
            || self.custom_dialog.is_some()
            || self.renaming.is_some()
            || self.confirm_delete.is_some();
        let inside = !blocked
            && self
                .graph_bounds
                .borrow()
                .is_some_and(|bounds| bounds.contains(&position));
        let next = inside.then_some(position);
        if next != self.graph_hover {
            self.graph_hover = next;
            cx.notify();
        }
    }

    pub(super) fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.dragging.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn set_auto(&mut self, key: FanKey, cx: &mut Context<Self>) {
        self.remember_current_curve(key);
        self.pending.remove(&key);
        self.pending_assign.insert(key, None);
        self.send_duty(key, None);
        cx.notify();
    }

    /// The latest published status of one fan.
    pub(super) fn fan_status(&self, key: FanKey) -> Option<FanStatus> {
        let UiState::Service(ServiceState::Ready { snapshots, .. }) = &self.state else {
            return None;
        };
        snapshots.get(key.0)?.fans.get(key.1).cloned()
    }

    /// Settings shown for a fan: the optimistic edit if one is in flight,
    /// otherwise what the service published.
    pub(super) fn fan_settings(&self, key: FanKey, fan: &FanStatus) -> FanSettings {
        self.pending_settings
            .get(&key)
            .copied()
            .unwrap_or(fan.settings)
    }

    /// Switch a fan from auto to manual, resuming at its last manual target
    /// (or a safe midpoint when there is none to resume).
    pub(super) fn set_manual(&mut self, key: FanKey, cx: &mut Context<Self>) {
        let percent = self
            .pending
            .get(&key)
            .copied()
            .or_else(|| self.fan_status(key)?.target_percent)
            .or_else(|| match self.fan_status(key)?.duty {
                Some(FanDuty::Manual { percent }) => Some(percent),
                _ => None,
            })
            .unwrap_or(50.0);
        self.apply_percent(key, percent, cx);
    }

    pub(super) fn toggle_tuning(&mut self, key: FanKey, cx: &mut Context<Self>) {
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
        cx.notify();
    }
}
