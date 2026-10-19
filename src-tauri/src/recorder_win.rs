// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! The background helper on Windows, as recorder.rs's launch agent is on macOS: `Tidemark.exe
//! --record` in the user's Run key (login_win.rs), so it starts at each sign-in, and started at
//! once when it is switched on. Windows lists it in Settings › Apps › Startup, where switching it
//! off is the user's to undo.
//!
//! Windows has no signals for a program with no window. The helper makes a named event as it
//! starts and waits on it: the app sets it to stop the helper (switching it off, an update), and
//! the helper saves the minute in hand and ends open incidents before it exits, as it does on
//! SIGTERM on macOS. The event existing is also how the app knows a helper is running.

use crate::login_item as login_win;
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{
    CreateEventW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE, INFINITE,
    SYNCHRONIZATION_ACCESS_RIGHTS,
};

/// The right to wait on the event (SYNCHRONIZE, which the windows crate files under files).
const SYNCHRONIZE: u32 = 0x0010_0000;

/// The Run value's name, beside the app's own "Tidemark" (Open at Login).
const NAME: &str = "Tidemark Helper";
/// Set to stop the helper; there while one runs. Local\ is this sign-in's session.
const STOP: &str = r"Local\com.wayned.tidemark.helper.stop";

pub fn log_path() -> std::path::PathBuf {
    crate::cache::dir().join("helper.log")
}

/// What the helper reports (eprintln) goes to the log, for a program Windows gives no console.
/// Rust's stderr asks for the handle at each write, so setting it here is enough.
pub fn log_to(path: &std::path::Path) {
    use std::os::windows::io::IntoRawHandle;
    use windows::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE};
    let _ = std::fs::create_dir_all(crate::cache::dir());
    if let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let h = HANDLE(f.into_raw_handle());
        let _ = unsafe { SetStdHandle(STD_ERROR_HANDLE, h) };
    }
}

/// Makes the stop event and waits on it on a thread of its own: once set, the minute in hand is
/// saved and open incidents end, and the helper exits.
pub fn on_stop_signal() {
    let Ok(ev) = (unsafe { CreateEventW(None, true, false, &HSTRING::from(STOP)) }) else {
        return;
    };
    let ev = ev.0 as isize;
    let _ = std::thread::Builder::new()
        .name("stop".into())
        .spawn(move || {
            unsafe { WaitForSingleObject(HANDLE(ev as *mut _), INFINITE) };
            crate::monitor::stop();
            std::process::exit(0);
        });
}

fn event(access: u32) -> Option<HANDLE> {
    unsafe {
        OpenEventW(
            SYNCHRONIZATION_ACCESS_RIGHTS(access),
            false,
            &HSTRING::from(STOP),
        )
    }
    .ok()
}

/// Only an installed app: the Run key points at a binary that stays where it is. Tauri's
/// installer puts its uninstaller beside the app; a build in target\ has none.
pub fn available() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| Some(p.parent()?.join("uninstall.exe")))
        .is_some_and(|u| u.is_file())
}

/// The binary the Run key starts; None when the helper isn't on.
pub fn agent_program() -> Option<String> {
    login_win::program(&login_win::get(NAME)?)
}

/// A helper is running in this session.
pub fn loaded() -> bool {
    event(SYNCHRONIZE).is_some_and(|h| {
        let _ = unsafe { CloseHandle(h) };
        true
    })
}

/// Switched off in Windows' Startup apps.
pub fn disabled() -> bool {
    login_win::switched_off(NAME)
}

/// Puts the helper in the Run key and starts it now, stopping one running an older build first.
pub fn install() -> bool {
    let Some(cmd) = login_win::command(crate::recorder::ARG) else {
        return false;
    };
    login_win::set_value(NAME, Some(&cmd));
    unload();
    if disabled() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    // Detached and in its own process group, so it outlives the app and Ctrl+C in a terminal
    // that started the app doesn't reach it.
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    use std::os::windows::process::CommandExt;
    let started = std::process::Command::new(exe)
        .arg(crate::recorder::ARG)
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok();
    // It is running once its event is there.
    started
        && (0..50).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            loaded()
        })
}

pub fn uninstall() {
    login_win::set_value(NAME, None);
    unload();
}

/// Stops a running helper and waits (up to 10 s) for it to have saved and gone.
pub fn unload() {
    let Some(h) = event(EVENT_MODIFY_STATE.0 | SYNCHRONIZE) else {
        return;
    };
    unsafe {
        let _ = SetEvent(h);
        let _ = CloseHandle(h);
    }
    for _ in 0..100 {
        if !loaded() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
