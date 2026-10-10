// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

//! The disk's history, kept up behind the window: the boot volume's used space about hourly
//! (statfs, near free), a full scan of the disk on the schedule Settings sets, and pruning.
//!
//! "Daily" waits for a moment that costs nothing: the Mac idle for 5 minutes and on power. If
//! that never comes (a laptop always on battery), it scans when idle once the last scan is two
//! days old.

use crate::disk::{self, AppState};
use crate::settings::{self, ScanEvery};
use crate::{history, volume};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
}

/// Seconds since the last keyboard or mouse input.
fn idle_secs() -> f64 {
    #[cfg(target_os = "macos")]
    unsafe {
        // kCGEventSourceStateHIDSystemState, kCGAnyInputEventType.
        CGEventSourceSecondsSinceLastEventType(1, u32::MAX)
    }
    #[cfg(not(target_os = "macos"))]
    0.0
}

fn on_power() -> bool {
    std::process::Command::new("/usr/bin/pmset")
        .args(["-g", "ps"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("'AC Power'"))
        .unwrap_or(true)
}

/// Whether a scheduled scan of the disk is due now, `age` seconds after the last one.
fn due(every: ScanEvery, age: Option<i64>) -> bool {
    let age = age.unwrap_or(i64::MAX);
    match every {
        ScanEvery::Off => false,
        ScanEvery::Hourly => age >= 3600 - 60,
        ScanEvery::Daily => {
            age >= 20 * 3600 && idle_secs() >= 300.0 && (age >= 48 * 3600 || on_power())
        }
    }
}

pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("schedule".into())
        .spawn(move || {
            let mut last_prune = 0i64;
            loop {
                let now = history::now();
                if history::last_disk_used().is_none_or(|t| now - t >= 3600) {
                    let v = volume::info("/");
                    if v.total > 0 {
                        history::add_disk_used(now, v.used, v.total);
                    }
                }
                if now - last_prune >= 6 * 3600 {
                    history::prune(settings::get().history_days);
                    last_prune = now;
                }
                let st: tauri::State<AppState> = app.state();
                if !st.scanning.load(Ordering::SeqCst)
                    && due(
                        settings::get().scan,
                        history::last_scan("/").map(|t| now - t),
                    )
                {
                    let _ = disk::start_scan_thread(&app, "/".into(), false);
                }
                std::thread::sleep(Duration::from_secs(60));
            }
        })
        .expect("the schedule thread starts");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_and_hourly() {
        assert!(!due(ScanEvery::Off, None));
        assert!(due(ScanEvery::Hourly, None));
        assert!(!due(ScanEvery::Hourly, Some(600)));
        assert!(due(ScanEvery::Hourly, Some(3600)));
    }
}
