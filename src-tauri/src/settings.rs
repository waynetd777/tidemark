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

static CURRENT: Mutex<Option<Settings>> = Mutex::new(None);

fn path() -> std::path::PathBuf {
    crate::cache::dir().join("settings.json")
}

pub fn get() -> Settings {
    let mut cur = CURRENT.lock().unwrap();
    if cur.is_none() {
        *cur = Some(
            std::fs::read(path())
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default(),
        );
    }
    cur.clone().unwrap()
}

pub fn set(mut s: Settings) -> Result<Settings, String> {
    s.history_days = s.history_days.clamp(1, 14);
    let bytes = serde_json::to_vec_pretty(&s).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(crate::cache::dir()).map_err(|e| e.to_string())?;
    std::fs::write(path(), bytes).map_err(|e| e.to_string())?;
    *CURRENT.lock().unwrap() = Some(s.clone());
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
