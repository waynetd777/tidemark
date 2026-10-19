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
//! The offer, the download's progress and every message are shown by the main window's own
//! dialog (src/Update.tsx), sent as the events `update-offer`, `update-progress` and
//! `update-message`; the window answers with update_install, update_later or update_skip. The
//! native alert is kept only for when there is no main window to show them.
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
use tauri::{AppHandle, Emitter, Manager};
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
    /// Found in the background, waiting for the main window's focus.
    pending: Mutex<Option<Update>>,
    /// Offered in the window, waiting for its answer.
    offered: Mutex<Option<Update>>,
    /// An offer is on screen or being installed: no second one until it is answered.
    asking: AtomicBool,
    busy: Busy,
}

/// What the window's dialog shows of an offer.
#[derive(Clone, Serialize)]
pub struct Offer {
    name: String,
    version: String,
    current: String,
    notes: Option<String>,
    /// The release's day, 2026-10-11.
    date: Option<String>,
}

impl Offer {
    fn of(app: &AppHandle, u: &Update) -> Offer {
        Offer {
            name: app_name(app),
            version: u.version.clone(),
            current: u.current_version.clone(),
            notes: u
                .body
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(String::from),
            date: u.date.map(|d| d.date().to_string()),
        }
    }
}

/// The install's progress: "downloading" (done of total bytes), then "installing", then
/// "restarting".
#[derive(Clone, Serialize)]
struct Progress {
    phase: &'static str,
    done: u64,
    total: Option<u64>,
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
    #[cfg(windows)]
    remove_old_installers(&app_name(app));
    app.manage(Updates {
        pending: Mutex::new(None),
        offered: Mutex::new(None),
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

/// The installers the updater downloaded, which it leaves in %TEMP%\<name>-<version>-updater-…
/// once they have run.
#[cfg(windows)]
fn remove_old_installers(name: &str) {
    let Ok(dir) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let prefix = format!("{name}-");
    for e in dir.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with(&prefix) && n.contains("-updater-") && e.path().is_dir() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
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
                // A release whose Windows installer isn't published yet has no entry for it.
                let text = if e.to_string().to_lowercase().contains("platform") {
                    "Updates for this system aren't published yet.".to_string()
                } else {
                    e.to_string()
                };
                tell(app, "Couldn't check for updates", &text).await;
            }
        }
    }
}

/// Shows the main window, to put the dialog in; false when there is none.
fn show_window(app: &AppHandle) -> bool {
    if app.get_webview_window("main").is_none() {
        return false;
    }
    // Check for Updates… can come while the window is closed. AppKit wants the main thread;
    // a hidden window's page still gets the event sent meanwhile.
    let a = app.clone();
    let _ = app.run_on_main_thread(move || crate::tray::show_main(&a));
    true
}

async fn offer(app: &AppHandle, update: Update) {
    let Some(state) = app.try_state::<Updates>() else {
        return;
    };
    if state.asking.swap(true, Ordering::SeqCst) {
        // Already on screen: brought forward, as Check for Updates… asked to see it.
        show_window(app);
        return;
    }
    if show_window(app) {
        let offer = Offer::of(app, &update);
        *state.offered.lock().unwrap() = Some(update);
        let _ = app.emit_to("main", "update-offer", offer);
        return;
    }
    let name = app_name(app);
    let o = Offer::of(app, &update);
    let mut text = format!("{name} {} is available. You have {}.", o.version, o.current);
    if let Some(notes) = o.notes {
        text.push_str("\n\n");
        text.push_str(&notes);
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
    // Custom buttons come back as their labels; anything else (Esc, closed) is Later.
    match choice {
        MessageDialogResult::Custom(c) if c == INSTALL => install(app, update).await,
        MessageDialogResult::Custom(c) if c == SKIP => skip(app, &update),
        _ => {}
    }
    state.asking.store(false, Ordering::SeqCst);
}

fn skip(app: &AppHandle, update: &Update) {
    let mut saved = load(app);
    saved.skipped = Some(update.version.clone());
    save(app, &saved);
}

/// The offer on screen, taken: None when it has been answered already (a second click).
fn answered(app: &AppHandle) -> Option<Update> {
    app.try_state::<Updates>()?.offered.lock().unwrap().take()
}

fn done_asking(app: &AppHandle) {
    if let Some(state) = app.try_state::<Updates>() {
        state.asking.store(false, Ordering::SeqCst);
    }
}

/// The offer the window should be showing, for a page that loaded after it was sent.
#[tauri::command]
pub fn update_offered(app: AppHandle) -> Option<Offer> {
    let state = app.try_state::<Updates>()?;
    let offered = state.offered.lock().unwrap();
    offered.as_ref().map(|u| Offer::of(&app, u))
}

/// Install and Restart. Progress and any failure come back as events.
#[tauri::command]
pub fn update_install(app: AppHandle) {
    let Some(update) = answered(&app) else { return };
    tauri::async_runtime::spawn(async move {
        install(&app, update).await;
        done_asking(&app);
    });
}

/// Later, Esc or the dialog closed: asked again at the next day's check.
#[tauri::command]
pub fn update_later(app: AppHandle) {
    if answered(&app).is_some() {
        done_asking(&app);
    }
}

/// Skip This Version: not offered again, unless asked for with Check for Updates….
#[tauri::command]
pub fn update_skip(app: AppHandle) {
    if let Some(update) = answered(&app) {
        skip(&app, &update);
        done_asking(&app);
    }
}

async fn install(app: &AppHandle, update: Update) {
    let state = app.state::<Updates>();
    if let Some(why) = (state.busy)(app) {
        let text = format!("{why} {} will offer it again later.", app_name(app));
        tell(app, "The update will wait", &text).await;
        return;
    }
    let progress = |phase, done, total| {
        let _ = app.emit_to("main", "update-progress", Progress { phase, done, total });
    };
    // Downloaded first, so the helper stops only for the swap itself. Progress is sent at most
    // every half percent (or 256 KB, size unknown), not for each chunk.
    let (mut done, mut sent) = (0u64, 0u64);
    progress("downloading", 0, None);
    let got = update.download(
        |chunk, total| {
            done += chunk as u64;
            let step = total.map_or(256 * 1024, |t| t / 200).max(1);
            if done - sent >= step || total == Some(done) {
                sent = done;
                progress("downloading", done, total);
            }
        },
        || {},
    );
    let bytes = match got.await {
        Ok(b) => b,
        Err(e) => return tell(app, "The update didn't download", &e.to_string()).await,
    };
    progress("installing", bytes.len() as u64, Some(bytes.len() as u64));
    #[cfg(windows)]
    let handle = app.clone();
    let installed = tauri::async_runtime::spawn_blocking(move || {
        let helper = crate::recorder::stop_for_update();
        // Windows' installer closes the app as it starts, so nothing after `install` runs there:
        // what quitting does (the minute in hand saved, Ask's CLIs ended) is done first.
        #[cfg(windows)]
        crate::before_quit(&handle);
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
        Ok(()) => {
            progress("restarting", 0, None);
            app.request_restart()
        }
        Err(e) => tell(app, "The update didn't install", &e).await,
    }
}

/// A message in the window's dialog, or a native alert when there is no window.
async fn tell(app: &AppHandle, title: &str, text: &str) {
    if show_window(app) {
        let message = serde_json::json!({ "title": title, "text": text });
        let _ = app.emit_to("main", "update-message", message);
        return;
    }
    let dialog = app
        .dialog()
        .message(text)
        .title(title)
        .kind(MessageDialogKind::Info);
    let _ = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show()).await;
}
