// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! One folder's entries with what the scan and the file list need of each: its kind, its length,
//! the space it takes on disk, when it changed, and whether it may have other hard links.
//!
//! On macOS that is the directory listing and an lstat per entry (`st_blocks * 512` on disk). On
//! Windows it is one call per folder, GetFileInformationByHandleEx(FileIdBothDirectoryInfo), whose
//! records carry each entry's allocation size, attributes and file id with its name, so no file
//! is opened: a cloud placeholder isn't downloaded, and a folder of 10,000 files costs one handle,
//! not 10,000. An online-only placeholder has a length but no allocation, as on macOS.
//!
//! Windows' records don't say how many links a file has. Hard links there are the system's own
//! (C:\Windows\WinSxS is linked into System32 and the rest of the Windows folder, where counting
//! every link would show the folder at nearly twice its size), so the scan asks for file ids only
//! inside the Windows folder and counts each id once there (`with_ids`).

use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    /// A symbolic link, or on Windows a junction or mount point: counted as itself, never
    /// followed.
    Link,
}

#[derive(Debug)]
pub struct Entry {
    pub name: String,
    pub kind: Kind,
    /// Logical bytes.
    pub len: u64,
    /// Bytes allocated on disk.
    pub alloc: u64,
    /// Last modified, unix seconds.
    pub modified: Option<u64>,
    /// A folder a cloud provider keeps (Windows' cloud files: OneDrive, iCloud and others).
    pub cloud: bool,
    /// Who the file is, when it may have other hard links: counted once by this key.
    pub link_key: Option<(u64, u64)>,
}

/// The entries of `dir`. `with_ids` (Windows only) asks for each file's id as its `link_key`.
#[cfg(unix)]
pub fn list(dir: &Path, _with_ids: bool) -> io::Result<Vec<Entry>> {
    use std::os::unix::fs::MetadataExt;
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)?.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        if ft.is_dir() {
            out.push(Entry {
                name,
                kind: Kind::Dir,
                len: 0,
                alloc: 0,
                modified: None,
                cloud: false,
                link_key: None,
            });
            continue;
        }
        // DirEntry::metadata is an lstat: a symlink counts as itself, never its target.
        let Ok(md) = e.metadata() else { continue };
        out.push(Entry {
            name,
            kind: if ft.is_symlink() {
                Kind::Link
            } else {
                Kind::File
            },
            len: md.len(),
            alloc: md.blocks() * 512,
            modified: md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            cloud: false,
            link_key: (md.nlink() > 1).then(|| (md.dev(), md.ino())),
        });
    }
    Ok(out)
}

#[cfg(windows)]
pub fn list(dir: &Path, with_ids: bool) -> io::Result<Vec<Entry>> {
    use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo,
        GetFileInformationByHandleEx, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    let path = crate::volume::wide(dir);
    let h = unsafe {
        CreateFileW(
            windows::core::PCWSTR(path.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .map_err(io::Error::from)?;
    let h = Handle(h);

    // u64s, so the records (8-byte aligned within it) are aligned in memory too.
    let mut buf = vec![0u64; 8 * 1024];
    let bytes = (buf.len() * 8) as u32;
    let mut out = Vec::new();
    let mut first = true;
    loop {
        let class = if first {
            FileIdBothDirectoryRestartInfo
        } else {
            FileIdBothDirectoryInfo
        };
        first = false;
        let got =
            unsafe { GetFileInformationByHandleEx(h.0, class, buf.as_mut_ptr() as *mut _, bytes) };
        if let Err(e) = got {
            if e.code() == ERROR_NO_MORE_FILES.to_hresult() {
                break;
            }
            // The folder went away, or can't be listed: what was read stands, if anything was.
            if out.is_empty() {
                return Err(io::Error::from(e));
            }
            break;
        }
        let base = buf.as_ptr() as *const u8;
        let mut off = 0usize;
        loop {
            // SAFETY: the system wrote whole records into buf, each NextEntryOffset bytes after
            // the last; FileName holds FileNameLength bytes.
            let rec = unsafe { &*(base.add(off) as *const FILE_ID_BOTH_DIR_INFO) };
            let name_ptr = std::ptr::addr_of!(rec.FileName) as *const u16;
            let units = rec.FileNameLength as usize / 2;
            let name =
                String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(name_ptr, units) });
            if name != "." && name != ".." {
                let attrs = rec.FileAttributes;
                let reparse = attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
                // With the reparse-point attribute, EaSize holds the reparse tag.
                let tag = if reparse { rec.EaSize } else { 0 };
                let kind = if is_name_surrogate(tag) {
                    Kind::Link
                } else if attrs & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
                    Kind::Dir
                } else {
                    Kind::File
                };
                out.push(Entry {
                    name,
                    kind,
                    len: rec.EndOfFile.max(0) as u64,
                    alloc: rec.AllocationSize.max(0) as u64,
                    modified: unix_secs(rec.LastWriteTime),
                    cloud: is_cloud(tag),
                    link_key: (with_ids && kind == Kind::File).then_some((0, rec.FileId as u64)),
                });
            }
            if rec.NextEntryOffset == 0 {
                break;
            }
            off += rec.NextEntryOffset as usize;
        }
    }
    Ok(out)
}

/// A reparse point that stands for another name: a symbolic link, a junction, a mount point.
/// Followed, a junction like the profile's "Application Data" would walk a folder again (or in
/// a loop). Other reparse points (cloud files, deduplicated files) are the files themselves.
#[cfg(windows)]
fn is_name_surrogate(tag: u32) -> bool {
    tag & 0x2000_0000 != 0
}

/// Windows' cloud files: IO_REPARSE_TAG_CLOUD and CLOUD_1 to CLOUD_F (0x9000_x01A), which the
/// sync root of OneDrive, iCloud and other providers and everything under it carry.
#[cfg(windows)]
fn is_cloud(tag: u32) -> bool {
    tag & 0xFFFF_0FFF == 0x9000_001A
}

/// A FILETIME (100 ns since 1601) as unix seconds.
#[cfg(windows)]
fn unix_secs(t: i64) -> Option<u64> {
    u64::try_from(t / 10_000_000 - 11_644_473_600).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_files_folders_and_sizes() {
        let dir = std::env::temp_dir().join(format!("tm-dirlist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("f.txt"), vec![b'x'; 5000]).unwrap();
        let mut e = list(&dir, true).unwrap();
        e.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].name.as_str(), e[0].kind), ("f.txt", Kind::File));
        assert_eq!(e[0].len, 5000);
        assert!(e[0].alloc >= 5000, "allocated in whole clusters");
        assert!(e[0].modified.is_some());
        assert_eq!(e[1].kind, Kind::Dir);
        assert!(list(&dir.join("missing"), false).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(windows)]
    #[test]
    fn ids_only_when_asked() {
        let dir = std::env::temp_dir().join(format!("tm-dirids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a"), "a").unwrap();
        std::fs::hard_link(dir.join("a"), dir.join("b")).unwrap();
        let e = list(&dir, true).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].link_key, e[1].link_key, "two links, one file id");
        assert!(e[0].link_key.is_some());
        assert!(list(&dir, false)
            .unwrap()
            .iter()
            .all(|e| e.link_key.is_none()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(windows)]
    #[test]
    fn filetimes() {
        assert_eq!(unix_secs(116_444_736_000_000_000), Some(0));
        assert_eq!(unix_secs(0), None);
        assert!(is_cloud(0x9000_001A) && is_cloud(0x9000_701A) && !is_cloud(0xA000_000C));
        assert!(is_name_surrogate(0xA000_0003) && !is_name_surrogate(0x9000_001A));
    }
}
