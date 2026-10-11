// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! What the history says over days and months: the disk's used space and its high-water mark,
//! the folders that grew, one folder's size over time, and when the Mac is usually busy.

use crate::history::{self, DiskPoint, Growth};
use serde::Serialize;

fn day_n_ago(days: i64) -> String {
    (chrono::Local::now() - chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

#[derive(Serialize)]
pub struct DiskTrend {
    /// Used space over the range: hourly up to 14 days, a point a day beyond.
    pub points: Vec<DiskPoint>,
    /// The highest used space in the range, and when.
    pub high: Option<DiskPoint>,
}

#[tauri::command]
pub async fn disk_trend(days: i64) -> DiskTrend {
    let points = if days <= 14 {
        history::disk_used_range(history::now() - days * 86_400)
    } else {
        history::days(&day_n_ago(days))
            .into_iter()
            .filter(|d| d.total > 0.0)
            .filter_map(|d| {
                let ts = chrono::NaiveDate::parse_from_str(&d.day, "%Y-%m-%d")
                    .ok()?
                    .and_hms_opt(12, 0, 0)?
                    .and_local_timezone(chrono::Local)
                    .single()?
                    .timestamp();
                Some(DiskPoint {
                    ts,
                    used: d.used_max,
                    total: d.total,
                })
            })
            .collect()
    };
    let high = points
        .iter()
        .max_by(|a, b| a.used.total_cmp(&b.used))
        .cloned();
    DiskTrend { points, high }
}

#[derive(Serialize)]
pub struct GrowthInfo {
    /// Where the growth landed (see history::landed), biggest first.
    pub landed: Vec<Growth>,
    /// What those add up to, a folder inside another listed one counted once.
    pub total: f64,
    /// Every remembered folder that changed by 50 MB or more, for the map.
    pub all: Vec<Growth>,
    /// The two scans compared.
    pub from: Option<i64>,
    pub to: Option<i64>,
}

/// What grew under `root` over the last `days` days. Up to the history's 14 days this compares
/// scans; beyond, the daily top folders (coarser: the 40 biggest, four levels deep).
#[tauri::command]
pub async fn disk_growth(root: String, days: i64) -> GrowthInfo {
    if days <= 14 {
        let (all, from, to) = history::growth(&root, history::now() - days * 86_400);
        let landed = history::landed(&all, &root);
        return GrowthInfo {
            total: history::landed_total(&landed),
            landed,
            all,
            from,
            to,
        };
    }
    let (old, new) = history::daily_tops(&day_n_ago(days));
    let mut all: Vec<Growth> = new
        .iter()
        .map(|(p, s)| {
            let before = old.get(p).copied().unwrap_or(0.0);
            Growth {
                path: p.clone(),
                size: *s,
                before,
                delta: s - before,
            }
        })
        .filter(|g| g.delta.abs() >= 50e6 && !old.is_empty())
        .collect();
    all.sort_by(|a, b| b.delta.total_cmp(&a.delta));
    let landed = history::landed(&all, "/");
    GrowthInfo {
        total: history::landed_total(&landed),
        landed,
        all,
        from: None,
        to: None,
    }
}

#[tauri::command]
pub async fn folder_history(path: String, days: i64) -> Vec<(i64, f64)> {
    history::folder_history(&path, history::now() - days * 86_400)
}

/// A measure's average by weekday and hour over the minutes kept (14 days at most): rows
/// Monday..Sunday, 24 columns, None where nothing was recorded. `metric` as `history::by_hour`.
#[tauri::command]
pub async fn perf_by_hour(metric: String) -> Vec<Vec<Option<f64>>> {
    let mut grid = vec![vec![None; 24]; 7];
    for (wd, h, v) in history::by_hour(&metric, history::now() - 14 * 86_400) {
        // SQLite's %w is 0 = Sunday; the grid starts on Monday.
        let row = (wd as usize + 6) % 7;
        if let Some(cell) = grid.get_mut(row).and_then(|r| r.get_mut(h as usize)) {
            *cell = Some(v);
        }
    }
    grid
}

/// Each day of the last `days`, a row of each measure (kept for a year).
#[tauri::command]
pub async fn perf_days(days: i64) -> Vec<history::PerfDay> {
    history::perf_days_since(history::now() - days * 86_400)
}

#[derive(Serialize)]
pub struct HistoryInfo {
    pub bytes: u64,
    pub since: Option<i64>,
    /// The daily record (Trends): how many days, and the first one's local midnight.
    pub days: i64,
    pub days_since: Option<i64>,
}

#[tauri::command]
pub async fn history_info() -> HistoryInfo {
    let (days, days_since) = history::daily_extent();
    HistoryInfo {
        bytes: history::bytes(),
        since: history::first_minute(),
        days,
        days_since,
    }
}
