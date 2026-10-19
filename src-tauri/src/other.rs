// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Stand-ins, off macOS, for the parts only macOS has, with the same names, so lib.rs and the
//! page call the same functions everywhere.

pub mod login_launch {
    /// Started by the Run key (Open at Login), which passes --login: it stays in the
    /// notification area.
    pub fn probe() -> bool {
        std::env::args().any(|a| a == crate::login_item::ARG)
    }
}
