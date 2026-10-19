// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! The volume a path lives on: its name, capacity and free space; and the form paths take in
//! the app.
//!
//! The app writes every path as macOS does, with `/` between the parts, on Windows too: there `/`
//! is the system drive (C:\), so C:\Users\wayne is /Users/wayne, and another drive keeps its
//! letter (D:/Photos). The history, the cached scans and the page then say "/" for the whole
//! system disk on both, and the page's path handling is one. `native` turns a path into what
//! Windows opens, and the page shows each path in Windows' own form (src/util.ts `shown`).

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug, Default)]
pub struct Volume {
    pub name: String,
    pub total: u64,
    pub free: u64,
    pub used: u64,
}

/// The system drive's letter and colon ("C:"), from %SystemDrive%.
#[cfg(windows)]
fn system_drive() -> &'static str {
    static DRIVE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DRIVE.get_or_init(|| {
        std::env::var("SystemDrive")
            .ok()
            .filter(|d| d.len() == 2 && d.ends_with(':'))
            .map(|d| d.to_uppercase())
            .unwrap_or_else(|| "C:".into())
    })
}

/// The system drive ("C:") on Windows, for the page to show paths with; empty elsewhere.
pub fn drive() -> String {
    #[cfg(windows)]
    return system_drive().to_string();
    #[cfg(not(windows))]
    String::new()
}

/// What the file system opens for an app path: on Windows "/" is the system drive's root, and
/// every `/` is a `\`; elsewhere the path as it is.
pub fn native(path: &str) -> PathBuf {
    #[cfg(windows)]
    {
        // A network share (//server/share) is not on the system drive.
        let p = if path.starts_with('/') && !path.starts_with("//") {
            format!("{}{path}", system_drive())
        } else {
            path.to_string()
        };
        // A drive on its own ("D:") is the folder the process is in on that drive; its root is
        // what is meant.
        let p = if p.len() == 2 && p.ends_with(':') {
            format!("{p}/")
        } else {
            p
        };
        PathBuf::from(p.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    PathBuf::from(path)
}

/// The app's form of a path the system gave (a folder picked, the home folder): on Windows `/`
/// between the parts, and the system drive's root as "/"; elsewhere the path as it is.
pub fn canonical(path: &Path) -> String {
    let s = path.to_string_lossy();
    #[cfg(windows)]
    {
        // \\?\C:\… is the same place as C:\….
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s).replace('\\', "/");
        let drive = system_drive();
        let on_system = s.len() >= 2
            && s.is_char_boundary(2)
            && s[..2].eq_ignore_ascii_case(drive)
            && (s.len() == 2 || s[2..].starts_with('/'));
        let s = if on_system {
            s[2..].to_string()
        } else if s.len() >= 2 && s.as_bytes()[1] == b':' {
            format!("{}{}", s[..1].to_uppercase(), &s[1..])
        } else {
            s
        };
        let s = s.trim_end_matches('/');
        if s.is_empty() {
            "/".into()
        } else if s.len() == 2 && s.ends_with(':') {
            format!("{s}/")
        } else {
            s.to_string()
        }
    }
    #[cfg(not(windows))]
    s.into_owned()
}

/// `name` inside the folder `dir`, in the app's form.
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Total and available bytes for the filesystem holding `path` (statfs, or GetDiskFreeSpaceEx).
pub fn stats(path: &str) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        let c = std::ffi::CString::new(path).ok()?;
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::statfs(c.as_ptr(), &mut st) };
        if rc != 0 {
            return None;
        }
        let bsize = st.f_bsize as u64;
        Some((st.f_blocks * bsize, st.f_bavail * bsize))
    }
    #[cfg(windows)]
    {
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let p = wide(&native(path));
        let (mut avail, mut total) = (0u64, 0u64);
        unsafe {
            GetDiskFreeSpaceExW(
                windows::core::PCWSTR(p.as_ptr()),
                Some(&mut avail),
                Some(&mut total),
                None,
            )
        }
        .ok()?;
        Some((total, avail))
    }
}

/// A path as Windows' wide-string APIs take it, NUL-terminated.
#[cfg(windows)]
pub fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain([0]).collect()
}

/// The boot volume's display name. macOS lists it under /Volumes as a symlink to `/`, which is
/// the cheapest place to read the name without loading Foundation. Only the symlinks are read
/// (their type comes with the directory listing, and readlink doesn't follow them): every other
/// entry is a mount point, and looking at one waits on its volume, which for a network share
/// that has gone away can hang. On Windows, the system drive as File Explorer names it. Read
/// once.
pub fn boot_volume_name() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| read_volume_name("/")).clone()
}

#[cfg(not(windows))]
fn read_volume_name(_root: &str) -> String {
    if let Ok(rd) = std::fs::read_dir("/Volumes") {
        for e in rd.flatten() {
            if !e.file_type().is_ok_and(|t| t.is_symlink()) {
                continue;
            }
            if std::fs::read_link(e.path()).is_ok_and(|t| t == Path::new("/")) {
                return e.file_name().to_string_lossy().into_owned();
            }
        }
    }
    "Macintosh HD".into()
}

/// A drive as File Explorer names it: its label and letter, "Windows (C:)", or "Local Disk (C:)"
/// without a label.
#[cfg(windows)]
fn read_volume_name(root: &str) -> String {
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;
    let path = native(root);
    let letter = path.to_string_lossy().trim_end_matches('\\').to_string();
    let mut label = [0u16; 261];
    let p = wide(&path);
    let ok = unsafe {
        GetVolumeInformationW(
            windows::core::PCWSTR(p.as_ptr()),
            Some(&mut label),
            None,
            None,
            None,
            None,
        )
    }
    .is_ok();
    let n = label.iter().position(|&c| c == 0).unwrap_or(0);
    let label = String::from_utf16_lossy(&label[..n]);
    if ok && !label.trim().is_empty() {
        format!("{} ({letter})", label.trim())
    } else {
        format!("Local Disk ({letter})")
    }
}

/// A volume's root rather than a folder on it: `/`, or a drive's on Windows ("D:/").
pub fn is_volume_root(root: &str) -> bool {
    root == "/" || (cfg!(windows) && root.len() == 3 && root.ends_with(":/"))
}

/// What to call a scan root in the UI: the volume name for a volume's root, the folder's own
/// name otherwise.
pub fn root_name(root: &str) -> String {
    if root == "/" {
        boot_volume_name()
    } else if is_volume_root(root) {
        #[cfg(windows)]
        return read_volume_name(root);
        #[cfg(not(windows))]
        root.to_string()
    } else {
        Path::new(root)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.to_string())
    }
}

pub fn info(root: &str) -> Volume {
    let (total, free) = stats(root).unwrap_or((0, 0));
    Volume {
        name: root_name(root),
        total,
        free,
        used: total.saturating_sub(free),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_has_stats() {
        let (total, free) = stats("/").unwrap();
        assert!(total > 0 && free <= total);
    }

    #[test]
    fn names() {
        assert_eq!(root_name("/Users/x/Projects"), "Projects");
        assert!(!root_name("/").is_empty());
    }

    #[test]
    fn joins() {
        assert_eq!(join("/", "Users"), "/Users");
        assert_eq!(join("D:/", "Photos"), "D:/Photos");
        assert_eq!(join("/Users", "x"), "/Users/x");
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths() {
        let drive = system_drive();
        assert_eq!(native("/"), PathBuf::from(format!("{drive}\\")));
        assert_eq!(
            native("/Users/x"),
            PathBuf::from(format!("{drive}\\Users\\x"))
        );
        assert_eq!(native("Q:/Photos"), PathBuf::from("Q:\\Photos"));
        assert_eq!(native("Q:"), PathBuf::from("Q:\\"));
        assert_eq!(canonical(Path::new(&format!("{drive}\\"))), "/");
        assert_eq!(
            canonical(Path::new(&format!("{drive}\\Users\\x\\"))),
            "/Users/x"
        );
        assert_eq!(canonical(Path::new(r"q:\Photos")), "Q:/Photos");
        assert_eq!(canonical(Path::new(r"Q:\")), "Q:/");
        assert_eq!(
            canonical(Path::new(&format!(r"\\?\{drive}\Windows"))),
            "/Windows"
        );
        assert_eq!(canonical(Path::new(r"\\nas\share\x")), "//nas/share/x");
        assert_eq!(native("//nas/share/x"), PathBuf::from(r"\\nas\share\x"));
        assert!(is_volume_root("Q:/") && is_volume_root("/") && !is_volume_root("/Users"));
        assert!(root_name("/").ends_with(&format!("({drive})")));
    }
}
