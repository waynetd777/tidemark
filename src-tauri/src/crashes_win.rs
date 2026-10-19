// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Crashes and hangs on Windows, as crashes.rs reads them on macOS: from the Application event
//! log, where Windows notes each one (Reliability Monitor is built on the same events), not from
//! Windows Error Reporting's folders, which keep only some of them. "Application Error" event
//! 1000 is a crash, "Application Hang" event 1002 an app that stopped responding and was closed.
//!
//! The log is read with wevtutil (no window), as XML; each event is kept once, by its record
//! number ("eventlog:Application:13167", in the history's file column), and Ask is told about
//! one by reading it again by that number.

use crate::history;
use std::process::Command;

const PREFIX: &str = "eventlog:Application:";

/// The query for crash and hang events, the last `ms` milliseconds or one record.
fn query(filter: &str) -> String {
    format!(
        "*[System[Provider[@Name='Application Error' or @Name='Application Hang'] \
         and (EventID=1000 or EventID=1002){filter}]]"
    )
}

fn wevtutil(query: &str, count: u32) -> Option<String> {
    let out = crate::process::no_window(&mut Command::new("wevtutil"))
        .args([
            "qe",
            "Application",
            &format!("/q:{query}"),
            "/f:xml",
            "/rd:true",
        ])
        .arg(format!("/c:{count}"))
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// One event, as the parts Tidemark uses.
#[derive(Debug, PartialEq)]
pub struct Event {
    pub record: u64,
    pub ts: i64,
    pub kind: &'static str,
    /// The EventData's Data elements, in order.
    pub data: Vec<String>,
}

/// The text between `open`'s end and `close`, after `from`.
fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let a = s.find(open)? + open.len();
    let b = s[a..].find(close)? + a;
    Some(&s[a..b])
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// wevtutil's XML: one <Event> per line.
pub fn parse(xml: &str) -> Vec<Event> {
    xml.split("<Event ")
        .skip(1)
        .filter_map(|e| {
            let id: u32 = between(e, "<EventID", "</EventID>")?
                .rsplit('>')
                .next()?
                .trim()
                .parse()
                .ok()?;
            let kind = match id {
                1000 => "crash",
                1002 => "hang",
                _ => return None,
            };
            let when = between(e, "SystemTime='", "'")?;
            let ts = chrono::DateTime::parse_from_rfc3339(when).ok()?.timestamp();
            let record = between(e, "<EventRecordID>", "</EventRecordID>")?
                .parse()
                .ok()?;
            let body = between(e, "<EventData>", "</EventData>").unwrap_or("");
            let data = body
                .split("<Data")
                .skip(1)
                .map(|d| {
                    // <Data>x</Data>, <Data/> or <Data Name='…'>x</Data>
                    if d.starts_with("/>") || d.trim_start().starts_with("/>") {
                        return String::new();
                    }
                    d.split_once('>')
                        .and_then(|(_, rest)| rest.split_once("</Data>"))
                        .map(|(v, _)| unescape(v))
                        .unwrap_or_default()
                })
                .collect();
            Some(Event {
                record,
                ts,
                kind,
                data,
            })
        })
        .collect()
}

/// The app an event is about, as the record names apps: the program's description from its
/// path when it can be read ("Firefox"), else its name ("firefox.exe" as "firefox").
fn app(e: &Event) -> String {
    let name = e.data.first().cloned().unwrap_or_default();
    // The program's path: Data 10 in a crash, 5 in a hang.
    let path = e.data.get(if e.kind == "crash" { 10 } else { 5 });
    crate::system_win::app_name(path.map(std::path::Path::new), &name)
}

/// Reads any events not yet in the history, from the last `days` days.
pub fn collect(days: u32) {
    let ms = days as u64 * 86_400_000;
    let Some(xml) = wevtutil(
        &query(&format!(" and TimeCreated[timediff(@SystemTime) <= {ms}]")),
        2000,
    ) else {
        return;
    };
    let known = history::crash_files();
    for e in parse(&xml) {
        let file = format!("{PREFIX}{}", e.record);
        if !known.contains(&file) {
            history::add_crash(e.ts, &app(&e), e.kind, &file);
        }
    }
}

/// What Ask is told about one event: the app, its version, and for a crash the module that
/// faulted and the exception.
pub fn report(file: &str) -> Result<String, String> {
    let record: u64 = file
        .strip_prefix(PREFIX)
        .and_then(|r| r.parse().ok())
        .ok_or("Not an event Tidemark has recorded.")?;
    let xml = wevtutil(&query(&format!(" and EventRecordID={record}")), 1)
        .ok_or("Couldn't read the event log.")?;
    let e = parse(&xml)
        .into_iter()
        .next()
        .ok_or("The event is no longer in the log.")?;
    Ok(summarise(&e))
}

pub fn summarise(e: &Event) -> String {
    let d = |i: usize| e.data.get(i).map(String::as_str).unwrap_or("");
    let when = chrono::DateTime::from_timestamp(e.ts, 0)
        .map(|t| t.with_timezone(&chrono::Local).to_rfc3339())
        .unwrap_or_default();
    let mut out = vec![
        format!("Event: Windows Application log, record {}", e.record),
        format!("App: {} {} · {when}", d(0), d(1)),
    ];
    if e.kind == "crash" {
        out.push(format!("Program: {}", d(10)));
        out.push(format!("Faulting module: {} {} ({})", d(3), d(4), d(11)));
        out.push(format!("Exception code: {} at offset {}", d(6), d(7)));
    } else {
        out.push(format!("Program: {}", d(5)));
        out.push("The program stopped responding and Windows closed it.".into());
        if !d(9).is_empty() && d(9) != "Unknown" {
            out.push(format!("Hang type: {}", d(9)));
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const HANG: &str = "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System><Provider Name='Application Hang'/><EventID Qualifiers='0'>1002</EventID><Version>0</Version><TimeCreated SystemTime='2026-09-07T09:00:54.8486792Z'/><EventRecordID>12028</EventRecordID></System><EventData><Data>firefox.exe</Data><Data>155.0.1.231</Data><Data>543c</Data><Data>01dd3ea72a7c114f</Data><Data>3994</Data><Data>C:\\Program Files\\Mozilla Firefox\\firefox.exe</Data><Data>7b36</Data><Data></Data><Data/><Data>Unknown</Data><Binary>55</Binary></EventData></Event>\n";
    const CRASH: &str = "<Event xmlns='x'><System><Provider Name='Application Error'/><EventID Qualifiers='0'>1000</EventID><TimeCreated SystemTime='2026-09-05T12:19:32Z'/><EventRecordID>11000</EventRecordID></System><EventData><Data>AOTListenerService.exe</Data><Data>26.3.1.38</Data><Data>684a0000</Data><Data>KERNELBASE.dll</Data><Data>10.0.19041.6456</Data><Data>0</Data><Data>e0434352</Data><Data>0012a8b9</Data><Data>1c2c</Data><Data>01dd</Data><Data>C:\\Program Files\\AOT\\AOTListenerService.exe</Data><Data>C:\\Windows\\System32\\KERNELBASE.dll</Data><Data>abc</Data><Data></Data><Data></Data></EventData></Event>\n";

    #[test]
    fn reads_hangs_and_crashes() {
        let e = parse(&format!("{HANG}{CRASH}"));
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].record, e[0].kind), (12028, "hang"));
        assert_eq!(e[0].ts, 1_788_771_654);
        assert_eq!(
            e[0].data[5],
            r"C:\Program Files\Mozilla Firefox\firefox.exe"
        );
        assert_eq!(e[0].data[8], "", "an empty <Data/> keeps its place");
        assert_eq!(e[0].data[9], "Unknown");
        assert_eq!((e[1].record, e[1].kind), (11000, "crash"));
        let s = summarise(&e[1]);
        assert!(s.contains("Faulting module: KERNELBASE.dll 10.0.19041.6456"));
        assert!(s.contains("Exception code: e0434352"));
        assert!(summarise(&e[0]).contains("stopped responding"));
    }

    #[test]
    fn reads_this_pcs_log() {
        // Whatever is there parses; an empty log is fine.
        let xml = wevtutil(&query(""), 5).unwrap_or_default();
        assert_eq!(parse(&xml).len(), xml.matches("<Event ").count());
    }
}
