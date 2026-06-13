//! zugluft — fast, simple fan control for Windows.
//!
//! This GUI runs unelevated; all hardware access happens in the
//! zugluft-service process, reached over a named pipe. Closing the GUI does
//! not change fan state — the service owns the fans.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod client;
mod config;
mod elevation;
mod tray;
mod ui;
mod winutil;

use std::sync::Arc;

use gpui::{
    App, AppContext as _, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px,
    size,
};

fn main() {
    let shared = Arc::new(client::Shared::default());
    let tx = client::spawn(shared.clone());
    let _tray = tray::spawn();

    Application::new()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            assets::load_fonts(cx);
            // Restore the last window size (centered); fall back to a sane
            // default. Tiny saved sizes are ignored so a glitch can't trap
            // the window below the usable layout.
            let saved = config::load_window();
            let target = saved
                .map(|g| size(px(g.width), px(g.height)))
                .filter(|s| s.width >= px(560.) && s.height >= px(420.))
                .unwrap_or_else(|| size(px(860.), px(620.)));
            let bounds = Bounds::centered(None, target, cx);
            let window_bounds = if saved.is_some_and(|g| g.maximized) {
                WindowBounds::Maximized(bounds)
            } else {
                WindowBounds::Windowed(bounds)
            };
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    // Below this the graph + sensor list can't lay out cleanly.
                    window_min_size: Some(size(px(560.), px(420.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("zugluft".into()),
                        // We paint our own flush, Zed-like title bar in the app.
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| ui::Zugluft::new(shared.clone(), tx.clone(), cx)),
            )
            .expect("failed to open window");
            cx.activate(true);

            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
        });
}
