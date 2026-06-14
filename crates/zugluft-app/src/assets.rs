//! Embedded assets: sidebar icons and the IBM Plex font family. Everything
//! ships inside the binary so the app has no install-time file dependencies.

use std::borrow::Cow;

use gpui::{App, AssetSource, Result, SharedString};

pub const FONT_SANS: &str = "IBM Plex Sans";
pub const FONT_MONO: &str = "IBM Plex Mono";

const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/dashboard.svg",
        include_bytes!("../assets/icons/dashboard.svg"),
    ),
    ("icons/fan.svg", include_bytes!("../assets/icons/fan.svg")),
    (
        "icons/spline.svg",
        include_bytes!("../assets/icons/spline.svg"),
    ),
    (
        "icons/thermometer.svg",
        include_bytes!("../assets/icons/thermometer.svg"),
    ),
    (
        "icons/pencil.svg",
        include_bytes!("../assets/icons/pencil.svg"),
    ),
    ("icons/pin.svg", include_bytes!("../assets/icons/pin.svg")),
    (
        "icons/trash.svg",
        include_bytes!("../assets/icons/trash.svg"),
    ),
    ("icons/eye.svg", include_bytes!("../assets/icons/eye.svg")),
    (
        "icons/eye-off.svg",
        include_bytes!("../assets/icons/eye-off.svg"),
    ),
    (
        "icons/settings.svg",
        include_bytes!("../assets/icons/settings.svg"),
    ),
    (
        "icons/search.svg",
        include_bytes!("../assets/icons/search.svg"),
    ),
    ("icons/plus.svg", include_bytes!("../assets/icons/plus.svg")),
    ("icons/wind.svg", include_bytes!("../assets/icons/wind.svg")),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}

/// Register the bundled IBM Plex faces with the text system.
pub fn load_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf") as &[u8]),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf") as &[u8]),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf") as &[u8]),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf") as &[u8]),
    ];
    cx.text_system()
        .add_fonts(fonts)
        .expect("failed to load bundled IBM Plex fonts");
}
