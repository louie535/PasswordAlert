use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What the person can change. Defaults are the original's numbers (see MECHANICS.md).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Warn with a pop-up when this many days or fewer are left (original: 10).
    pub warn_days: i64,
    /// Check every hour when this many days or fewer are left (original: 3).
    pub urgent_days: i64,
    /// Minutes between checks when urgent (original: 60).
    pub urgent_interval_min: u64,
    /// Minutes between checks otherwise (original: 1 day).
    pub normal_interval_min: u64,
    /// Show the warning box (original: always).
    pub popups: bool,
    /// Start hidden in the tray, like the original (which has no window at all).
    pub start_minimized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            warn_days: 10,
            urgent_days: 3,
            urgent_interval_min: 60,
            normal_interval_min: 24 * 60,
            popups: true,
            start_minimized: true,
        }
    }
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("PasswordAlert").join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}
