//! The zugluft main view: one panel per chip, one row per fan with a
//! click/drag target slider, plus temperature readouts and the service
//! setup/health screens.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use gpui::{
    AnchoredPositionMode, BorderStyle, Bounds, BoxShadow, ClickEvent, ClipboardItem, Context,
    Corner, Div, ElementId, FocusHandle, FontWeight, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, SharedString, Window, anchored, canvas,
    deferred, div, fill, hsla, point, prelude::*, px, quad, relative, rgb, size, svg,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use zugluft_ipc::{
    ChipInfo, ChipSnapshot, CurveDef, CurveFunction, CurveHysteresis, CurveKind, CurveSource,
    CurveWindow, CustomInput, CustomKind, CustomSensorDef, CustomSensorValue, FanDuty, FanSettings,
    FanStatus, Request, ServiceState,
};

use crate::assets::{FONT_MONO, FONT_SANS};
use crate::client::{Shared, UiState};
use crate::config::{self, FanUnit, HiddenCategory, NamesConfig, TempUnit};
use crate::elevation;
use crate::winutil;

mod appearance;
mod chrome;
mod controls;
mod curve_draw;
mod curve_helpers;
mod curve_panel;
mod curve_ui;
mod curves;
mod custom_ui;
mod dashboard;
mod editing;
mod fan_card;
mod fans;
mod graph;
mod management;
mod sensor_panel;
mod sensors;
mod settings_view;
mod tuning;
mod types;

use curve_draw::*;
use types::*;

pub struct Zugluft {
    shared: Arc<Shared>,
    tx: Sender<Request>,
    seen_seq: u64,
    state: UiState,
    active_view: AppView,
    dragging: Option<FanKey>,
    /// Optimistic target (%) while dragging, so the slider tracks the cursor
    /// instead of waiting for the next hardware read.
    pending: HashMap<FanKey, f32>,
    sensor_history: Vec<SensorFrame>,
    temp_unit: TempUnit,
    fan_unit: FanUnit,
    /// User display names from config.toml, hot-reloaded on file change.
    names: NamesConfig,
    names_mtime: Option<std::time::SystemTime>,
    refresh_ticks: u32,
    toast: Option<Toast>,
    template_written: bool,
    /// Slider track screen bounds, captured during paint.
    track_bounds: Rc<RefCell<HashMap<FanKey, Bounds<Pixels>>>>,
    /// Plot area screen bounds, captured during paint.
    graph_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
    /// Cursor position over the plot, in window coordinates.
    graph_hover: Option<gpui::Point<Pixels>>,
    /// Curve id whose edit dialog is open.
    curve_dialog: Option<String>,
    /// Name field state for the open curve dialog.
    curve_name_edit: Option<(String, TextEdit)>,
    /// Custom sensor id whose editor dialog is open.
    custom_dialog: Option<String>,
    /// Name field state for the open custom sensor dialog.
    custom_name_edit: Option<(String, TextEdit)>,
    /// Pending delete awaiting confirmation.
    confirm_delete: Option<ConfirmDelete>,
    /// The dropdown popup currently open, if any.
    open_dropdown: Option<Dropdown>,
    /// Trigger widths captured during paint, so an open dropdown's list can
    /// match the width of the control that opened it.
    dropdown_widths: Rc<RefCell<HashMap<Dropdown, Pixels>>>,
    /// Local copy of the curve being point-dragged; committed to
    /// config.toml on release.
    curve_edit: Option<CurveDef>,
    /// Index of the dragged point within `curve_edit`.
    curve_drag: Option<usize>,
    /// Curve editor plot bounds, captured during paint.
    curve_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
    /// Optimistic fan→curve assignments until the service echoes them.
    pending_assign: HashMap<FanKey, Option<String>>,
    /// Last curve selected per fan in this app session; also persisted in
    /// config.toml so curve mode resumes the same pick after restart.
    last_curve: HashMap<FanKey, String>,
    /// Inline rename in the sensor panel; keyboard input goes here.
    renaming: Option<Rename>,
    /// Sensor list filter (Sensors page header).
    sensor_search: TextEdit,
    /// Whether the search box has keyboard focus.
    search_active: bool,
    /// Fan cards whose tuning section is open.
    expanded: HashSet<FanKey>,
    /// Inline edit of a tuning field; keyboard input goes here while set.
    editing: Option<FieldEdit>,
    /// Inline edit of a curve dialog number field.
    curve_number_edit: Option<CurveNumberEdit>,
    /// Optimistic settings per fan, held until the service echoes them.
    pending_settings: HashMap<FanKey, FanSettings>,
    selected_curve: Option<String>,
    /// Keyboard focus anchor for the rename editor.
    focus_handle: FocusHandle,
    /// Whether the service has the current custom sensor definitions;
    /// cleared on config changes and reconnects to trigger a (re)send.
    customs_synced: bool,
    /// The window-resize observer is registered lazily on first render,
    /// where the `Window` handle is available.
    window_observer: bool,
    /// Latest windowed size (px) and maximized flag seen by the observer;
    /// flushed to disk by `refresh` so a resize drag doesn't write per event.
    window_size: Option<(f32, f32)>,
    window_maximized: bool,
    /// Geometry last written to disk, to skip redundant writes.
    saved_window: Option<(f32, f32, bool)>,
}

impl Zugluft {
    pub fn new(shared: Arc<Shared>, tx: Sender<Request>, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this.update(cx, |this, cx| this.refresh(cx)).is_err() {
                    break; // view dropped
                }
            }
        })
        .detach();

        let names = config::load();
        Self {
            shared,
            tx,
            seen_seq: 0,
            state: UiState::Connecting,
            active_view: AppView::Dashboard,
            dragging: None,
            pending: HashMap::new(),
            sensor_history: Vec::new(),
            temp_unit: names.temp_unit(),
            fan_unit: names.fan_unit(),
            names,
            names_mtime: config::mtime(),
            refresh_ticks: 0,
            toast: None,
            template_written: false,
            track_bounds: Rc::default(),
            graph_bounds: Rc::default(),
            graph_hover: None,
            sensor_search: TextEdit::new(String::new()),
            search_active: false,
            curve_dialog: None,
            curve_name_edit: None,
            custom_dialog: None,
            custom_name_edit: None,
            confirm_delete: None,
            open_dropdown: None,
            dropdown_widths: Rc::default(),
            curve_edit: None,
            curve_drag: None,
            curve_bounds: Rc::default(),
            pending_assign: HashMap::new(),
            last_curve: HashMap::new(),
            renaming: None,
            expanded: HashSet::new(),
            editing: None,
            curve_number_edit: None,
            pending_settings: HashMap::new(),
            selected_curve: None,
            focus_handle: cx.focus_handle(),
            customs_synced: false,
            window_observer: false,
            window_size: None,
            window_maximized: false,
            saved_window: None,
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let seq = self.shared.seq();
        if seq != self.seen_seq {
            self.seen_seq = seq;
            self.state = self.shared.state();
            self.record_sensor_frame();
            if self.dragging.is_none() {
                self.pending.clear();
            }
            // Optimistic settings are done once the service echoes them.
            if let UiState::Service(ServiceState::Ready { snapshots, .. }) = &self.state {
                self.pending_settings.retain(|key, pending| {
                    snapshots
                        .get(key.0)
                        .and_then(|snap| snap.fans.get(key.1))
                        .is_none_or(|fan| fan.settings != *pending)
                });
                self.pending_assign.retain(|key, pending| {
                    snapshots
                        .get(key.0)
                        .and_then(|snap| snap.fans.get(key.1))
                        .is_none_or(|fan| fan.curve != *pending)
                });
            }
            if !self.template_written
                && let UiState::Service(ServiceState::Ready {
                    chips, snapshots, ..
                }) = &self.state
            {
                config::write_template(chips, snapshots);
                self.template_written = true;
            }
            cx.notify();
        }

        // Hot-reload names and custom sensors when config.toml changes
        // (checked ~every 2 s).
        self.refresh_ticks = self.refresh_ticks.wrapping_add(1);
        if self.toast.as_ref().is_some_and(|toast| {
            self.refresh_ticks.wrapping_sub(toast.shown_tick) >= dashboard::TOAST_TICKS
        }) {
            self.toast = None;
            cx.notify();
        }
        if self.refresh_ticks.is_multiple_of(20) {
            let mtime = config::mtime();
            if mtime != self.names_mtime {
                self.names_mtime = mtime;
                self.names = config::load();
                self.temp_unit = self.names.temp_unit();
                self.fan_unit = self.names.fan_unit();
                self.customs_synced = false;
                cx.notify();
            }
        }

        // Keep the service's custom sensor and curve definitions in step
        // with the config, resending after edits and reconnects.
        match &self.state {
            UiState::Service(ServiceState::Ready { .. }) if !self.customs_synced => {
                let defs = self.names.customs().to_vec();
                let _ = self.tx.send(Request::SetCustomSensors(defs));
                let curves = self.names.curves().to_vec();
                let _ = self.tx.send(Request::SetCurves(curves));
                self.customs_synced = true;
            }
            UiState::Connecting | UiState::ServiceUnavailable => self.customs_synced = false,
            _ => {}
        }

        // Persist the window size when it settles (the observer keeps
        // `window_size` current; this writes only on an actual change).
        if let Some((w, h)) = self.window_size {
            let current = (w.round(), h.round(), self.window_maximized);
            if self.saved_window != Some(current) {
                self.saved_window = Some(current);
                config::save_window(config::WindowGeometry {
                    width: current.0,
                    height: current.1,
                    maximized: current.2,
                });
            }
        }

        // While a text input is open, re-render every tick so its caret
        // blinks (state changes alone don't fire often enough).
        if self.renaming.is_some()
            || self.editing.is_some()
            || self.curve_number_edit.is_some()
            || self.search_active
        {
            cx.notify();
        }
    }

    fn record_sensor_frame(&mut self) {
        let UiState::Service(ServiceState::Ready {
            snapshots, customs, ..
        }) = &self.state
        else {
            return;
        };

        let mut readings = HashMap::new();
        for (i, custom) in customs.iter().enumerate() {
            let Some(value) = custom.value else { continue };
            let key = SensorKey {
                kind: SensorKind::Custom,
                chip: 0,
                index: i,
            };
            readings.insert(key, value);
        }
        for (ci, snapshot) in snapshots.iter().enumerate() {
            for (ti, temperature) in snapshot.temps.iter().enumerate() {
                let Some(value) = temperature else {
                    continue;
                };
                let key = SensorKey {
                    kind: SensorKind::Temperature,
                    chip: ci,
                    index: ti,
                };
                readings.insert(key, *value);
            }
            for (fi, fan) in snapshot.fans.iter().enumerate() {
                let Some(rpm) = fan.rpm else {
                    continue;
                };
                let key = SensorKey {
                    kind: SensorKind::FanRpm,
                    chip: ci,
                    index: fi,
                };
                readings.insert(key, rpm);
            }
            for (pi, power) in snapshot.powers.iter().enumerate() {
                let Some(value) = power else {
                    continue;
                };
                let key = SensorKey {
                    kind: SensorKind::Power,
                    chip: ci,
                    index: pi,
                };
                readings.insert(key, *value);
            }
        }

        if readings.is_empty() {
            return;
        }

        self.sensor_history.push(SensorFrame { readings });
        if self.sensor_history.len() > HISTORY_LIMIT {
            let overflow = self.sensor_history.len() - HISTORY_LIMIT;
            self.sensor_history.drain(0..overflow);
        }
    }

    fn remember_window_handle(window: &mut Window) {
        if let Ok(handle) = window.window_handle()
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            winutil::remember_main_window(handle.hwnd.get());
        }
    }

    fn ensure_window_observer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.window_observer {
            return;
        }

        self.window_observer = true;
        cx.observe_window_bounds(window, |this, window, _cx| {
            let bounds = window.window_bounds().get_bounds();
            this.window_size = Some((f32::from(bounds.size.width), f32::from(bounds.size.height)));
            this.window_maximized = window.is_maximized();
        })
        .detach();
    }

    fn render_body_and_modals(&self, cx: &mut Context<Self>) -> (Div, Option<Div>, Option<Div>) {
        match &self.state {
            UiState::Connecting => (
                self.render_message("Connecting to service...", vec![], None),
                None,
                None,
            ),
            UiState::ServiceUnavailable => (self.render_service_unavailable(cx), None, None),
            UiState::Service(ServiceState::Detecting) => (
                self.render_message("Detecting hardware...", vec![], None),
                None,
                None,
            ),
            UiState::Service(ServiceState::Failed { error }) => {
                (self.render_failed(error, cx), None, None)
            }
            UiState::Service(ServiceState::Calibrating { message }) => (
                self.render_message(
                    "Calibrating fans...",
                    vec![
                        message.clone(),
                        "Fans step from full speed down to a stop test; previous duties are \
                         restored afterwards."
                            .to_string(),
                    ],
                    None,
                ),
                None,
                None,
            ),
            UiState::Service(ServiceState::Ready {
                chips,
                snapshots,
                notes,
                customs,
                // The GUI evaluates curves locally (same code path via
                // zugluft_ipc); the published statuses are for the CLI.
                curves: _,
            }) => {
                let dialog = self
                    .curve_dialog
                    .as_deref()
                    .and_then(|id| self.render_curve_dialog(id, chips, snapshots, customs, cx))
                    .or_else(|| {
                        self.custom_dialog.as_deref().and_then(|id| {
                            self.render_custom_dialog(id, chips, snapshots, customs, cx)
                        })
                    });
                let confirm = self
                    .confirm_delete
                    .as_ref()
                    .and_then(|id| self.render_confirm_delete(id, cx));
                (
                    self.render_ready(chips, snapshots, notes, customs, cx),
                    dialog,
                    confirm,
                )
            }
        }
    }

    fn render_dropdown_overlay(&self, cx: &mut Context<Self>) -> Option<Div> {
        self.open_dropdown.is_some().then(|| {
            // Swallow the whole click and close on mouse-up. Closing on
            // mouse-down would remove this overlay before the paired mouse-up,
            // which then leaks to the modal backdrop and closes the dialog too.
            div()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseUpEvent, _, cx| {
                        cx.stop_propagation();
                        this.open_dropdown = None;
                        cx.notify();
                    }),
                )
        })
    }

    fn handle_root_mouse_down(&mut self, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            self.commit_edit(cx);
        } else if self.curve_number_edit.is_some() {
            self.commit_curve_number_edit(cx);
        } else if self.search_active {
            self.search_active = false;
            cx.notify();
        }
    }

    fn handle_root_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            self.handle_edit_key(event, cx);
        } else if self.curve_number_edit.is_some() {
            self.handle_curve_number_key(event, cx);
        } else if self.renaming.is_some() {
            self.handle_rename_key(event, cx);
        } else if self.search_active {
            self.handle_search_key(event, cx);
        } else if self.custom_dialog.is_some() && self.custom_name_edit.is_some() {
            self.handle_custom_name_key(event, cx);
        } else if event.keystroke.key.as_str() == "escape" {
            if self.open_dropdown.take().is_some() || self.confirm_delete.take().is_some() {
                cx.notify();
            } else if self.curve_dialog.is_some() {
                self.close_curve_dialog(cx);
            } else if self.custom_dialog.is_some() {
                self.close_custom_dialog(cx);
            }
        } else if self.curve_dialog.is_some() && self.curve_name_edit.is_some() {
            self.handle_curve_name_key(event, cx);
        }
    }
}

impl Render for Zugluft {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let maximized = window.is_maximized();
        Self::remember_window_handle(window);
        self.ensure_window_observer(window, cx);
        let (body, dialog, confirm) = self.render_body_and_modals(cx);

        // Rename happens in a modal (over any view), not in-row.
        let rename_modal = self
            .renaming
            .clone()
            .map(|rename| self.render_rename_dialog(&rename, cx));

        // Click-away for dropdown popups: painted above the page (and the
        // dialog), below the deferred popups; the popup itself stops the
        // mouse down from reaching this.
        let dropdown_overlay = self.render_dropdown_overlay(cx);
        let toast = self.render_toast();

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_sm()
            .font_family(FONT_SANS)
            .track_focus(&self.focus_handle)
            // Click anywhere outside an inline input blurs it (commit a tuning
            // field, close search). The inputs stop_propagation on their own
            // mouse-down so clicking inside them doesn't trigger this; modals
            // dismiss via their backdrop.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.handle_root_mouse_down(cx);
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.handle_root_key_down(event, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                this.drag_move(event.position.x, cx);
                this.curve_drag_move(event.position, cx);
                this.update_graph_hover(event.position, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.end_curve_drag(cx);
                    this.end_drag(cx)
                }),
            )
            .child(self.render_titlebar(maximized, cx))
            .child(body)
            .children(dialog)
            .children(confirm)
            .children(rename_modal)
            .children(dropdown_overlay)
            .children(toast)
    }
}
