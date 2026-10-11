// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Checks GitHub for a newer release and offers to install it (tauri-plugin-updater: the
//! endpoint, releases/latest/download/latest.json, and the public key that every package must
//! be signed with are in tauri.conf.json under plugins.updater; tools/update_files.py writes
//! the release's files).
//!
//! When: 20 seconds after launch, then once a day while the app runs (the last check's time is
//! kept, so restarting doesn't check again), and whenever the user picks Check for Updates….
//! An update found in the background waits until the main window is next focused, so the offer
//! never pops up over another app. It offers Install and Restart, Later (asked again at the next
//! day's check) or Skip This Version. Release builds only, and the windowed app only: never the
//! background helper.
//!
//! Installing does what `make install-app` does. The helper runs from the same bundle, so its
//! agent is unloaded while the bundle is swapped and loaded again after, running the new
//! binary. The app then restarts through the normal quit (RunEvent::Exit: the minute in hand is
//! saved, incidents end, Ask's tools stop, the window's place is kept).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};
use tauri_plugin_updater::{Update, UpdaterExt};

const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const TICK: Duration = Duration::from_secs(60 * 60);

const INSTALL: &str = "Install and Restart";
const LATER: &str = "Later";
const SKIP: &str = "Skip This Version";

/// Why an update can't be installed right now ("A scan is running."), or None when it can.
pub type Busy = fn(&AppHandle) -> Option<String>;

pub struct Updates {
    pending: Mutex<Option<Update>>,
    asking: AtomicBool,
    busy: Busy,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    checked: u64,
    #[serde(default)]
    skipped: Option<String>,
}

fn saved_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("updates.json"))
}

fn load(app: &AppHandle) -> Saved {
    saved_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, s: &Saved) {
    let Some(p) = saved_path(app) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(b) = serde_json::to_vec_pretty(s) {
        let _ = std::fs::write(p, b);
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn app_name(app: &AppHandle) -> String {
    app.package_info().name.clone()
}

/// Starts the background checks. Call once from setup, in the windowed app only.
pub fn start(app: &AppHandle, busy: Busy) {
    app.manage(Updates {
        pending: Mutex::new(None),
        asking: AtomicBool::new(false),
        busy,
    });
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            if now().saturating_sub(load(&app).checked) >= EVERY.as_secs() {
                tauri::async_runtime::block_on(check(&app, false));
            }
            std::thread::sleep(TICK);
        }
    });
}

/// Check for Updates…: says what it found, even when there's nothing new.
pub fn check_now(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // A development build would be replaced by the release.
        if cfg!(debug_assertions) || app.try_state::<Updates>().is_none() {
            let text = "Development builds don't update; make install-app does.";
            return tell(&app, "Updates are off", text).await;
        }
        check(&app, true).await
    });
}

/// The main window gained focus: offer an update a background check found.
pub fn focused(app: &AppHandle) {
    let Some(state) = app.try_state::<Updates>() else {
        return;
    };
    let Some(update) = state.pending.lock().unwrap().take() else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move { offer(&app, update).await });
}

async fn check(app: &AppHandle, asked: bool) {
    let found = match app.updater() {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    let mut saved = load(app);
    match found {
        Ok(Some(update)) => {
            saved.checked = now();
            save(app, &saved);
            if asked {
                offer(app, update).await;
            } else if saved.skipped.as_deref() != Some(update.version.as_str()) {
                let focused = app
                    .get_webview_window("main")
                    .and_then(|w| w.is_focused().ok())
                    .unwrap_or(false);
                if focused {
                    offer(app, update).await;
                } else if let Some(state) = app.try_state::<Updates>() {
                    *state.pending.lock().unwrap() = Some(update);
                }
            }
        }
        Ok(None) => {
            saved.checked = now();
            save(app, &saved);
            if asked {
                let v = app.package_info().version.to_string();
                let text = format!("{} {v} is the newest version.", app_name(app));
                tell(app, "You're up to date", &text).await;
            }
        }
        Err(e) => {
            // A failed background check is tried again within the hour.
            eprintln!("update check failed: {e}");
            if asked {
                tell(app, "Couldn't check for updates", &e.to_string()).await;
            }
        }
    }
}

async fn offer(app: &AppHandle, update: Update) {
    let Some(state) = app.try_state::<Updates>() else {
        return;
    };
    if state.asking.swap(true, Ordering::SeqCst) {
        return;
    }
    let name = app_name(app);
    let mut text = format!(
        "{name} {} is available. You have {}.",
        update.version, update.current_version
    );
    if let Some(notes) = update
        .body
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        text.push_str("\n\n");
        text.push_str(notes);
    }
    let dialog = app
        .dialog()
        .message(text)
        .title(format!("A new version of {name} is available"))
        .buttons(MessageDialogButtons::YesNoCancelCustom(
            INSTALL.into(),
            LATER.into(),
            SKIP.into(),
        ));
    let choice = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show_with_result())
        .await
        .unwrap_or(MessageDialogResult::Cancel);
    state.asking.store(false, Ordering::SeqCst);
    // Custom buttons come back as their labels; anything else (Esc, closed) is Later.
    match choice {
        MessageDialogResult::Custom(c) if c == INSTALL => install(app, update).await,
        MessageDialogResult::Custom(c) if c == SKIP => {
            let mut saved = load(app);
            saved.skipped = Some(update.version.clone());
            save(app, &saved);
        }
        _ => {}
    }
}

async fn install(app: &AppHandle, update: Update) {
    let state = app.state::<Updates>();
    if let Some(why) = (state.busy)(app) {
        let text = format!("{why} {} will offer it again later.", app_name(app));
        tell(app, "The update will wait", &text).await;
        return;
    }
    // Downloaded first, so the helper stops only for the swap itself.
    let bytes = match update.download(|_, _| {}, || {}).await {
        Ok(b) => b,
        Err(e) => return tell(app, "The update didn't download", &e.to_string()).await,
    };
    let installed = tauri::async_runtime::spawn_blocking(move || {
        let helper = crate::recorder::stop_for_update();
        let done = update.install(bytes);
        // Loaded again whether or not the swap worked: from the new binary, or the old one.
        if helper {
            crate::recorder::start_after_update();
        }
        done.map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match installed {
        // Through RunEvent::Exit, as a quit is: AppHandle::restart skips it on the main thread.
        Ok(()) => app.request_restart(),
        Err(e) => tell(app, "The update didn't install", &e).await,
    }
}

async fn tell(app: &AppHandle, title: &str, text: &str) {
    let dialog = app
        .dialog()
        .message(text)
        .title(title)
        .kind(MessageDialogKind::Info);
    let _ = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show()).await;
}
