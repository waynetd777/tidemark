// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

//! Crashes and hangs, from the reports macOS writes: one .ips file per event in
//! ~/Library/Logs/DiagnosticReports (this user's apps) and /Library/Logs/DiagnosticReports (the
//! system's), each starting with a line of JSON naming the app, the time and the kind (bug_type).
//! Reports macOS marks as simulated are faults it noted but the app survived, so they are left
//! out. The recorder reads the folders every few minutes and keeps each report once (by file
//! name) in the history.

use crate::history;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Header {
    #[serde(default)]
    app_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    bug_type: String,
    #[serde(default)]
    is_simulated: Option<serde_json::Value>,
}

/// What a report is about: "crash" or "hang"; None for the kinds Tidemark doesn't count.
fn kind_of(bug_type: &str) -> Option<&'static str> {
    match bug_type {
        // Crash reports (the JSON format, and the older one).
        "309" | "109" => Some("crash"),
        // Hangs and spins: an app that stopped responding.
        "298" | "288" | "228" => Some("hang"),
        _ => None,
    }
}

/// One report's first line, as (unix time, app, kind); None if it isn't one Tidemark counts.
pub fn parse_header(line: &str) -> Option<(i64, String, &'static str)> {
    let h: Header = serde_json::from_str(line).ok()?;
    if h.is_simulated
        .as_ref()
        .is_some_and(|v| v.as_i64() == Some(1) || v.as_bool() == Some(true))
    {
        return None;
    }
    let kind = kind_of(&h.bug_type)?;
    let ts = chrono::DateTime::parse_from_str(&h.timestamp, "%Y-%m-%d %H:%M:%S%.f %z")
        .ok()?
        .timestamp();
    let app = if h.app_name.is_empty() {
        h.name
    } else {
        h.app_name
    };
    (!app.is_empty()).then_some((ts, app, kind))
}

fn folders() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    vec![
        Path::new(&home).join("Library/Logs/DiagnosticReports"),
        PathBuf::from("/Library/Logs/DiagnosticReports"),
    ]
}

fn first_line(p: &Path) -> Option<String> {
    use std::io::{BufRead, BufReader, Read};
    let f = std::fs::File::open(p).ok()?;
    let mut line = String::new();
    BufReader::new(f.take(64 * 1024))
        .read_line(&mut line)
        .ok()?;
    Some(line)
}

/// Reads any reports not yet in the history, from the last `days` days.
pub fn collect(days: u32) {
    let since = history::now() - days as i64 * 86_400;
    let known = history::crash_files();
    for dir in folders() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("ips") {
                continue;
            }
            let file = p.to_string_lossy().to_string();
            if known.contains(&file) {
                continue;
            }
            // Old files are skipped without opening them.
            let recent = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .is_ok_and(|d| d.as_secs() as i64 >= since)
            });
            if !recent {
                continue;
            }
            if let Some((ts, app, kind)) = first_line(&p).as_deref().and_then(parse_header) {
                if ts >= since {
                    history::add_crash(ts, &app, kind, &file);
                }
            }
        }
    }
}

#[tauri::command]
pub async fn crashes(days: i64) -> Vec<history::Crash> {
    history::crashes(history::now() - days * 86_400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_crash_and_skips_a_simulated_one() {
        let real = r#"{"app_name":"Tidemark","timestamp":"2026-10-10 22:16:50.00 +0200","bug_type":"309","name":"Tidemark"}"#;
        let (ts, app, kind) = parse_header(real).unwrap();
        assert_eq!((app.as_str(), kind), ("Tidemark", "crash"));
        assert_eq!(ts, 1791663410);
        let simulated = r#"{"is_simulated":1,"app_name":"Google Chrome","timestamp":"2026-10-06 10:49:20.00 +0200","bug_type":"309"}"#;
        assert_eq!(parse_header(simulated), None);
        let other =
            r#"{"app_name":"x","timestamp":"2026-10-06 10:49:20.00 +0200","bug_type":"211"}"#;
        assert_eq!(parse_header(other), None);
    }
}
