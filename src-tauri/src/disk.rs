// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

//! The disk side: the scan on screen, scanning, and the Finder commands the map offers. A
//! finished scan is also handed to the history, which is how the app knows what grew.

use crate::{cache, history, scan, volume};
use serde::Serialize;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    /// The folder being shown. Persisted, so the app reopens on the same root.
    pub root: Mutex<String>,
    /// The scan on screen: the cached one at launch, replaced when a fresh scan completes.
    pub scan: Mutex<Option<Arc<cache::Scan>>>,
    pub progress: Arc<scan::Progress>,
    pub scanning: Arc<AtomicBool>,
    /// True while the cached scan is being read from disk at launch.
    pub loading: Arc<AtomicBool>,
    pub scan_started: Mutex<Option<Instant>>,
    /// The root the scan in flight is walking: a scheduled scan of the disk can run while
    /// another folder is on screen.
    pub scan_root: Mutex<String>,
}

impl AppState {
    pub fn new(root: String) -> Self {
        AppState {
            root: Mutex::new(root),
            scan: Mutex::new(None),
            progress: Default::default(),
            scanning: Default::default(),
            loading: Arc::new(AtomicBool::new(true)),
            scan_started: Mutex::new(None),
            scan_root: Mutex::new(String::new()),
        }
    }

    pub fn root(&self) -> String {
        self.root
            .lock()
            .map(|r| r.clone())
            .unwrap_or_else(|_| "/".into())
    }
}

#[derive(Serialize)]
pub struct ScanSummary {
    finished: String,
    duration_ms: u64,
    items: u64,
    size: u64,
    apparent: u64,
    denied: u64,
    cache_bytes: u64,
}

#[derive(Serialize)]
pub struct AppInfo {
    root: String,
    root_name: String,
    scanning: bool,
    loading: bool,
    scan: Option<ScanSummary>,
    volume: volume::Volume,
    recent: Vec<cache::Recent>,
    /// Whether the app can read a folder that only Full Disk Access opens.
    full_disk_access: bool,
    home: String,
}

#[derive(Serialize)]
pub struct ProgressInfo {
    scanning: bool,
    items: u64,
    bytes: u64,
    dirs: u64,
    denied: u64,
    current: String,
    elapsed_ms: u64,
    /// Items counted by the previous scan of this root, if there was one — what the bar is
    /// measured against.
    expected_items: Option<u64>,
    /// What is being scanned.
    root_name: String,
}

pub fn home() -> String {
    // The demo's home in screenshots, so its paths show as ~/...
    if crate::scene::active() {
        if let Ok(h) = std::env::var("TIDEMARK_SCENE_HOME") {
            return h;
        }
    }
    std::env::var("HOME").unwrap_or_else(|_| "/".into())
}

/// ~/Library/Safari is only readable with Full Disk Access (~/Library/CloudStorage is NOT a
/// valid probe: it is readable without Full Disk Access).
fn has_full_disk_access() -> bool {
    std::fs::read_dir(Path::new(&home()).join("Library/Safari")).is_ok()
}

#[tauri::command]
pub fn get_state(state: State<AppState>) -> AppInfo {
    let root = state.root();
    let scan = state.scan.lock().ok().and_then(|s| s.clone());
    AppInfo {
        root_name: volume::root_name(&root),
        scanning: state.scanning.load(Ordering::SeqCst),
        loading: state.loading.load(Ordering::SeqCst),
        scan: scan.map(|s| ScanSummary {
            finished: s.finished.clone(),
            duration_ms: s.duration_ms,
            items: s.items,
            size: s.size,
            apparent: s.apparent,
            denied: s.denied,
            cache_bytes: cache::scan_file_size(&s.root),
        }),
        volume: volume::info(&root),
        recent: cache::load_recent(),
        full_disk_access: has_full_disk_access(),
        home: home(),
        root,
    }
}

#[tauri::command]
pub fn get_progress(state: State<AppState>) -> ProgressInfo {
    let p = &state.progress;
    let scan_root = state
        .scan_root
        .lock()
        .map(|r| r.clone())
        .unwrap_or_default();
    let expected = state
        .scan
        .lock()
        .ok()
        .and_then(|s| s.as_ref().filter(|s| s.root == scan_root).map(|s| s.items));
    ProgressInfo {
        scanning: state.scanning.load(Ordering::SeqCst),
        items: p.items.load(Ordering::Relaxed),
        bytes: p.bytes.load(Ordering::Relaxed),
        dirs: p.dirs.load(Ordering::Relaxed),
        denied: p.denied.load(Ordering::Relaxed),
        current: p.current.lock().map(|c| c.clone()).unwrap_or_default(),
        elapsed_ms: state
            .scan_started
            .lock()
            .ok()
            .and_then(|s| *s)
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0),
        expected_items: expected,
        root_name: volume::root_name(&scan_root),
    }
}

/// The subtree under `path`, pruned to what can be drawn. `min_fraction` is the smallest
/// folder worth a block, as a share of the folder being viewed (the UI passes ~1/4000).
#[tauri::command(async)]
pub fn get_tree(
    state: State<AppState>,
    path: String,
    min_fraction: f64,
) -> Result<scan::View, String> {
    let scan = state
        .scan
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .ok_or("no scan yet")?;
    let root = scan.root.clone();
    let rel = if path == root {
        String::new()
    } else if root == "/" {
        path.trim_start_matches('/').to_string()
    } else {
        path.strip_prefix(&format!("{root}/"))
            .ok_or_else(|| format!("{path} is not under {root}"))?
            .to_string()
    };
    let parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
    let node = scan
        .tree
        .find(&parts)
        .ok_or_else(|| format!("{path} is not in the scan"))?;
    let depth = parts.len() as u32;
    let min_weight = ((scan::weight(node) as f64) * min_fraction.clamp(0.0, 1.0)) as u64;
    Ok(scan::view(
        node,
        &path,
        depth,
        min_weight.max(1),
        depth + 14,
    ))
}

/// Walks `root` completely. None if the walk was stopped or the root could not be read.
/// Nothing here needs the app, so the background helper scans the same way (recorder.rs).
pub fn walk_root(root: &str, progress: &scan::Progress) -> Option<cache::Scan> {
    let t0 = Instant::now();
    let name = volume::root_name(root);
    let tree = scan::walk(Path::new(root), name.clone(), false, progress)?;
    Some(cache::Scan {
        root: root.to_string(),
        root_name: name,
        finished: chrono::Local::now().to_rfc3339(),
        duration_ms: t0.elapsed().as_millis() as u64,
        items: tree.items,
        size: tree.size,
        apparent: tree.apparent,
        denied: tree.denied,
        tree,
    })
}

/// Keeps what a finished scan found: the recent list, the cache for the next launch, and the
/// history (which is how growth is known).
pub fn keep(s: &cache::Scan) {
    cache::push_recent(cache::Recent {
        path: s.root.clone(),
        name: s.root_name.clone(),
        size: s.size,
        items: s.items,
        when: s.finished.clone(),
    });
    if let Err(e) = cache::save_scan(s) {
        eprintln!("could not cache the scan: {e}");
    }
    history::add_scan(&s.root, history::now(), &s.tree);
    if s.root == "/" {
        let v = volume::info("/");
        history::add_disk_used(history::now(), v.used, v.total);
    }
}

/// Starts a scan of `root`. With `show` the app switches to that root (the user asked); without
/// it, the scan runs behind whatever is on screen (the schedule), and only replaces the scan on
/// screen if it is of the same root.
pub fn start_scan_thread(app: &AppHandle, root: String, show: bool) -> Result<(), String> {
    let state: State<AppState> = app.state();
    if !Path::new(&root).is_dir() {
        return Err(format!("{root} is not a folder"));
    }
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Err("a scan is already running".into());
    }
    // Switching root: show that root's cached scan (if any) straight away, and remember it.
    let switched = show && state.root() != root;
    if switched {
        if let Ok(mut r) = state.root.lock() {
            *r = root.clone();
        }
        cache::save_root(&root);
        let cached = cache::load_scan(&root).map(Arc::new);
        if let Ok(mut s) = state.scan.lock() {
            *s = cached;
        }
    }
    if let Ok(mut r) = state.scan_root.lock() {
        *r = root.clone();
    }
    state.progress.reset();
    if let Ok(mut s) = state.scan_started.lock() {
        *s = Some(Instant::now());
    }
    let progress = state.progress.clone();
    let scanning = state.scanning.clone();
    let handle = app.clone();
    std::thread::spawn(move || {
        if let Some(s) = walk_root(&root, &progress) {
            let st: State<AppState> = handle.state();
            // Only publish if the root is still the one this scan was for.
            let still = st.root() == root;
            if still {
                if let Ok(mut cur) = st.scan.lock() {
                    *cur = Some(Arc::new(s.clone()));
                }
            }
            // Kept first (cache, history), then announced: whoever reloads on scan-finished, and
            // the schedule watching `scanning`, must find it saved.
            keep(&s);
            scanning.store(false, Ordering::SeqCst);
            let _ = handle.emit("scan-finished", &root);
        } else {
            scanning.store(false, Ordering::SeqCst);
            let _ = handle.emit("scan-finished", &root);
        }
    });
    Ok(())
}

#[tauri::command(async)]
pub fn start_scan(app: AppHandle, root: Option<String>) -> Result<(), String> {
    let state: State<AppState> = app.state();
    let root = match root {
        Some(r) if !r.is_empty() => r,
        _ => state.root(),
    };
    start_scan_thread(&app, root, true)
}

/// Ends the scan in flight. The scan on screen stays what it was.
#[tauri::command]
pub fn stop_scan(state: State<AppState>) {
    state.progress.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command(async)]
pub fn reveal(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err(format!("{path} does not exist"));
    }
    Command::new("/usr/bin/open")
        .arg("-R")
        .arg(&path)
        .status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Finder's Get Info window for a path, via AppleScript (there is no URL scheme for it).
#[tauri::command(async)]
pub fn get_info(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err(format!("{path} does not exist"));
    }
    let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "tell application \"Finder\"\nactivate\nopen information window of (POSIX file \"{escaped}\" as alias)\nend tell"
    );
    Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command(async)]
pub fn open_privacy_settings() -> Result<(), String> {
    Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .status()
        .map_err(|e| e.to_string())?;
    Ok(())
}
