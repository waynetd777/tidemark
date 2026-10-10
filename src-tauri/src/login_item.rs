// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! "Open at Login", through SMAppService.
//!
//! SMAppService is the same mechanism System Settings > General > Login Items drives, so a
//! toggle here and the switch there are two views of one setting: whatever was set in System
//! Settings is what `status()` reports, and there is no second LaunchAgent to fall out of step
//! with it. (A LaunchAgent plist, which the usual autostart crates write, WOULD be a second
//! source of truth — the app would keep launching from a plist the Login Items list never
//! mentions.)
//!
//! It needs a stable, signed bundle identifier, which this app has: com.wayned.tidemark,
//! signed with the identity in signing.local (see `make install-app`). Registration is per-user
//! and survives a rebuild as long as the identifier does.
//!
//! Nothing here works for the unbundled `make dev` binary: SMAppService registers a BUNDLE, so
//! the switch is left disabled there (see `available`).

#![cfg(target_os = "macos")]

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};

/// SMAppServiceStatus, as documented. Anything unrecognised is treated as NotFound.
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Status {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
}

impl Status {
    /// Whether the menu item should show a tick. RequiresApproval means macOS has the
    /// registration but the user has switched it off in System Settings, so it is NOT on.
    pub fn is_on(self) -> bool {
        self == Status::Enabled
    }
}

fn service() -> Option<*mut AnyObject> {
    // The class is missing rather than the call failing if ServiceManagement did not link.
    let cls = AnyClass::get(c"SMAppService")?;
    let svc: *mut AnyObject = unsafe { msg_send![cls, mainAppService] };
    (!svc.is_null()).then_some(svc)
}

/// The background helper's launch agent: the plist in the bundle's
/// Contents/Library/LaunchAgents (see recorder.rs). It shows in System Settings > General >
/// Login Items under "Allow in the Background".
fn agent() -> Option<*mut AnyObject> {
    let cls = AnyClass::get(c"SMAppService")?;
    let name = objc2_foundation::NSString::from_str(crate::recorder::PLIST);
    let svc: *mut AnyObject = unsafe { msg_send![cls, agentServiceWithPlistName: &*name] };
    (!svc.is_null()).then_some(svc)
}

fn status_of(svc: Option<*mut AnyObject>) -> Status {
    let Some(svc) = svc else {
        return Status::NotFound;
    };
    let raw: isize = unsafe { msg_send![svc, status] };
    match raw {
        0 => Status::NotRegistered,
        1 => Status::Enabled,
        2 => Status::RequiresApproval,
        _ => Status::NotFound,
    }
}

fn set_on(svc: Option<*mut AnyObject>, on: bool) {
    if let Some(svc) = svc {
        let mut err: *mut AnyObject = std::ptr::null_mut();
        let _ok: bool = unsafe {
            if on {
                msg_send![svc, registerAndReturnError: &mut err]
            } else {
                msg_send![svc, unregisterAndReturnError: &mut err]
            }
        };
    }
}

pub fn agent_status() -> Status {
    status_of(agent())
}

/// Registers or unregisters the helper; the status afterwards, as for `set`.
pub fn agent_set(on: bool) -> Status {
    set_on(agent(), on);
    agent_status()
}

/// Whether this is a bundled app, which is what SMAppService registers. The menu item is enabled
/// on this rather than on `status()`: macOS reports NotFound for an app bundle that has never been
/// registered, as well as for the unbundled `make dev` binary.
pub fn available() -> bool {
    service().is_some()
        && std::env::current_exe()
            .is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
}

pub fn status() -> Status {
    let Some(svc) = service() else {
        return Status::NotFound;
    };
    let raw: isize = unsafe { msg_send![svc, status] };
    match raw {
        0 => Status::NotRegistered,
        1 => Status::Enabled,
        2 => Status::RequiresApproval,
        _ => Status::NotFound,
    }
}

/// Turn it on or off. Returns the status afterwards, which is what the menu should show: asking
/// again is the only honest answer, because macOS can answer RequiresApproval to a register.
pub fn set(on: bool) -> Status {
    if let Some(svc) = service() {
        let mut err: *mut AnyObject = std::ptr::null_mut();
        let _ok: bool = unsafe {
            if on {
                msg_send![svc, registerAndReturnError: &mut err]
            } else {
                msg_send![svc, unregisterAndReturnError: &mut err]
            }
        };
        // The error is deliberately ignored: every failure mode here (already in that state,
        // unsigned binary, approval needed) shows up in the status we read back anyway.
    }
    status()
}

/// System Settings > General > Login Items, for when macOS says the user has to approve.
pub fn open_settings() {
    let _ = std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.LoginItems-Settings.extension")
        .status();
}
