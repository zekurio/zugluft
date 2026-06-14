use super::*;

impl Zugluft {
    pub(super) fn install_service(&mut self, cx: &mut Context<Self>) {
        if let Some(exe) = service_exe() {
            // The client thread reconnects automatically once the service
            // is up; nothing else to do here.
            elevation::run_elevated(&exe, "install");
        }
        cx.notify();
    }

    pub(super) fn button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<String>,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        div().child(
            div()
                .id(id)
                .px_3()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(BORDER))
                .text_sm()
                .text_color(rgb(TEXT))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on_click(this, cx)))
                .child(label.into()),
        )
    }

    pub(super) fn icon_button(
        &self,
        id: impl Into<ElementId>,
        icon: &'static str,
        label: impl Into<String>,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        div().child(
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_1p5()
                .px_2p5()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(BORDER))
                .text_sm()
                .text_color(rgb(TEXT))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on_click(this, cx)))
                .child(svg().path(icon).w(px(13.)).h(px(13.)).text_color(rgb(TEXT)))
                .child(label.into()),
        )
    }

    pub(super) fn modal_panel(
        &self,
        id: impl Into<ElementId>,
        width: Pixels,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .w(width)
            .max_w(relative(1.))
            .max_h(relative(1.))
            .min_w(px(0.))
            .min_h(px(0.))
            .flex()
            .flex_col()
            .rounded_lg()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(floating_shadow())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
    }

    pub(super) fn modal_backdrop(
        &self,
        panel: impl IntoElement,
        cx: &mut Context<Self>,
        on_close: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_2()
            .bg(hsla(0.0, 0.0, 0.0, 0.55))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| on_close(this, cx)),
            )
            .child(panel)
    }

    /// One icon-only entry in the navigation rail. The active view gets an
    /// accent-tinted icon on a raised tile.
    pub(super) fn nav_item(&self, view: AppView, cx: &mut Context<Self>) -> Div {
        let active = self.active_view == view;
        div().child(
            div()
                .id(("nav", view.id()))
                .w(px(36.))
                .h(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgb(if active { TRACK } else { BG }))
                .text_color(rgb(if active { FILL_MANUAL } else { TEXT_DIM }))
                .cursor_pointer()
                .hover(move |s| {
                    let s = s.bg(rgb(if active { TRACK } else { FILL_HOVER }));
                    if active { s } else { s.text_color(rgb(TEXT)) }
                })
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.select_view(view, cx)),
                )
                .child(
                    svg()
                        .path(view.icon())
                        .w(px(18.))
                        .h(px(18.))
                        .text_color(rgb(if active { FILL_MANUAL } else { TEXT_DIM })),
                ),
        )
    }

    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> Div {
        // A floating rounded pill, matching the card surfaces it sits beside.
        div()
            .flex_none()
            .my_2()
            .ml_2()
            .w(px(48.))
            .flex()
            .flex_col()
            .items_center()
            .gap_1p5()
            .py_2()
            .rounded_lg()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(subtle_shadow())
            .child(self.nav_item(AppView::Dashboard, cx))
            .child(self.nav_item(AppView::Curves, cx))
            .child(self.nav_item(AppView::Fans, cx))
            .child(self.nav_item(AppView::Telemetry, cx))
            .child(div().flex_1())
            .child(self.nav_item(AppView::Settings, cx))
    }

    /// A single window-control button (minimize / maximize / close). Lives in
    /// the client area so it gets hover feedback; the click drives the native
    /// window via the platform `Window` methods.
    pub(super) fn window_button(
        &self,
        id: &'static str,
        glyph: &'static str,
        danger: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Window) + 'static,
    ) -> Div {
        let hover_bg = if danger { ERROR } else { FILL_HOVER };
        div().h_full().child(
            div()
                .id(id)
                .w(px(46.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                // Windows caption glyphs; they share a baseline and weight.
                .font_family("Segoe Fluent Icons")
                .text_size(px(10.))
                .text_color(rgb(TEXT_DIM))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(hover_bg)).text_color(rgb(TEXT)))
                .on_click(cx.listener(move |_, _: &ClickEvent, window, _| on_click(window)))
                .child(glyph),
        )
    }

    /// Flush, Zed-like title bar painted by the app itself: the wordmark in
    /// monospace on the left, a draggable region, and the window controls.
    pub(super) fn render_titlebar(&self, maximized: bool, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .h(px(40.))
            // Never yield height to an overflowing page: flex shrink would
            // squeeze the titlebar in short windows.
            .flex_none()
            .bg(rgb(BG))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .px_4()
                    // No WindowControlArea::Drag here: gpui 0.2.2 either
                    // ignores it (hit test broken on Windows) or routes the
                    // press down the non-client path where gpui swallows it
                    // before DefWindowProc can start the move loop. Posting
                    // the caption-drag syscommand runs the native loop, so
                    // Aero Snap and maximized-drag-restore behave normally.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, event: &MouseDownEvent, _, _| {
                            if event.click_count >= 2 {
                                winutil::toggle_maximize();
                            } else {
                                winutil::begin_titlebar_drag();
                            }
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .font_family(FONT_MONO)
                            .child(
                                div()
                                    .w(px(22.))
                                    .h(px(22.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(rgb(BG))
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        svg()
                                            .path("icons/wind.svg")
                                            .w(px(15.))
                                            .h(px(15.))
                                            .text_color(rgb(FILL_MANUAL)),
                                    ),
                            )
                            .child(div().text_color(rgb(TEXT_DIM)).child("zugluft"))
                            .child(div().text_color(rgb(TEXT_DIM)).child("›"))
                            .child(div().text_color(rgb(TEXT)).child(self.active_view.label())),
                    ),
            )
            .child(self.window_button("win-min", "\u{e921}", false, cx, |w| w.minimize_window()))
            .child(self.window_button(
                "win-max",
                // Maximize ⇄ restore glyphs; gpui's zoom() can't restore on
                // Windows (0.2.2), so winutil toggles it.
                if maximized { "\u{e923}" } else { "\u{e922}" },
                false,
                cx,
                |_| winutil::toggle_maximize(),
            ))
            .child(self.window_button("win-close", "\u{e8bb}", true, cx, |w| w.remove_window()))
    }

    pub(super) fn render_message(
        &self,
        title: &str,
        lines: Vec<String>,
        action: Option<Div>,
    ) -> Div {
        div().flex_1().flex().items_center().justify_center().child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .items_center()
                .max_w(px(560.))
                .child(div().text_lg().child(title.to_string()))
                .children(
                    lines
                        .into_iter()
                        .map(|line| div().text_sm().text_color(rgb(TEXT_DIM)).child(line)),
                )
                .children(action),
        )
    }

    pub(super) fn render_service_unavailable(&self, cx: &mut Context<Self>) -> Div {
        let (lines, action) = if service_exe().is_some() {
            (
                vec![
                    "The zugluft service does all hardware access, so the app never needs UAC."
                        .to_string(),
                    "Install it once (one elevation prompt); it starts with Windows.".to_string(),
                ],
                Some(self.button(
                    "install-service",
                    "Install & start service",
                    cx,
                    |this, cx| this.install_service(cx),
                )),
            )
        } else {
            (
                vec![
                    "zugluft-service.exe was not found next to the app.".to_string(),
                    "Build it with `cargo build -p zugluft-service` or start it manually."
                        .to_string(),
                ],
                None,
            )
        };
        self.render_message("zugluft service not running", lines, action)
    }

    pub(super) fn render_failed(&self, error: String, cx: &mut Context<Self>) -> Div {
        let action = self.button("redetect", "Retry detection", cx, |this, cx| {
            let _ = this.tx.send(Request::Redetect);
            cx.notify();
        });
        self.render_message(
            "Hardware unavailable",
            vec![
                error,
                "The service retries automatically every 30 s.".to_string(),
            ],
            Some(action),
        )
    }
}
