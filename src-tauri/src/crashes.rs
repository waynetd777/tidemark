// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

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

/// Reports read and found not to count (simulated, another kind, unreadable): remembered so
/// they are not opened again every few minutes.
static SKIPPED: std::sync::Mutex<Option<std::collections::HashSet<String>>> =
    std::sync::Mutex::new(None);

/// Reads any reports not yet in the history, from the last `days` days.
pub fn collect(days: u32) {
    let since = history::now() - days as i64 * 86_400;
    let known = history::crash_files();
    let mut skipped = SKIPPED.lock().unwrap_or_else(|e| e.into_inner());
    let skipped = skipped.get_or_insert_with(Default::default);
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
            if known.contains(&file) || skipped.contains(&file) {
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
            match first_line(&p).as_deref().and_then(parse_header) {
                Some((ts, app, kind)) if ts >= since => history::add_crash(ts, &app, kind, &file),
                _ => {
                    skipped.insert(file);
                }
            }
        }
    }
}

#[tauri::command]
pub async fn crashes(days: i64) -> Vec<history::Crash> {
    history::crashes(history::now() - days * 86_400)
}

/// What Ask is told about one report: the app, the exception and why macOS ended it, and the
/// top of the crashed thread's stack. Only a report in the history can be read. A hang report is
/// text, so its first lines are given as they are.
#[tauri::command(async)]
pub fn crash_report(file: String) -> Result<String, String> {
    if !history::crash_files().contains(&file) {
        return Err("Not a report Tidemark has recorded.".into());
    }
    let raw =
        std::fs::read_to_string(&file).map_err(|e| format!("Couldn't read the report: {e}"))?;
    Ok(summarise(&file, &raw))
}

/// At most this much of a report goes to Ask.
const MAX: usize = 4000;

pub fn summarise(file: &str, raw: &str) -> String {
    let (first, body) = raw.split_once('\n').unwrap_or((raw, ""));
    let head: serde_json::Value = serde_json::from_str(first).unwrap_or_default();
    let s = |v: &serde_json::Value, k: &str| v[k].as_str().unwrap_or("").to_string();
    let mut out = vec![format!("Report: {file}")];
    let app = [s(&head, "app_name"), s(&head, "name")]
        .into_iter()
        .find(|a| !a.is_empty())
        .unwrap_or_default();
    out.push(format!(
        "App: {app} {} · {} · {}",
        s(&head, "app_version"),
        s(&head, "os_version"),
        s(&head, "timestamp")
    ));
    let Ok(b) = serde_json::from_str::<serde_json::Value>(body) else {
        // A hang (spindump) or an older plain-text report.
        out.push(String::new());
        out.push(body.lines().take(80).collect::<Vec<_>>().join("\n"));
        return clip(out.join("\n"));
    };
    let ex = &b["exception"];
    if ex.is_object() {
        out.push(format!(
            "Exception: {} {} {}",
            s(ex, "type"),
            s(ex, "signal"),
            s(ex, "codes")
        ));
    }
    let term = &b["termination"];
    if term.is_object() {
        let reasons: Vec<&str> = term["reasons"]
            .as_array()
            .map(|a| a.iter().filter_map(|r| r.as_str()).collect())
            .unwrap_or_default();
        out.push(format!(
            "Termination: {} {} {}",
            s(term, "namespace"),
            s(term, "indicator"),
            reasons.join("; ")
        ));
    }
    if let Some(asi) = b["asi"].as_object() {
        for (k, v) in asi {
            let lines: Vec<&str> = v
                .as_array()
                .map(|a| a.iter().filter_map(|l| l.as_str()).collect())
                .unwrap_or_default();
            out.push(format!("Application info ({k}): {}", lines.join(" ")));
        }
    }
    let images: Vec<String> = b["usedImages"]
        .as_array()
        .map(|a| a.iter().map(|i| s(i, "name")).collect())
        .unwrap_or_default();
    let n = b["faultingThread"].as_u64().unwrap_or(0) as usize;
    let thread = &b["threads"][n];
    let frames = thread["frames"].as_array().cloned().unwrap_or_default();
    if !frames.is_empty() {
        let label = [s(thread, "name"), s(thread, "queue")]
            .into_iter()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        out.push(format!(
            "Crashed thread {n}{}:",
            if label.is_empty() {
                String::new()
            } else {
                format!(" ({label})")
            }
        ));
        for f in frames.iter().take(16) {
            let image = f["imageIndex"]
                .as_u64()
                .and_then(|i| images.get(i as usize))
                .filter(|x| !x.is_empty())
                .cloned()
                .unwrap_or_else(|| "?".into());
            let sym = s(f, "symbol");
            out.push(if sym.is_empty() {
                format!("  {image} +{}", f["imageOffset"].as_u64().unwrap_or(0))
            } else {
                format!(
                    "  {image} {sym} +{}",
                    f["symbolLocation"].as_u64().unwrap_or(0)
                )
            });
        }
    } else {
        out.push("The crashed thread's stack is empty: the app was stopped before it ran.".into());
    }
    clip(out.join("\n"))
}

fn clip(mut s: String) -> String {
    if s.len() > MAX {
        let mut cut = MAX;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push_str("\n…");
    }
    s
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

    #[test]
    fn summarises_a_crash_for_ask() {
        let raw = concat!(
            r#"{"app_name":"Xcode","app_version":"26.0","os_version":"macOS 27.0","timestamp":"2026-10-10 22:16:50.00 +0200","bug_type":"309"}"#,
            "\n",
            r#"{"exception":{"type":"EXC_BAD_ACCESS","signal":"SIGSEGV","codes":"0x1"},"termination":{"namespace":"SIGNAL","indicator":"Segmentation fault: 11"},"faultingThread":1,"usedImages":[{"name":"Xcode"},{"name":"libsystem_kernel.dylib"}],"threads":[{"frames":[]},{"queue":"main","frames":[{"imageIndex":1,"symbol":"__pthread_kill","symbolLocation":8},{"imageIndex":0,"imageOffset":4096}]}]}"#
        );
        let s = summarise("/x.ips", raw);
        assert!(s.contains("App: Xcode 26.0"));
        assert!(s.contains("Exception: EXC_BAD_ACCESS SIGSEGV"));
        assert!(s.contains("Crashed thread 1 (main):"));
        assert!(s.contains("libsystem_kernel.dylib __pthread_kill +8"));
        assert!(s.contains("Xcode +4096"));
        // A text report keeps its first lines.
        let hang = summarise(
            "/h.ips",
            "{\"app_name\":\"Photos\"}\nDate/Time: today\nDuration: 9s",
        );
        assert!(hang.contains("Duration: 9s"));
    }
}
