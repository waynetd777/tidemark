// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

fn main() {
    // SMAppService (src/login_item.rs) lives in ServiceManagement, and the idle time the disk
    // schedule waits for (src/schedule.rs) in CoreGraphics; without these the class and the
    // symbol are missing at run time.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=framework=ServiceManagement");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
    }
    tauri_build::build()
}
