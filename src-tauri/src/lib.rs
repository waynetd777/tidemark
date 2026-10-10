// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

//! Puts the app together: the disk side (disk.rs, scan.rs), the recorder (monitor.rs), the
//! history and its schedule (history.rs, schedule.rs, trends.rs), the menu bar (tray.rs) and Ask.

mod assistant;
mod cache;
mod crashes;
mod disk;
mod energy;
mod files;
mod history;
#[cfg(target_os = "macos")]
mod login_item;
#[cfg(target_os = "macos")]
mod login_launch;
mod monitor;
mod recorder;
mod scan;
mod schedule;
mod settings;
mod system;
mod tray;
mod trends;
mod volume;

use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{Manager, State};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

/// Everything except VISIBLE. The window is deliberately created hidden and shown once it
/// carries the theme colour, so restoring a saved "visible" would put it back on screen too
/// early and reintroduce the white flash.
pub(crate) const STATE_FLAGS: StateFlags = StateFlags::all().difference(StateFlags::VISIBLE);

/// Startup timing, printed when TIDEMARK_TIMING is set. Run the binary inside the bundle to see it:
///   TIDEMARK_TIMING=1 "/Applications/Tidemark.app/Contents/MacOS/Tidemark"
static START: Mutex<Option<Instant>> = Mutex::new(None);

pub fn mark_start() {
    if let Ok(mut g) = START.lock() {
        *g = Some(Instant::now());
    }
}

pub fn stamp(what: &str) {
    if std::env::var_os("TIDEMARK_TIMING").is_none() {
        return;
    }
    let ms = START
        .lock()
        .ok()
        .and_then(|g| *g)
        .map(|t| t.elapsed().as_millis())
        .unwrap_or(0);
    eprintln!("[timing] {ms:>5} ms  {what}");
}

/// The app's website, as the Help menu opens it.
const WEBSITE: &str = "https://tidemark.davies.co.za";

/// The standard menu bar, with Tidemark's help and website in its Help menu.
fn help_menu(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, HELP_SUBMENU_ID};
    use tauri::Emitter;
    let menu = Menu::default(app.handle())?;
    if let Some(help) = menu
        .get(HELP_SUBMENU_ID)
        .and_then(|m| m.as_submenu().cloned())
    {
        help.append(&MenuItem::with_id(
            app,
            "help",
            "Tidemark Help",
            true,
            Some("CmdOrCtrl+Shift+/"),
        )?)?;
        help.append(&MenuItem::with_id(
            app,
            "website",
            "Tidemark Website",
            true,
            None::<&str>,
        )?)?;
    }
    app.set_menu(menu)?;
    app.on_menu_event(|app, ev| match ev.id.as_ref() {
        "help" => {
            tray::show_main(app);
            let _ = app.emit("open-help", ());
        }
        "website" => {
            let _ = std::process::Command::new("/usr/bin/open")
                .arg(WEBSITE)
                .status();
        }
        _ => {}
    });
    Ok(())
}

/// `Tidemark --record`: the background helper (recorder.rs) rather than the app. Some(never
/// returns) when the arguments ask for it.
pub fn helper(args: &[String]) -> Option<std::convert::Infallible> {
    if args.iter().any(|a| a == recorder::ARG) {
        recorder::run();
    }
    None
}

pub fn run() {
    let root = cache::load_root();
    tauri::Builder::default()
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(STATE_FLAGS)
                .with_denylist(&["tray"])
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(Arc::new(assistant::Running::default()))
        .manage(disk::AppState::new(root.clone()))
        .invoke_handler(tauri::generate_handler![
            disk::get_state,
            disk::get_progress,
            disk::get_tree,
            disk::start_scan,
            disk::stop_scan,
            disk::reveal,
            disk::get_info,
            disk::open_privacy_settings,
            files::get_files,
            assistant::assistant_status,
            assistant::ask,
            assistant::ask_cancel,
            monitor::perf_live,
            monitor::perf_history,
            monitor::perf_apps,
            monitor::incidents,
            monitor::sleeps,
            crashes::crashes,
            monitor::pause_recording,
            monitor::mac_info,
            monitor::perf_brief,
            trends::disk_trend,
            trends::disk_growth,
            trends::folder_history,
            trends::cpu_by_hour,
            trends::history_info,
            settings::get_settings,
            settings::save_settings,
            tray::show_main_window,
            tray::open_ask,
            tray::quit,
            tray::open_activity_monitor,
            tray::login_item,
            tray::set_login_item,
            recorder::background_helper,
            recorder::set_background_helper,
        ])
        .setup(move |app| {
            stamp("setup() start");
            let handle = app.handle().clone();

            // Did Login Items start this, rather than someone opening the app? Asked first: the
            // answer is in the launch AppleEvent AppKit is dispatching now. A login launch stays
            // in the menu bar.
            #[cfg(target_os = "macos")]
            let quiet = login_launch::probe();
            #[cfg(not(target_os = "macos"))]
            let quiet = false;
            if quiet {
                tray::set_in_dock(&handle, false);
            }

            // Read the cached scan off the startup path, then start the rescan behind it. The
            // UI polls get_state until `loading` clears, draws the cache, and follows progress.
            {
                let h = handle.clone();
                let root = root.clone();
                std::thread::spawn(move || {
                    let st: State<disk::AppState> = h.state();
                    let cached = cache::load_scan(&root).map(Arc::new);
                    // A scan under an hour old is shown as it is: rescanning the whole disk on
                    // every launch costs a minute of every core for next to nothing new.
                    let fresh = cached.as_ref().is_some_and(|c| {
                        chrono::DateTime::parse_from_rfc3339(&c.finished).is_ok_and(|t| {
                            chrono::Local::now().signed_duration_since(t).num_minutes() < 60
                        })
                    });
                    if let Ok(mut s) = st.scan.lock() {
                        *s = cached;
                    }
                    st.loading.store(false, std::sync::atomic::Ordering::SeqCst);
                    stamp("cache loaded");
                    if !fresh {
                        let _ = disk::start_scan_thread(&h, root, true);
                    }
                });
            }

            monitor::start();
            schedule::start(handle.clone());
            tray::setup(app)?;
            help_menu(app)?;

            if let Some(w) = app.get_webview_window("main") {
                // Paint the native window in the theme's background colour before it is shown,
                // so no frame of the default white can appear behind the webview. Keep these two
                // colours in step with --bg in src/styles.css and index.html.
                // Settings' appearance, if it is not "match the Mac", for the native window too.
                let dark = match settings::get().theme {
                    settings::Theme::Light => {
                        let _ = w.set_theme(Some(tauri::Theme::Light));
                        false
                    }
                    settings::Theme::Dark => {
                        let _ = w.set_theme(Some(tauri::Theme::Dark));
                        true
                    }
                    settings::Theme::System => matches!(w.theme(), Ok(tauri::Theme::Dark)),
                };
                let bg = if dark {
                    tauri::window::Color(30, 30, 32, 255)
                } else {
                    tauri::window::Color(245, 245, 247, 255)
                };
                let _ = w.set_background_color(Some(bg));

                // The WKWebView is a separate surface and Tauri's set_background_color is a no-op
                // for it on macOS. WebKit paints the empty document white on its first composite,
                // before index.html's inline <style> is parsed, so turn the webview's own
                // background off (drawsBackground = NO via KVC) and let the window colour show
                // until the page paints. Show from INSIDE the with_webview closure: it is queued
                // onto the event loop, and a show() here would race it on a cold start.
                #[cfg(target_os = "macos")]
                {
                    let (r, g, b) = if dark {
                        (30.0, 30.0, 32.0)
                    } else {
                        (245.0, 245.0, 247.0)
                    };
                    let w_show = w.clone();
                    let _ = w.with_webview(move |wv| {
                        unsafe {
                            use objc2::runtime::AnyObject;
                            let webview: *mut AnyObject = wv.inner() as *mut AnyObject;
                            let color = objc2_app_kit::NSColor::colorWithSRGBRed_green_blue_alpha(
                                r / 255.0,
                                g / 255.0,
                                b / 255.0,
                                1.0,
                            );
                            let _: () =
                                objc2::msg_send![webview, setUnderPageBackgroundColor: &*color];
                            let no = objc2_foundation::NSNumber::new_bool(false);
                            let key = objc2_foundation::NSString::from_str("drawsBackground");
                            let _: () = objc2::msg_send![webview, setValue: &*no, forKey: &*key];
                            let _: () = objc2::msg_send![webview, setWantsLayer: true];
                            let layer: *mut AnyObject = objc2::msg_send![webview, layer];
                            if !layer.is_null() {
                                let cg: *mut AnyObject = objc2::msg_send![&*color, CGColor];
                                let _: () = objc2::msg_send![layer, setBackgroundColor: cg];
                            }
                        }
                        stamp("webview background set");
                        if !quiet {
                            let _ = w_show.show();
                            let _ = w_show.set_focus();
                        }
                        stamp("window shown");
                    });
                    // If that message is never delivered, the window would stay hidden for good.
                    let w_fallback = w.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1200));
                        if !quiet && matches!(w_fallback.is_visible(), Ok(false)) {
                            stamp("fallback show");
                            let _ = w_fallback.show();
                            let _ = w_fallback.set_focus();
                        }
                    });
                }
                #[cfg(not(target_os = "macos"))]
                if !quiet {
                    let _ = w.show();
                    let _ = w.set_focus();
                }

                // Closing the window hides it; the app leaves the Dock and stays in the menu bar,
                // recording.
                let w2 = w.clone();
                w.on_window_event(move |ev| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                        api.prevent_close();
                        let _ = w2.app_handle().save_window_state(STATE_FLAGS);
                        let _ = w2.hide();
                        tray::set_in_dock(w2.app_handle(), false);
                    }
                });
            }
            stamp("setup() done");
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, ev| {
            // No CLI answering a question outlives the app.
            if let tauri::RunEvent::Exit = ev {
                app.state::<Arc<assistant::Running>>().kill_all();
            }
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = ev {
                tray::show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, ev);
        });
}
