// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

//! The few things Settings lets you choose, in settings.json beside the cache. Open at Login is
//! not here: macOS keeps that (see login_item.rs).

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ScanEvery {
    /// One full scan a day, when the Mac is idle and on power.
    Daily,
    Hourly,
    Off,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Match the Mac.
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// Days of per-minute history kept. Daily disk totals are kept for a year regardless.
    pub history_days: u32,
    pub scan: ScanEvery,
    /// Also kept by the page (localStorage) to paint before it loads; here so the native window
    /// gets the right colour before the page exists.
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            history_days: 14,
            scan: ScanEvery::Daily,
            theme: Theme::System,
        }
    }
}

/// The settings as last read, and the file's modification time then: the app and the
/// background helper each read settings.json, so a change made in one reaches the other.
static CURRENT: Mutex<Option<(Settings, Option<std::time::SystemTime>)>> = Mutex::new(None);

fn path() -> std::path::PathBuf {
    crate::cache::dir().join("settings.json")
}

fn modified() -> Option<std::time::SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}

pub fn get() -> Settings {
    let mut cur = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
    let m = modified();
    if cur.as_ref().is_none_or(|(_, seen)| *seen != m) {
        let s = std::fs::read(path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        *cur = Some((s, m));
    }
    cur.as_ref().map(|(s, _)| s.clone()).unwrap_or_default()
}

pub fn set(mut s: Settings) -> Result<Settings, String> {
    s.history_days = s.history_days.clamp(1, 14);
    let bytes = serde_json::to_vec_pretty(&s).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(crate::cache::dir()).map_err(|e| e.to_string())?;
    // Written whole and renamed into place, so the other process never reads half of it.
    let tmp = path().with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path()).map_err(|e| e.to_string())?;
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some((s.clone(), modified()));
    Ok(s)
}

#[tauri::command]
pub fn get_settings() -> Settings {
    get()
}

#[tauri::command]
pub async fn save_settings(settings: Settings) -> Result<Settings, String> {
    let s = set(settings)?;
    crate::history::prune(s.history_days);
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_fill_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"scan":"hourly"}"#).unwrap();
        assert_eq!(s.history_days, 14);
        assert_eq!(s.scan, ScanEvery::Hourly);
    }
}
