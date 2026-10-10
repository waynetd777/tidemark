// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

//! Was this process started by the Login Items list, or by someone opening the app?
//!
//! macOS says so in the launch AppleEvent: the `kAEOpenApplication` ('oapp') event that starts
//! every app carries `keyAEPropData` = `keyAELaunchedAsLogInItem` ('lgit') when the launch came
//! from Login Items. Nothing else distinguishes the two — both are launched by launchd, with the
//! same arguments and the same environment — and `SMAppService.mainApp`, which is what this app
//! registers (see login_item.rs), cannot pass arguments of its own.
//!
//! `currentAppleEvent` is only the launch event while AppKit is dispatching it, so this must be
//! asked EARLY. Tauri's `setup` hook is inside that window - verified 2026-09-13 in another app of this author's, the
//! same pattern on the same Tauri, by launching the installed bundle through `NSWorkspaceOpenConfiguration.appleEvent` carrying the same flag, and
//! reading true at "setup start" against false for a plain `open -a`. That it is readable at the
//! top of `setup` is what makes this usable at all: the decision lands before anything shows a
//! window, so a login launch never puts one on screen to take away again.

#![cfg(target_os = "macos")]

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
/// Four-char codes, as the Apple Event Manager spells them.
const K_AE_OPEN_APPLICATION: u32 = u32::from_be_bytes(*b"oapp");
const KEY_AE_PROP_DATA: u32 = u32::from_be_bytes(*b"prdt");
const KEY_AE_LAUNCHED_AS_LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit");

/// True when Login Items started this process. Must be called while AppKit is still dispatching
/// the launch event - Tauri's `setup` hook is inside that window, and verified to be (see the
/// module comment). Anything missing or unexpected reads as false, so the app opens its window
/// as it always did rather than starting invisibly for a reason nobody can see.
pub fn probe() -> bool {
    let Some(cls) = AnyClass::get(c"NSAppleEventManager") else {
        return false;
    };
    let mgr: *mut AnyObject = unsafe { msg_send![cls, sharedAppleEventManager] };
    if mgr.is_null() {
        return false;
    }
    let ev: *mut AnyObject = unsafe { msg_send![mgr, currentAppleEvent] };
    if ev.is_null() {
        return false;
    }
    let id: u32 = unsafe { msg_send![ev, eventID] };
    if id != K_AE_OPEN_APPLICATION {
        return false;
    }
    let prop: *mut AnyObject =
        unsafe { msg_send![ev, paramDescriptorForKeyword: KEY_AE_PROP_DATA] };
    if prop.is_null() {
        return false;
    }
    let code: u32 = unsafe { msg_send![prop, enumCodeValue] };
    code == KEY_AE_LAUNCHED_AS_LOGIN_ITEM
}
