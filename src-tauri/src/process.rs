// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Child processes. On Windows a console program started by an app with no console of its own
//! opens a console window for itself, flashing up beside the app, unless it is told not to.

use std::process::Command;

/// On Windows, runs a console program without opening a console window for it; elsewhere,
/// nothing. Every Command the app runs goes through this, short ones (where, taskkill) too.
pub fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(cmd, 0x0800_0000); // CREATE_NO_WINDOW
    cmd
}

/// Opens a file, folder or web address in what the system opens it with (`open`, or Explorer's
/// shell on Windows), without waiting for it.
pub fn open(target: &str) {
    #[cfg(target_os = "macos")]
    let child = Command::new("/usr/bin/open").arg(target).spawn();
    // explorer.exe opens a folder or a web address in the default app; it never answers with a
    // useful exit code, so nothing is waited for.
    #[cfg(windows)]
    let child = no_window(&mut Command::new("explorer.exe"))
        .arg(target)
        .spawn();
    if let Ok(mut child) = child {
        // Reaped off the caller's thread, so no zombie is left behind.
        std::thread::spawn(move || child.wait());
    }
}
