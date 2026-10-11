// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Screenshot mode, for tools/screenshots.py (after Keepr's). TIDEMARK_SCENE holds a scene as
//! JSON: the screen to open ("now", "performance", "disk", "trends", "settings"), or "tray": true
//! for the menu-bar window, and the theme. With it the app records nothing and scans nothing: it
//! shows what is in TIDEMARK_DATA (the demo's history, scan and live readings) in a window that
//! is invisible and takes no focus, saves that window's webview snapshot to TIDEMARK_SNAPSHOT
//! after TIDEMARK_SNAPSHOT_AFTER seconds, and is then stopped by the script.

use std::sync::OnceLock;

static SCENE: OnceLock<Option<serde_json::Value>> = OnceLock::new();

pub fn get() -> Option<&'static serde_json::Value> {
    SCENE
        .get_or_init(|| {
            std::env::var("TIDEMARK_SCENE")
                .ok()
                .filter(|s| !s.is_empty())
                .and_then(|s| serde_json::from_str(&s).ok())
        })
        .as_ref()
}

pub fn active() -> bool {
    get().is_some()
}

pub fn tray() -> bool {
    get().is_some_and(|s| s["tray"] == true)
}

/// The scene, for the page: which screen to open and in which theme.
#[tauri::command]
pub fn scene() -> Option<serde_json::Value> {
    get().cloned()
}

/// Makes a window invisible and click-through while it still draws, so nothing flashes on
/// screen while a shot is taken.
pub fn make_unseen(w: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    if let Ok(ns) = w.ns_window() {
        // Tauri's own NSWindow, alive as long as the window is; setup runs on the main thread.
        let window = unsafe { &*(ns as *const objc2_app_kit::NSWindow) };
        window.setAlphaValue(0.0);
        window.setIgnoresMouseEvents(true);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = w;
}

/// Saves what the window's webview shows to `dest` as a TIFF (an invisible window's own capture
/// is blank; its webview's snapshot isn't). The file appears, written whole, once WebKit has
/// drawn it.
pub fn snapshot(w: &tauri::WebviewWindow, dest: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let dest = dest.to_path_buf();
        w.with_webview(move |wv| unsafe {
            use objc2::runtime::AnyObject;
            let webview = wv.inner() as *mut AnyObject;
            let done = block2::RcBlock::new(move |image: *mut AnyObject, _error: *mut AnyObject| {
                if image.is_null() {
                    return;
                }
                let tiff: *mut AnyObject = objc2::msg_send![image, TIFFRepresentation];
                if !tiff.is_null() {
                    let path = objc2_foundation::NSString::from_str(&dest.to_string_lossy());
                    let _: bool = objc2::msg_send![tiff, writeToFile: &*path, atomically: true];
                }
            });
            let config: *mut AnyObject = std::ptr::null_mut();
            let _: () = objc2::msg_send![webview, takeSnapshotWithConfiguration: config, completionHandler: &*done];
        })
        .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (w, dest);
        Err("Snapshots are macOS only.".into())
    }
}

/// Sets the scene's window up unseen, and saves its snapshot once it has settled.
pub fn setup(app: &tauri::App) {
    use tauri::Manager;
    let label = if tray() { "tray" } else { "main" };
    let Some(w) = app.get_webview_window(label) else {
        return;
    };
    if !tray() {
        // macOS fits a window to its screen, so the shot's height depends on the screen it
        // opens on: always the tallest, so a second, shorter display doesn't crop the shot.
        if let Ok(ms) = w.available_monitors() {
            let tall = |m: &tauri::Monitor| m.size().height as f64 / m.scale_factor();
            if let Some(m) = ms.iter().max_by(|a, b| tall(a).total_cmp(&tall(b))) {
                let p = m.position();
                let _ = w.set_position(tauri::PhysicalPosition::new(p.x, p.y));
            }
        }
        let h = get().and_then(|s| s["height"].as_f64()).unwrap_or(940.0);
        let _ = w.set_size(tauri::LogicalSize::new(1400.0, h));
    } else {
        let _ = w.set_position(tauri::LogicalPosition::new(200.0, 120.0));
    }
    make_unseen(&w);
    let _ = w.show();
    if tray() {
        use tauri::Emitter;
        let _ = w.emit("tray-opened", ());
    }
    let Some(out) = std::env::var_os("TIDEMARK_SNAPSHOT") else {
        return;
    };
    let after = std::env::var("TIDEMARK_SNAPSHOT_AFTER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5.0);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs_f64(after));
        let _ = snapshot(&w, std::path::Path::new(&out));
    });
}
