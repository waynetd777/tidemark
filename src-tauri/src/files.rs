// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

use crate::dirlist;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
pub struct FileEntry {
    name: String,
    path: String,
    size: u64,
    disk_size: u64,
    modified: Option<u64>,
    kind: String,
}

// Read metadata only: opening a cloud placeholder's contents could download it.
fn list_files(path: &Path) -> Result<Vec<FileEntry>, String> {
    let entries = dirlist::list(path, false)
        .map_err(|e| format!("Could not read {}: {e}", crate::volume::canonical(path)))?;
    Ok(entries
        .into_iter()
        .filter(|e| e.kind != dirlist::Kind::Dir)
        .map(|e| {
            let kind = if e.kind == dirlist::Kind::Link {
                LINK.to_string()
            } else {
                Path::new(&e.name)
                    .extension()
                    .and_then(|s| s.to_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("{} document", s.to_uppercase()))
                    .unwrap_or_else(|| "File".into())
            };
            FileEntry {
                path: crate::volume::canonical(&path.join(&e.name)),
                name: e.name,
                size: e.len,
                disk_size: e.alloc,
                modified: e.modified,
                kind,
            }
        })
        .collect())
}

/// What a link is called: macOS's word for one, or Windows'.
const LINK: &str = if cfg!(windows) { "Link" } else { "Alias" };

#[tauri::command]
pub async fn get_files(path: String) -> Result<Vec<FileEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || list_files(&crate::volume::native(&path)))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn lists_direct_files_and_links_without_following_them() {
        let dir = std::env::temp_dir().join(format!("du-files-{}", std::process::id()));
        fs::create_dir_all(dir.join("child")).unwrap();
        fs::write(dir.join("example.txt"), "hello").unwrap();
        fs::write(dir.join("child/nested.txt"), "nested").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("missing"), dir.join("link")).unwrap();
        // A junction: a symbolic link needs a privilege on Windows, a junction none.
        #[cfg(windows)]
        assert!(
            crate::process::no_window(&mut std::process::Command::new("cmd"))
                .args(["/C", "mklink", "/J"])
                .arg(dir.join("link"))
                .arg(dir.join("child"))
                .output()
                .unwrap()
                .status
                .success()
        );
        let entries = list_files(&dir).unwrap();
        assert_eq!(entries.len(), 2);
        let file = entries.iter().find(|f| f.name == "example.txt").unwrap();
        assert_eq!(file.size, 5);
        assert_eq!(file.kind, "TXT document");
        assert!(file.modified.is_some());
        assert_eq!(
            entries.iter().find(|f| f.name == "link").unwrap().kind,
            LINK
        );
        fs::remove_dir_all(&dir).unwrap();
        assert!(list_files(&dir).is_err());
    }
}
