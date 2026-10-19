// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// No console window beside the app on Windows, in a debug build too (and none for the background
// helper, which is the same binary).
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let _ = tidemark_lib::helper(&args);
    tidemark_lib::mark_start();
    tidemark_lib::run()
}
