// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

//! The background helper: the app's own binary run as `Tidemark --record` by a launch agent, so
//! the record goes on while the app is quit. It has no window and no Dock icon; it records
//! (monitor.rs) and keeps the disk's schedule (schedule.rs) whenever it holds the recorder lock,
//! which it does unless the app took it first, and then takes over when the app quits.
//!
//! The agent's plist ships inside the bundle (Contents/Library/LaunchAgents, from
//! launchagent/ via tauri.conf.json) and is registered through SMAppService, like Open at Login,
//! so System Settings' Login Items is the one place it is listed and can be switched off.

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
        if login_item::agent_set(on) == Status::RequiresApproval {
            login_item::open_settings();
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = on;
    helper()
}
