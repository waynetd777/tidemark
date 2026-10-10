// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! The menu bar: an icon (with a mark when something needs attention) that opens a small
//! window under it, the "tray" window (src/Tray.tsx), rather than a native menu. With the main
//! window closed the app leaves the Dock and lives here.

use crate::monitor;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_window_state::AppHandleExt;

/// The popover's width in points; Tray.tsx sizes its height to what it shows.
const WIDTH: f64 = 360.0;

const ICON: &[u8] = include_bytes!("../icons/tray@2x.png");
const ICON_ALERT: &[u8] = include_bytes!("../icons/tray-alert@2x.png");

#[cfg(target_os = "macos")]
pub fn set_in_dock(app: &AppHandle, shown: bool) {
    let _ = app.set_activation_policy(if shown {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    });
}

#[cfg(not(target_os = "macos"))]
pub fn set_in_dock(_app: &AppHandle, _shown: bool) {}

/// Brings the app to the front: after coming back from Accessory, `set_focus()` alone can leave
/// the window behind whatever the user was working in.
#[cfg(target_os = "macos")]
fn activate() {
    use objc2::runtime::{AnyClass, AnyObject};
    let Some(cls) = AnyClass::get(c"NSApplication") else {
        return;
    };
    unsafe {
        let nsapp: *mut AnyObject = objc2::msg_send![cls, sharedApplication];
        if !nsapp.is_null() {
            let _: () = objc2::msg_send![nsapp, activateIgnoringOtherApps: true];
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn activate() {}

pub fn show_main(app: &AppHandle) {
    // Back in the Dock before the window is shown: done afterwards, the window can come up
    // behind whatever had focus.
    set_in_dock(app, true);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        activate();
        let _ = w.set_focus();
    }
}

/// When the popover last closed itself on losing focus.
static BLURRED_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// A click on the icon this soon after the popover closed on losing focus is the click that took
/// the focus: it closes the popover rather than opening it again.
const CLICK_AFTER_BLUR: Duration = Duration::from_millis(400);

/// Where the icon was that the popover last opened under. With two screens each menu bar has
/// the icon, and a click on the other screen's takes the focus too: that one opens it there.
static OPENED_UNDER: Mutex<Option<(f64, f64)>> = Mutex::new(None);

fn icon_place(rect: &tauri::Rect) -> (f64, f64) {
    let p = rect.position.to_physical::<f64>(1.0);
    (p.x, p.y)
}

fn toggle(app: &AppHandle, rect: tauri::Rect) {
    let Some(w) = app.get_webview_window("tray") else {
        return;
    };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    let same_icon = *OPENED_UNDER.lock().unwrap() == Some(icon_place(&rect));
    if BLURRED_AT
        .lock()
        .unwrap()
        .take()
        .is_some_and(|t| t.elapsed() < CLICK_AFTER_BLUR)
        && same_icon
    {
        return;
    }
    *OPENED_UNDER.lock().unwrap() = Some(icon_place(&rect));
    // Placed in points: on macOS they are one space across screens, where physical pixels are
    // not. The click gives the icon in the pixels of its screen; that screen is the one where,
    // in its own points, the icon falls inside it (within an icon's height above its top edge,
    // where macOS can report it).
    let monitors = app.available_monitors().unwrap_or_default();
    let icon = rect.position.to_physical::<f64>(1.0);
    let icon_size = rect.size.to_physical::<f64>(1.0);
    let in_points = |m: &tauri::Monitor| {
        let k = m.scale_factor();
        let (p, sz) = (m.position(), m.size());
        let (left, top) = (p.x as f64 / k, p.y as f64 / k);
        let (x, y, slack) = (icon.x / k, icon.y / k, icon_size.height / k);
        let inside = x >= left
            && x < left + sz.width as f64 / k
            && y >= top - slack
            && y < top + sz.height as f64 / k;
        inside.then_some((k, left, left + sz.width as f64 / k))
    };
    let screen = monitors.iter().find_map(in_points);
    let k = screen.map_or_else(|| w.scale_factor().unwrap_or(2.0), |s| s.0);
    let (x, y, iw, ih) = (
        icon.x / k,
        icon.y / k,
        icon_size.width / k,
        icon_size.height / k,
    );
    let mut left = x + iw / 2.0 - WIDTH / 2.0;
    if let Some((_, screen_left, screen_right)) = screen {
        left = left.min(screen_right - WIDTH - 8.0).max(screen_left + 8.0);
    }
    let place = tauri::LogicalPosition::new(left, y + ih + 6.0);
    let _ = w.set_position(place);
    let _ = w.show();
    // Once on that screen, place it again, in case macOS moved it while showing it.
    let _ = w.set_position(place);
    let _ = w.set_focus();
    let _ = w.emit("tray-opened", ());
}

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    if crate::scene::active() {
        return Ok(());
    }
    let icon = tauri::image::Image::from_bytes(ICON).expect("tray icon is a valid png");
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Tidemark")
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = ev
            {
                toggle(tray.app_handle(), rect);
            }
        })
        .build(app)?;

    if let Some(t) = app.get_webview_window("tray") {
        let t2 = t.clone();
        t.on_window_event(move |ev| {
            if let tauri::WindowEvent::Focused(false) = ev {
                if t2.is_visible().unwrap_or(false) {
                    *BLURRED_AT.lock().unwrap() = Some(Instant::now());
                }
                let _ = t2.hide();
            }
        });
    }

    // The icon follows the status; the recorder samples faster while a window is in front.
    let h = app.handle().clone();
    std::thread::spawn(move || {
        let mut alert = false;
        loop {
            // Fast sampling only while someone is looking: the window in front, or the
            // menu-bar window open.
            let seen = h
                .get_webview_window("main")
                .is_some_and(|w| w.is_focused().unwrap_or(false))
                || h.get_webview_window("tray")
                    .is_some_and(|w| w.is_visible().unwrap_or(false));
            monitor::FAST.store(seen, Ordering::Relaxed);
            let now = monitor::status().level == "attention";
            if now != alert {
                alert = now;
                if let Some(t) = h.tray_by_id("main") {
                    if let Ok(img) =
                        tauri::image::Image::from_bytes(if alert { ICON_ALERT } else { ICON })
                    {
                        let _ = t.set_icon(Some(img));
                        let _ = t.set_icon_as_template(true);
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
    Ok(())
}

/// Opens the main window, on a screen ("now", "disk", "performance", "trends", "settings") if
/// given, and closes the popover.
#[tauri::command]
pub fn show_main_window(app: AppHandle, screen: Option<String>, focus: Option<String>) {
    show_main(&app);
    if let Some(s) = screen {
        // The screen, and the section to bring into view on it (as a Details link does).
        let _ = app.emit(
            "navigate",
            serde_json::json!({ "screen": s, "focus": focus }),
        );
    }
    if let Some(t) = app.get_webview_window("tray") {
        let _ = t.hide();
    }
}

/// Opens the main window with Ask open.
#[tauri::command]
pub fn open_ask(app: AppHandle) {
    show_main_window(app.clone(), None, None);
    let _ = app.emit("open-ask", ());
}

/// Off the main thread (async), so waiting for `open` holds nothing up.
#[tauri::command(async)]
pub fn open_activity_monitor() {
    let _ = std::process::Command::new("/usr/bin/open")
        .args(["-a", "Activity Monitor"])
        .status();
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    let _ = app.save_window_state(crate::STATE_FLAGS);
    app.exit(0)
}

/// Open at Login: (available, on). Only the installed app can register itself.
#[tauri::command]
pub fn login_item() -> (bool, bool) {
    // Screenshots run the dev build, which can't register; show it as the installed app would.
    if crate::scene::active() {
        return (true, true);
    }
    #[cfg(target_os = "macos")]
    {
        (
            crate::login_item::available(),
            crate::login_item::status().is_on(),
        )
    }
    #[cfg(not(target_os = "macos"))]
    (false, false)
}

#[tauri::command]
pub fn set_login_item(on: bool) -> bool {
    #[cfg(target_os = "macos")]
    {
        use crate::login_item::Status;
        let s = crate::login_item::set(on);
        if s == Status::RequiresApproval {
            crate::login_item::open_settings();
        }
        s.is_on()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = on;
        false
    }
}
