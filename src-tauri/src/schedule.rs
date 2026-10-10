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
use crate::{cache, history, monitor, volume};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

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

/// One minute's upkeep, done by whichever process is recording (see monitor.rs): the used
/// space hourly, pruning, and the scheduled scan of the disk, started by `scan_disk`.
fn upkeep(last_prune: &mut i64, scanning: bool, scan_disk: &dyn Fn()) {
    let now = history::now();
    if history::last_disk_used().is_none_or(|t| now - t >= 3600) {
        let v = volume::info("/");
        if v.total > 0 {
            history::add_disk_used(now, v.used, v.total);
        }
    }
    // Crash and hang reports, every 5 minutes (a directory listing; only new files are read).
    if now % 300 < 60 {
        crate::crashes::collect(settings::get().history_days);
    }
    if now - *last_prune >= 6 * 3600 {
        history::prune(settings::get().history_days);
        *last_prune = now;
    }
    if !scanning
        && due(
            settings::get().scan,
            history::last_scan("/").map(|t| now - t),
        )
    {
        scan_disk();
    }
}

/// The app's schedule. While the background helper records, the helper scans; the app picks
/// up the scan it leaves in the cache.
pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("schedule".into())
        .spawn(move || {
            let mut last_prune = 0i64;
            let mut seen: Option<SystemTime> = None;
            loop {
                let st: tauri::State<AppState> = app.state();
                if monitor::recording() {
                    upkeep(&mut last_prune, st.scanning.load(Ordering::SeqCst), &|| {
                        let _ = disk::start_scan_thread(&app, "/".into(), false);
                    });
                } else {
                    reload_if_newer(&app, &mut seen);
                }
                std::thread::sleep(Duration::from_secs(60));
            }
        })
        .expect("the schedule thread starts");
}

/// A scan another process saved for the root on screen replaces the one shown.
fn reload_if_newer(app: &AppHandle, seen: &mut Option<SystemTime>) {
    let st: tauri::State<AppState> = app.state();
    if st.scanning.load(Ordering::SeqCst) {
        return;
    }
    let root = st.root();
    let m = std::fs::metadata(cache::scan_path(&root))
        .and_then(|m| m.modified())
        .ok();
    if seen.is_none() || m.is_none() {
        *seen = m;
        return;
    }
    if m != *seen {
        *seen = m;
        if let Some(s) = cache::load_scan(&root) {
            if let Ok(mut cur) = st.scan.lock() {
                *cur = Some(Arc::new(s));
            }
            let _ = app.emit("scan-finished", &root);
        }
    }
}

/// A scheduled scan is running in the background helper.
static HEADLESS_SCANNING: AtomicBool = AtomicBool::new(false);

/// The background helper's schedule: the same upkeep, scanning on a thread of its own.
pub fn start_headless() {
    std::thread::Builder::new()
        .name("schedule".into())
        .spawn(|| {
            let mut last_prune = 0i64;
            loop {
                if monitor::recording() {
                    upkeep(
                        &mut last_prune,
                        HEADLESS_SCANNING.load(Ordering::SeqCst),
                        &|| {
                            HEADLESS_SCANNING.store(true, Ordering::SeqCst);
                            std::thread::spawn(|| {
                                let progress = crate::scan::Progress::default();
                                if let Some(s) = disk::walk_root("/", &progress) {
                                    disk::keep(&s);
                                }
                                HEADLESS_SCANNING.store(false, Ordering::SeqCst);
                            });
                        },
                    );
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
