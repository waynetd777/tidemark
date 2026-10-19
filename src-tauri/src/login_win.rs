// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! "Open at Login" on Windows, as login_item.rs is on macOS: the app in the user's Run key
//! (HKCU\Software\Microsoft\Windows\CurrentVersion\Run), started with --login so it stays in the
//! notification area (login_launch::probe). The background helper starts from the same key
//! (recorder_win.rs).
//!
//! Windows' Settings › Apps › Startup (and Task Manager's Startup apps) can switch a Run entry
//! off without removing it: it keeps that in Explorer's StartupApproved\Run, a binary value per
//! name whose first byte is odd when it is off. That is macOS's "approve it in System Settings":
//! only the user can switch it back on, there.

use windows::core::HSTRING;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
    RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const NAME: &str = "Tidemark";
/// What a launch from the Run key is started with.
pub const ARG: &str = "--login";

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Status {
    Enabled,
    NotRegistered,
    /// In the Run key, but switched off in Windows' Startup apps.
    RequiresApproval,
}

impl Status {
    pub fn is_on(self) -> bool {
        self == Status::Enabled
    }
}

/// The command a Run value holds, or None when there is none.
pub fn get(name: &str) -> Option<String> {
    let mut buf = [0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN),
            &HSTRING::from(name),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    read.is_ok()
        .then(|| String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)]))
}

/// Writes a Run value; None removes it.
pub fn set_value(name: &str, command: Option<&str>) {
    let (run, name) = (HSTRING::from(RUN), HSTRING::from(name));
    unsafe {
        match command {
            Some(cmd) => {
                let wide: Vec<u16> = cmd.encode_utf16().chain([0]).collect();
                let _ = RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    &run,
                    &name,
                    REG_SZ.0,
                    Some(wide.as_ptr().cast()),
                    (wide.len() * 2) as u32,
                );
            }
            None => {
                let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &run, &name);
            }
        }
    }
}

/// Switched off in Windows' Startup apps.
pub fn switched_off(name: &str) -> bool {
    let mut buf = [0u8; 12];
    let mut size = buf.len() as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(APPROVED),
            &HSTRING::from(name),
            RRF_RT_REG_BINARY,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    read.is_ok() && size > 0 && buf[0] & 1 == 1
}

/// This copy of the app, quoted, with its arguments.
pub fn command(arg: &str) -> Option<String> {
    Some(format!(
        "\"{}\" {arg}",
        std::env::current_exe().ok()?.display()
    ))
}

/// The program a Run command starts: the quoted path at its start.
pub fn program(command: &str) -> Option<String> {
    let rest = command.strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

pub fn available() -> bool {
    command(ARG).is_some()
}

/// Enabled when the Run key starts this copy of the app and Startup apps hasn't switched it off.
pub fn status() -> Status {
    match get(NAME) {
        Some(held) if command(ARG).is_some_and(|c| c.eq_ignore_ascii_case(&held)) => {
            if switched_off(NAME) {
                Status::RequiresApproval
            } else {
                Status::Enabled
            }
        }
        _ => Status::NotRegistered,
    }
}

pub fn set(on: bool) -> Status {
    let cmd = command(ARG);
    set_value(NAME, if on { cmd.as_deref() } else { None });
    status()
}

/// Windows' Startup apps, where an entry switched off there is switched back on.
pub fn open_settings() {
    crate::process::open("ms-settings:startupapps");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_program_from_a_command() {
        assert_eq!(
            program(r#""C:\Users\x\AppData\Local\Tidemark\tidemark.exe" --record"#).as_deref(),
            Some(r"C:\Users\x\AppData\Local\Tidemark\tidemark.exe")
        );
        assert_eq!(program("tidemark.exe"), None);
    }
}
