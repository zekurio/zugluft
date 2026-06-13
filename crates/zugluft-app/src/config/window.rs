//! Persisted window geometry (last size + maximized state). Kept in its own
//! file beside config.toml so it never tangles with the hot-reloaded,
//! user-editable display-name config.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::config_path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
}

fn window_path() -> Option<PathBuf> {
    Some(config_path()?.with_file_name("window.toml"))
}

pub fn load_window() -> Option<WindowGeometry> {
    let text = std::fs::read_to_string(window_path()?).ok()?;
    toml::from_str(&text).ok()
}

/// Best-effort, like the other config writes — a failure here just means the
/// next launch falls back to the default size.
pub fn save_window(geom: WindowGeometry) {
    let Some(path) = window_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = toml::to_string(&geom) {
        let _ = std::fs::write(path, text);
    }
}
