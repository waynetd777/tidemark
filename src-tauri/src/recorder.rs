// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! The background helper: the app's own binary run as `Tidemark --record` by a launch agent, so
//! the record goes on while the app is quit. It has no window and no Dock icon; it records
//! (monitor.rs) and keeps the disk's schedule (schedule.rs) whenever it holds the recorder lock,
//! which it does unless the app took it first, and then takes over when the app quits.
//!
//! The agent's plist ships inside the bundle (Contents/Library/LaunchAgents, from
//! launchagent/ via tauri.conf.json) and is registered through SMAppService, like Open at Login,
//! so System Settings' Login Items is the one place it is listed and can be switched off.
//!
//! macOS ties the registration to the build that made it: with the self-signed identity (no
//! team ID) it pins the binary's code hash, so after a rebuild or an update launchd refuses to
//! start the helper (exit 78, EX_CONFIG) and retries forever. The installed app re-registers it
//! at launch when the build has changed since it was last registered (`renew_if_rebuilt`).

use serde::Serialize;

/// The plist's file name in Contents/Library/LaunchAgents.
pub const PLIST: &str = "com.wayned.tidemark.recorder.plist";

/// The argument the launch agent passes.
pub const ARG: &str = "--record";

/// Runs the helper until it is stopped. Never returns.
pub fn run() -> ! {
    // No window of its own: slow sampling, unless the app says one is on screen.
    crate::monitor::FAST.store(false, std::sync::atomic::Ordering::Relaxed);
    crate::monitor::start();
    crate::schedule::start_headless();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

#[derive(Serialize)]
pub struct Helper {
    /// Only the installed app can register it.
    pub available: bool,
    pub on: bool,
    /// Registered, but switched off in System Settings: it needs allowing there.
    pub needs_approval: bool,
}

fn helper() -> Helper {
    if crate::scene::active() {
        return Helper {
            available: true,
            on: true,
            needs_approval: false,
        };
    }
    #[cfg(target_os = "macos")]
    {
        use crate::login_item::{self, Status};
        let s = login_item::agent_status();
        Helper {
            available: login_item::available(),
            on: s == Status::Enabled,
            needs_approval: s == Status::RequiresApproval,
        }
    }
    #[cfg(not(target_os = "macos"))]
    Helper {
        available: false,
        on: false,
        needs_approval: false,
    }
}

/// This build of the app: its version, and the binary's size and time, which every rebuild
/// changes.
fn build_id() -> Option<String> {
    let m = std::fs::metadata(std::env::current_exe().ok()?).ok()?;
    let t = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(format!("{} {} {t}", env!("CARGO_PKG_VERSION"), m.len()))
}

/// Where the build the helper was last registered for is kept.
fn build_file() -> std::path::PathBuf {
    crate::cache::dir().join("recorder.build")
}

fn remember_build() {
    if let Some(b) = build_id() {
        let _ = std::fs::create_dir_all(crate::cache::dir());
        if let Err(e) = std::fs::write(build_file(), b) {
            // Not kept: the next launch registers the helper again, which is harmless.
            eprintln!("recorder: keeping the build registered: {e}");
        }
    }
}

/// Registers the helper afresh when it is on but was registered by another build, whose
/// registration macOS would refuse to start. Run once at the installed app's launch.
pub fn renew_if_rebuilt() {
    #[cfg(target_os = "macos")]
    {
        use crate::login_item::{self, Status};
        if crate::scene::active() || !login_item::available() {
            return;
        }
        if login_item::agent_status() != Status::Enabled {
            return;
        }
        let Some(build) = build_id() else {
            return;
        };
        if std::fs::read_to_string(build_file()).ok().as_deref() == Some(build.as_str()) {
            return;
        }
        // SMAppService hands back autoreleased objects, and this thread has no pool of its own.
        objc2::rc::autoreleasepool(|_| {
            login_item::agent_set(false);
            let mut s = login_item::agent_set(true);
            if s == Status::NotRegistered {
                // The unregister may still be going through launchd: once more, a moment later.
                std::thread::sleep(std::time::Duration::from_secs(1));
                s = login_item::agent_set(true);
            }
            // Kept only when it took, so a failed attempt is tried again at the next launch;
            // needing approval shows in Settings, where the switch says so.
            if s == Status::Enabled {
                remember_build();
            } else {
                eprintln!("recorder: registering the helper afresh left it {s:?}");
            }
        });
    }
}

#[tauri::command]
pub fn background_helper() -> Helper {
    helper()
}

/// Turns the helper on or off. Asking for approval opens System Settings at Login Items.
#[tauri::command]
pub fn set_background_helper(on: bool) -> Helper {
    #[cfg(target_os = "macos")]
    {
        use crate::login_item::{self, Status};
        let s = login_item::agent_set(on);
        if s == Status::RequiresApproval {
            login_item::open_settings();
        }
        if on && s != Status::NotFound {
            remember_build();
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = on;
    helper()
}
