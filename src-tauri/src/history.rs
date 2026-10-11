// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Everything Tidemark remembers over time, in one SQLite file beside the cache
//! (history.sqlite). Per-minute performance, the busiest apps of each minute, incidents, the
//! disk's used space hourly and the folders of each scan are kept for the history setting's days
//! (14 at most); one row a day of disk totals and top folders, and one of each performance
//! measure (perf_daily, rolled up from the minutes), is kept for a year.
//!
//! Times are unix seconds. Sizes are bytes, CPU is a percentage of the whole machine.

use crate::scan::Node;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS perf (
  ts INTEGER PRIMARY KEY,
  cpu REAL, cpu_max REAL,
  mem REAL, mem_max REAL, mem_total REAL,
  swap REAL, swap_max REAL,
  pressure INTEGER,
  disk_r REAL, disk_w REAL, disk_max REAL,
  net_in REAL, net_out REAL, net_max REAL,
  thermal INTEGER
);
CREATE TABLE IF NOT EXISTS procs (
  ts INTEGER, app TEXT, cpu REAL, mem REAL,
  PRIMARY KEY (ts, app)
);
CREATE TABLE IF NOT EXISTS incidents (
  id INTEGER PRIMARY KEY, kind TEXT, started INTEGER, ended INTEGER, peak REAL, peak_at INTEGER,
  snapshot TEXT
);
CREATE INDEX IF NOT EXISTS incidents_started ON incidents (started);
CREATE TABLE IF NOT EXISTS disk_used (
  ts INTEGER PRIMARY KEY, used REAL, total REAL
);
CREATE TABLE IF NOT EXISTS disk_snap (
  ts INTEGER, root TEXT, path TEXT, size REAL,
  PRIMARY KEY (ts, path)
);
CREATE INDEX IF NOT EXISTS disk_snap_root ON disk_snap (root, ts);
CREATE INDEX IF NOT EXISTS disk_snap_path ON disk_snap (path, ts);
CREATE TABLE IF NOT EXISTS crashes (
  ts INTEGER, app TEXT, kind TEXT, file TEXT PRIMARY KEY
);
CREATE INDEX IF NOT EXISTS crashes_ts ON crashes (ts);
CREATE TABLE IF NOT EXISTS sleeps (
  started INTEGER PRIMARY KEY, ended INTEGER, battery_before REAL, battery_after REAL
);
CREATE TABLE IF NOT EXISTS disk_daily (
  day TEXT PRIMARY KEY, used REAL, used_max REAL, total REAL, top TEXT
);
CREATE TABLE IF NOT EXISTS perf_daily (
  day TEXT PRIMARY KEY, ts INTEGER, minutes INTEGER,
  cpu REAL, cpu_max REAL, cpu_p REAL, cpu_e REAL, gpu REAL, gpu_max REAL,
  mem REAL, mem_max REAL, mem_total REAL, swap_max REAL, pressure INTEGER, thermal INTEGER,
  disk_r REAL, disk_w REAL, disk_max REAL, net_in REAL, net_out REAL, net_in_max REAL,
  net_out_max REAL, power REAL, power_max REAL,
  read REAL, written REAL, down REAL, up REAL, energy REAL,
  incidents INTEGER, crashes INTEGER, apps TEXT
);
";

/// Growth compares scans at least this far apart (20 hours).
const MIN_APART: i64 = 20 * 3600;
/// Folders at least this big are kept in each scan's snapshot.
const SNAP_MIN: u64 = 100_000_000;
/// ...down to this many levels below the scan root.
const SNAP_DEPTH: u32 = 8;
/// The daily row's top folders: this many, at most this deep (from /).
const DAILY_TOP: usize = 40;
const DAILY_DEPTH: u32 = 4;

static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

fn open() -> Connection {
    let dir = crate::cache::dir();
    let _ = std::fs::create_dir_all(&dir);
    let c = Connection::open(dir.join("history.sqlite"))
        .or_else(|e| {
            eprintln!("history: {e}; keeping it in memory");
            Connection::open_in_memory()
        })
        .expect("sqlite opens in memory");
    // The app and the background helper can both write (a scan each, say): wait for the other.
    let _ = c.busy_timeout(std::time::Duration::from_secs(10));
    if let Err(e) = c.execute_batch(SCHEMA) {
        eprintln!("history schema: {e}");
    }
    // Columns added after the first release of the schema; an error means it is there already.
    for sql in [
        "ALTER TABLE perf ADD COLUMN power REAL",
        "ALTER TABLE perf ADD COLUMN power_max REAL",
        "ALTER TABLE perf ADD COLUMN battery REAL",
        "ALTER TABLE perf ADD COLUMN on_ac INTEGER",
        "ALTER TABLE procs ADD COLUMN energy REAL",
        "ALTER TABLE perf ADD COLUMN gpu REAL",
        "ALTER TABLE perf ADD COLUMN gpu_max REAL",
        "ALTER TABLE perf ADD COLUMN mem_app REAL",
        "ALTER TABLE perf ADD COLUMN mem_wired REAL",
        "ALTER TABLE perf ADD COLUMN mem_comp REAL",
        "ALTER TABLE perf ADD COLUMN net_in_max REAL",
        "ALTER TABLE perf ADD COLUMN net_out_max REAL",
        "ALTER TABLE perf ADD COLUMN cpu_p REAL",
        "ALTER TABLE perf ADD COLUMN cpu_e REAL",
        "ALTER TABLE procs ADD COLUMN cpu_max REAL",
        "ALTER TABLE procs ADD COLUMN mem_max REAL",
        "ALTER TABLE procs ADD COLUMN energy_max REAL",
        "ALTER TABLE procs ADD COLUMN disk REAL",
        "ALTER TABLE procs ADD COLUMN disk_max REAL",
    ] {
        let _ = c.execute(sql, []);
    }
    c
}

/// Runs `f` on the database. A failed query is logged and becomes the default: history is
/// worth having, never worth stopping the app for.
fn with<T: Default>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> T {
    let db = DB.get_or_init(|| Mutex::new(open()));
    // A query that panicked mid-way leaves the lock poisoned; the connection is still sound.
    let c = db.lock().unwrap_or_else(|e| e.into_inner());
    f(&c).unwrap_or_else(|e| {
        eprintln!("history: {e}");
        T::default()
    })
}

pub fn now() -> i64 {
    chrono::Local::now().timestamp()
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

// ---- performance ----

#[derive(Serialize, Clone, Debug, Default)]
pub struct Minute {
    pub ts: i64,
    pub cpu: f64,
    pub cpu_max: f64,
    pub mem: f64,
    pub mem_max: f64,
    pub mem_total: f64,
    pub swap: f64,
    pub swap_max: f64,
    /// macOS memory pressure: 1 normal, 2 warning, 4 critical. The minute's worst.
    pub pressure: i64,
    pub disk_r: f64,
    pub disk_w: f64,
    pub disk_max: f64,
    pub net_in: f64,
    pub net_out: f64,
    pub net_max: f64,
    /// NSProcessInfo thermal state: 0 nominal .. 3 critical. The minute's worst.
    pub thermal: i64,
    /// What the Mac drew, watts (average and peak); None without a battery controller.
    #[serde(default)]
    pub power: Option<f64>,
    #[serde(default)]
    pub power_max: Option<f64>,
    /// Battery charge, 0-100, and whether a power adapter was connected.
    #[serde(default)]
    pub battery: Option<f64>,
    #[serde(default)]
    pub on_ac: bool,
    /// GPU utilisation, 0-100 (average and peak); None if no GPU reports it.
    #[serde(default)]
    pub gpu: Option<f64>,
    #[serde(default)]
    pub gpu_max: Option<f64>,
    /// What memory in use was made of, bytes (averages).
    #[serde(default)]
    pub mem_app: Option<f64>,
    #[serde(default)]
    pub mem_wired: Option<f64>,
    #[serde(default)]
    pub mem_comp: Option<f64>,
    /// The busiest moment of download and of upload in the minute, bytes a second.
    #[serde(default)]
    pub net_in_max: f64,
    #[serde(default)]
    pub net_out_max: f64,
    /// The performance cores' and the efficiency cores' average use, 0-100; None before they
    /// were recorded, or on a Mac that doesn't say which core is which.
    #[serde(default)]
    pub cpu_p: Option<f64>,
    #[serde(default)]
    pub cpu_e: Option<f64>,
    /// Minutes recorded in this point (1 for a minute; a bucket's count), so a rate times
    /// `minutes * 60` is what moved.
    #[serde(default)]
    pub minutes: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppUse {
    pub app: String,
    pub cpu: f64,
    pub mem: f64,
    /// Watts.
    #[serde(default)]
    pub energy: f64,
    /// Disk read and written, bytes a second.
    #[serde(default)]
    pub disk: f64,
    /// The busiest moment of each: within the minute for a history row, within the span for
    /// `peak_apps`; 0 where it wasn't recorded (rows from before these were kept). A moment is
    /// one reading of the app: 2 seconds while a window is open, 15 otherwise (monitor.rs).
    #[serde(default)]
    pub cpu_max: f64,
    #[serde(default)]
    pub mem_max: f64,
    #[serde(default)]
    pub energy_max: f64,
    #[serde(default)]
    pub disk_max: f64,
}

pub fn add_minute(m: &Minute, apps: &[AppUse]) {
    add_minute_rows(m, apps);
    // Today's row, every ten minutes (and the day before, at the turn of the day).
    if m.ts % 600 == 0 {
        roll_up_recent();
    }
}

fn add_minute_rows(m: &Minute, apps: &[AppUse]) {
    with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO perf (ts, cpu, cpu_max, mem, mem_max, mem_total, swap, swap_max,
               pressure, disk_r, disk_w, disk_max, net_in, net_out, net_max, thermal, power,
               power_max, battery, on_ac, gpu, gpu_max, mem_app, mem_wired, mem_comp, net_in_max,
               net_out_max, cpu_p, cpu_e)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,
                     ?21,?22,?23,?24,?25,?26,?27,?28,?29)",
            params![
                m.ts, m.cpu, m.cpu_max, m.mem, m.mem_max, m.mem_total, m.swap, m.swap_max,
                m.pressure, m.disk_r, m.disk_w, m.disk_max, m.net_in, m.net_out, m.net_max,
                m.thermal, m.power, m.power_max, m.battery, m.on_ac, m.gpu, m.gpu_max,
                m.mem_app, m.mem_wired, m.mem_comp, m.net_in_max, m.net_out_max, m.cpu_p, m.cpu_e
            ],
        )?;
        let mut st = c.prepare_cached(
            "INSERT OR REPLACE INTO procs (ts, app, cpu, mem, energy, disk, cpu_max, mem_max,
               energy_max, disk_max) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        )?;
        for a in apps {
            st.execute(params![
                m.ts,
                a.app,
                a.cpu,
                a.mem,
                a.energy,
                a.disk,
                a.cpu_max,
                a.mem_max,
                a.energy_max,
                a.disk_max
            ])?;
        }
        Ok(())
    })
}

/// The minutes between `from` and `to`, folded into at most `points` buckets. Averages are
/// averaged; maxima stay maxima (a peak is never averaged away).
pub fn perf_range(from: i64, to: i64, points: i64) -> Vec<Minute> {
    let bucket = ((to - from) / points.max(1)).max(60);
    with(|c| {
        let mut st = c.prepare_cached(
            "SELECT MIN(ts), AVG(cpu), MAX(cpu_max), AVG(mem), MAX(mem_max), MAX(mem_total),
                    AVG(swap), MAX(swap_max), MAX(pressure), AVG(disk_r), AVG(disk_w), MAX(disk_max),
                    AVG(net_in), AVG(net_out), MAX(net_max), MAX(thermal), COUNT(*),
                    AVG(power), MAX(power_max), AVG(battery), MAX(on_ac),
                    AVG(gpu), MAX(gpu_max), AVG(mem_app), AVG(mem_wired), AVG(mem_comp),
                    MAX(COALESCE(net_in_max, net_in)), MAX(COALESCE(net_out_max, net_out)),
                    AVG(cpu_p), AVG(cpu_e)
             FROM perf WHERE ts >= ?1 AND ts <= ?2 GROUP BY (ts - ?1) / ?3 ORDER BY 1",
        )?;
        let rows = st.query_map(params![from, to, bucket], |r| {
            Ok(Minute {
                ts: r.get(0)?,
                cpu: r.get(1)?,
                cpu_max: r.get(2)?,
                mem: r.get(3)?,
                mem_max: r.get(4)?,
                mem_total: r.get(5)?,
                swap: r.get(6)?,
                swap_max: r.get(7)?,
                pressure: r.get(8)?,
                disk_r: r.get(9)?,
                disk_w: r.get(10)?,
                disk_max: r.get(11)?,
                net_in: r.get(12)?,
                net_out: r.get(13)?,
                net_max: r.get(14)?,
                thermal: r.get(15)?,
                minutes: r.get(16)?,
                power: r.get(17)?,
                power_max: r.get(18)?,
                battery: r.get(19)?,
                on_ac: r.get::<_, Option<i64>>(20)?.unwrap_or(0) != 0,
                gpu: r.get(21)?,
                gpu_max: r.get(22)?,
                mem_app: r.get(23)?,
                mem_wired: r.get(24)?,
                mem_comp: r.get(25)?,
                net_in_max: r.get::<_, Option<f64>>(26)?.unwrap_or(0.0),
                net_out_max: r.get::<_, Option<f64>>(27)?.unwrap_or(0.0),
                cpu_p: r.get(28)?,
                cpu_e: r.get(29)?,
            })
        })?;
        rows.collect()
    })
}

/// The highest point of one metric since `from`: (value, when). `col` is a perf column.
pub fn high(col: &str, from: i64) -> Option<(f64, i64)> {
    // Only ever called with the fixed names below; never with anything from the UI.
    debug_assert!([
        "cpu_max",
        "mem_max",
        "swap_max",
        "disk_max",
        "net_max",
        "power_max",
        "gpu_max",
        "net_in_max",
        "net_out_max"
    ]
    .contains(&col));
    with(|c| {
        c.query_row(
            &format!(
                "SELECT {col}, ts FROM perf WHERE ts >= ?1 AND {col} IS NOT NULL
                 ORDER BY {col} DESC LIMIT 1"
            ),
            params![from],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
    })
}

#[derive(Clone, Copy)]
pub enum By {
    Cpu,
    Mem,
    Energy,
    Disk,
}

/// The apps that used the most between `from` and `to`, averaged over the minutes they ran,
/// by CPU, memory or energy.
pub fn top_apps(from: i64, to: i64, by: By, n: usize) -> Vec<AppUse> {
    let order = match by {
        By::Cpu => "cpu",
        By::Mem => "mem",
        By::Energy => "energy",
        By::Disk => "disk",
    };
    apps_query(from, to, order, n)
}

/// The apps that were busiest at their busiest between `from` and `to`, by each app's peak
/// within a minute: the app behind a short spike, at its real figure. A row from before peaks
/// were kept counts its average.
pub fn peak_apps(from: i64, to: i64, by: By, n: usize) -> Vec<AppUse> {
    let order = match by {
        By::Cpu => "cpu_max",
        By::Mem => "mem_max",
        By::Energy => "energy_max",
        By::Disk => "disk_max",
    };
    apps_query(from, to, order, n)
}

/// The apps that used the most in all between `from` and `to` (a day): ranked by their total,
/// so an app that ran all day at 30% comes before one that ran two minutes at 90%; the figures
/// are still averages over the minutes each ran.
pub fn day_apps(from: i64, to: i64, by: By, n: usize) -> Vec<AppUse> {
    let order = match by {
        By::Cpu => "SUM(cpu)",
        By::Mem => "SUM(mem)",
        By::Energy => "SUM(COALESCE(energy, 0))",
        By::Disk => "SUM(COALESCE(disk, 0))",
    };
    apps_query(from, to, order, n)
}

fn apps_query(from: i64, to: i64, order: &str, n: usize) -> Vec<AppUse> {
    with(|c| {
        let mut st = c.prepare(&format!(
            "SELECT app, AVG(cpu) cpu, AVG(mem) mem, AVG(COALESCE(energy, 0)) energy,
                    AVG(COALESCE(disk, 0)) disk, MAX(COALESCE(cpu_max, cpu)) cpu_max,
                    MAX(COALESCE(mem_max, mem)) mem_max,
                    MAX(COALESCE(energy_max, energy, 0)) energy_max,
                    MAX(COALESCE(disk_max, disk, 0)) disk_max
             FROM procs WHERE ts >= ?1 AND ts <= ?2 GROUP BY app ORDER BY {order} DESC LIMIT ?3"
        ))?;
        let rows = st.query_map(params![from, to, n as i64], |r| {
            Ok(AppUse {
                app: r.get(0)?,
                cpu: r.get(1)?,
                mem: r.get(2)?,
                energy: r.get(3)?,
                disk: r.get(4)?,
                cpu_max: r.get(5)?,
                mem_max: r.get(6)?,
                energy_max: r.get(7)?,
                disk_max: r.get(8)?,
            })
        })?;
        rows.collect()
    })
}

pub fn first_minute() -> Option<i64> {
    with(|c| c.query_row("SELECT MIN(ts) FROM perf", [], |r| r.get(0)))
}

// ---- crashes ----

#[derive(Serialize, Clone, Debug, Default)]
pub struct Crash {
    pub ts: i64,
    pub app: String,
    /// "crash" or "hang".
    pub kind: String,
    /// The report macOS wrote, for Ask.
    pub file: String,
}

pub fn add_crash(ts: i64, app: &str, kind: &str, file: &str) {
    with(|c| {
        c.execute(
            "INSERT OR IGNORE INTO crashes VALUES (?1,?2,?3,?4)",
            params![ts, app, kind, file],
        )?;
        Ok(())
    })
}

/// The report files already kept, so each is read once.
pub fn crash_files() -> std::collections::HashSet<String> {
    with(|c| {
        let mut st = c.prepare("SELECT file FROM crashes")?;
        let rows = st.query_map([], |r| r.get(0))?;
        rows.collect()
    })
}

pub fn crashes(from: i64) -> Vec<Crash> {
    with(|c| {
        let mut st =
            c.prepare("SELECT ts, app, kind, file FROM crashes WHERE ts >= ?1 ORDER BY ts DESC")?;
        let rows = st.query_map(params![from], |r| {
            Ok(Crash {
                ts: r.get(0)?,
                app: r.get(1)?,
                kind: r.get(2)?,
                file: r.get(3)?,
            })
        })?;
        rows.collect()
    })
}

// ---- sleep ----

#[derive(Serialize, Clone, Debug, Default)]
pub struct Sleep {
    pub started: i64,
    pub ended: i64,
    pub battery_before: Option<f64>,
    pub battery_after: Option<f64>,
}

/// A stretch the Mac spent asleep (or the record was stopped), with the battery either side.
pub fn add_sleep(s: &Sleep) {
    with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO sleeps VALUES (?1,?2,?3,?4)",
            params![s.started, s.ended, s.battery_before, s.battery_after],
        )?;
        Ok(())
    })
}

pub fn sleeps(from: i64) -> Vec<Sleep> {
    with(|c| {
        let mut st = c.prepare(
            "SELECT started, ended, battery_before, battery_after FROM sleeps WHERE ended >= ?1
             ORDER BY started DESC",
        )?;
        let rows = st.query_map(params![from], |r| {
            Ok(Sleep {
                started: r.get(0)?,
                ended: r.get(1)?,
                battery_before: r.get(2)?,
                battery_after: r.get(3)?,
            })
        })?;
        rows.collect()
    })
}

// ---- incidents ----

#[derive(Serialize, Clone, Debug, Default)]
pub struct Incident {
    pub id: i64,
    /// "cpu", "pressure" or "swap".
    pub kind: String,
    pub started: i64,
    /// None while it is still going on.
    pub ended: Option<i64>,
    pub peak: f64,
    pub peak_at: i64,
    /// What each app was doing at the peak.
    pub snapshot: Vec<AppUse>,
}

pub fn open_incident(kind: &str, started: i64, peak: f64, snapshot: &[AppUse]) -> i64 {
    let snap = serde_json::to_string(snapshot).unwrap_or_default();
    with(|c| {
        c.execute(
            "INSERT INTO incidents (kind, started, ended, peak, peak_at, snapshot) VALUES (?1,?2,NULL,?3,?2,?4)",
            params![kind, started, peak, snap],
        )?;
        Ok(c.last_insert_rowid())
    })
}

pub fn update_incident(id: i64, peak: f64, peak_at: i64, snapshot: Option<&[AppUse]>) {
    with(|c| {
        match snapshot {
            Some(s) => c.execute(
                "UPDATE incidents SET peak = ?2, peak_at = ?3, snapshot = ?4 WHERE id = ?1",
                params![
                    id,
                    peak,
                    peak_at,
                    serde_json::to_string(s).unwrap_or_default()
                ],
            )?,
            None => c.execute(
                "UPDATE incidents SET peak = ?2, peak_at = ?3 WHERE id = ?1",
                params![id, peak, peak_at],
            )?,
        };
        Ok(())
    })
}

pub fn close_incident(id: i64, ended: i64) {
    with(|c| {
        c.execute(
            "UPDATE incidents SET ended = ?2 WHERE id = ?1",
            params![id, ended],
        )?;
        Ok(())
    })
}

/// Incidents left open by an app that stopped (quit, crash) end where the record stops.
pub fn close_dangling() {
    with(|c| {
        c.execute(
            "UPDATE incidents SET ended = MAX(peak_at, started) WHERE ended IS NULL",
            [],
        )?;
        Ok(())
    })
}

pub fn incidents(from: i64) -> Vec<Incident> {
    with(|c| {
        let mut st = c.prepare(
            "SELECT id, kind, started, ended, peak, peak_at, snapshot FROM incidents
             WHERE started >= ?1 ORDER BY started DESC",
        )?;
        let rows = st.query_map(params![from], |r| {
            let snap: String = r.get(6)?;
            Ok(Incident {
                id: r.get(0)?,
                kind: r.get(1)?,
                started: r.get(2)?,
                ended: r.get(3)?,
                peak: r.get(4)?,
                peak_at: r.get(5)?,
                snapshot: serde_json::from_str(&snap).unwrap_or_default(),
            })
        })?;
        rows.collect()
    })
}

// ---- disk ----

/// The boot volume's used space, sampled about hourly.
pub fn add_disk_used(ts: i64, used: u64, total: u64) {
    with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO disk_used VALUES (?1,?2,?3)",
            params![ts, used as f64, total as f64],
        )?;
        c.execute(
            "INSERT INTO disk_daily (day, used, used_max, total, top) VALUES (?1,?2,?2,?3,NULL)
             ON CONFLICT(day) DO UPDATE SET used = ?2, used_max = MAX(used_max, ?2), total = ?3",
            params![today(), used as f64, total as f64],
        )?;
        Ok(())
    })
}

pub fn last_disk_used() -> Option<i64> {
    with(|c| c.query_row("SELECT MAX(ts) FROM disk_used", [], |r| r.get(0)))
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct DiskPoint {
    pub ts: i64,
    pub used: f64,
    pub total: f64,
}

pub fn disk_used_range(from: i64) -> Vec<DiskPoint> {
    with(|c| {
        let mut st =
            c.prepare("SELECT ts, used, total FROM disk_used WHERE ts >= ?1 ORDER BY ts")?;
        let rows = st.query_map(params![from], |r| {
            Ok(DiskPoint {
                ts: r.get(0)?,
                used: r.get(1)?,
                total: r.get(2)?,
            })
        })?;
        rows.collect()
    })
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Day {
    pub day: String,
    pub used: f64,
    /// The day's high-water mark.
    pub used_max: f64,
    pub total: f64,
}

pub fn days(since: &str) -> Vec<Day> {
    with(|c| {
        let mut st = c.prepare(
            "SELECT day, used, used_max, total FROM disk_daily WHERE day >= ?1 ORDER BY day",
        )?;
        let rows = st.query_map(params![since], |r| {
            Ok(Day {
                day: r.get(0)?,
                used: r.get(1)?,
                used_max: r.get(2)?,
                total: r.get(3)?,
            })
        })?;
        rows.collect()
    })
}

/// The top-folder sizes of the first day on or after `since` and of the latest day.
pub fn daily_tops(since: &str) -> (HashMap<String, f64>, HashMap<String, f64>) {
    let get = |sql: &str, arg: &str| -> HashMap<String, f64> {
        with(|c| {
            let s: Option<String> = c
                .query_row(sql, params![arg], |r| r.get(0))
                .optional()?
                .flatten();
            Ok(s.and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default())
        })
    };
    (
        get(
            "SELECT top FROM disk_daily WHERE day >= ?1 AND top IS NOT NULL ORDER BY day LIMIT 1",
            since,
        ),
        get(
            "SELECT top FROM disk_daily WHERE day >= ?1 AND top IS NOT NULL ORDER BY day DESC LIMIT 1",
            since,
        ),
    )
}

fn join(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

/// Folders worth remembering from one scan: big enough, not too deep.
fn snap_rows(node: &Node, path: &str, depth: u32, out: &mut Vec<(String, u64)>) {
    if node.size < SNAP_MIN {
        return;
    }
    out.push((path.to_string(), node.size));
    if depth >= SNAP_DEPTH {
        return;
    }
    for k in &node.kids {
        snap_rows(k, &join(path, &k.name), depth + 1, out);
    }
}

/// Keeps what a finished scan found, so later scans can say what grew.
pub fn add_scan(root: &str, ts: i64, tree: &Node) {
    let mut rows = Vec::new();
    snap_rows(tree, root, 0, &mut rows);
    let daily_top = (root == "/").then(|| {
        let mut top: Vec<&(String, u64)> = rows
            .iter()
            .filter(|(p, _)| p != "/" && p.matches('/').count() as u32 <= DAILY_DEPTH)
            .collect();
        top.sort_by_key(|t| std::cmp::Reverse(t.1));
        top.truncate(DAILY_TOP);
        let m: HashMap<&str, u64> = top.iter().map(|(p, s)| (p.as_str(), *s)).collect();
        serde_json::to_string(&m).unwrap_or_default()
    });
    with(|c| {
        let tx = c.unchecked_transaction()?;
        {
            let mut st =
                tx.prepare_cached("INSERT OR REPLACE INTO disk_snap VALUES (?1,?2,?3,?4)")?;
            for (p, s) in &rows {
                st.execute(params![ts, root, p, *s as f64])?;
            }
        }
        if let Some(top) = &daily_top {
            tx.execute(
                "INSERT INTO disk_daily (day, used, used_max, total, top) VALUES (?1,0,0,0,?2)
                 ON CONFLICT(day) DO UPDATE SET top = ?2",
                params![today(), top],
            )?;
        }
        tx.commit()
    })
}

/// When `root` was last scanned completely, as far as the history knows.
pub fn last_scan(root: &str) -> Option<i64> {
    with(|c| {
        c.query_row(
            "SELECT MAX(ts) FROM disk_snap WHERE root = ?1",
            params![root],
            |r| r.get(0),
        )
    })
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Growth {
    pub path: String,
    pub size: f64,
    pub before: f64,
    pub delta: f64,
}

/// How each remembered folder of `root` changed between the first scan since `since` and the
/// latest. A folder that was under the snapshot size counts from 0, so a new big folder shows
/// as all growth (overstated by at most the snapshot minimum).
pub fn growth(root: &str, since: i64) -> (Vec<Growth>, Option<i64>, Option<i64>) {
    with(|c| {
        let latest: Option<i64> = c.query_row(
            "SELECT MAX(ts) FROM disk_snap WHERE root = ?1",
            params![root],
            |r| r.get(0),
        )?;
        // Scans closer together than this say more about what each could read than about
        // growth, so the baseline must be at least this much older than the latest.
        let first: Option<i64> = match latest {
            Some(l) => c.query_row(
                "SELECT MIN(ts) FROM disk_snap WHERE root = ?1 AND ts >= ?2 AND ts <= ?3",
                params![root, since, l - MIN_APART],
                |r| r.get(0),
            )?,
            None => None,
        };
        let (Some(a), Some(b)) = (first, latest) else {
            return Ok((Vec::new(), first, latest));
        };
        if a == b {
            return Ok((Vec::new(), first, latest));
        }
        let load = |ts: i64| -> rusqlite::Result<HashMap<String, f64>> {
            let mut st =
                c.prepare_cached("SELECT path, size FROM disk_snap WHERE ts = ?1 AND root = ?2")?;
            let rows = st.query_map(params![ts, root], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        };
        let (old, new) = (load(a)?, load(b)?);
        let mut out: Vec<Growth> = new
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
            .collect();
        for (p, s) in &old {
            if !new.contains_key(p) {
                out.push(Growth {
                    path: p.clone(),
                    size: 0.0,
                    before: *s,
                    delta: -s,
                });
            }
        }
        out.retain(|g| g.delta.abs() >= 50e6);
        out.sort_by(|x, y| y.delta.total_cmp(&x.delta));
        Ok((out, first, latest))
    })
}

fn below(p: &str, d: &str) -> bool {
    d.len() > p.len() && d.starts_with(p) && (p == "/" || d.as_bytes()[p.len()] == b'/')
}

/// Where growth landed: drops a folder when most of its growth is in one folder below it, so
/// the list names ~/.colima/_lima/_disks rather than /Users, ~ and ~/.colima as well; and drops
/// a folder whose growth the listed folders below it already account for (within 30%).
pub fn landed(all: &[Growth], root: &str) -> Vec<Growth> {
    let mut cand: Vec<&Growth> = all
        .iter()
        .filter(|g| g.path != root && g.delta.abs() >= 100e6)
        .filter(|g| {
            !all.iter().any(|d| {
                below(&g.path, &d.path)
                    && d.delta.signum() == g.delta.signum()
                    && d.delta.abs() >= 0.6 * g.delta.abs()
            })
        })
        .collect();
    // Deepest first, so a folder is judged against what is already kept below it.
    cand.sort_by_key(|g| std::cmp::Reverse(g.path.matches('/').count()));
    let mut keep: Vec<&Growth> = Vec::new();
    for g in cand {
        let covered: f64 = keep
            .iter()
            .filter(|d| {
                below(&g.path, &d.path)
                    && !keep
                        .iter()
                        .any(|k| below(&g.path, &k.path) && below(&k.path, &d.path))
            })
            .map(|d| d.delta)
            .sum();
        if covered != 0.0 && (g.delta - covered).abs() < (0.3 * g.delta.abs()).max(100e6) {
            continue;
        }
        keep.push(g);
    }
    let mut out: Vec<Growth> = keep.into_iter().cloned().collect();
    out.sort_by(|x, y| y.delta.total_cmp(&x.delta));
    out
}

/// What the listed folders add up to, counting a folder inside another listed one only once.
pub fn landed_total(landed: &[Growth]) -> f64 {
    landed
        .iter()
        .filter(|g| !landed.iter().any(|a| below(&a.path, &g.path)))
        .map(|g| g.delta)
        .sum()
}

/// One folder's size at each scan since `from`.
pub fn folder_history(path: &str, from: i64) -> Vec<(i64, f64)> {
    with(|c| {
        let mut st =
            c.prepare("SELECT ts, size FROM disk_snap WHERE path = ?1 AND ts >= ?2 ORDER BY ts")?;
        let rows = st.query_map(params![path, from], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    })
}

// ---- a day at a time, kept for a year ----

/// One local day of performance, rolled up from its minutes: averages and peaks as the minutes
/// have them, the day's totals (bytes moved, watt-hours drawn), its incidents and crashes, and
/// its busiest apps (averaged over the minutes they ran).
#[derive(Serialize, Clone, Debug, Default)]
pub struct PerfDay {
    pub day: String,
    /// Local midnight, unix seconds.
    pub ts: i64,
    /// Minutes recorded that day.
    pub minutes: i64,
    pub cpu: f64,
    pub cpu_max: f64,
    pub cpu_p: Option<f64>,
    pub cpu_e: Option<f64>,
    pub gpu: Option<f64>,
    pub gpu_max: Option<f64>,
    pub mem: f64,
    pub mem_max: f64,
    pub mem_total: f64,
    pub swap_max: f64,
    pub pressure: i64,
    pub thermal: i64,
    pub disk_r: f64,
    pub disk_w: f64,
    pub disk_max: f64,
    pub net_in: f64,
    pub net_out: f64,
    pub net_in_max: f64,
    pub net_out_max: f64,
    pub power: Option<f64>,
    pub power_max: Option<f64>,
    /// Bytes read, written, downloaded and uploaded that day; watt-hours drawn.
    pub read: f64,
    pub written: f64,
    pub down: f64,
    pub up: f64,
    pub energy: Option<f64>,
    pub incidents: i64,
    pub crashes: i64,
    pub apps: DayApps,
}

/// A day's busiest apps by each measure.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DayApps {
    pub cpu: Vec<AppUse>,
    pub mem: Vec<AppUse>,
    pub energy: Vec<AppUse>,
    pub disk: Vec<AppUse>,
}

/// A local day's first and last second, and its name (YYYY-MM-DD).
fn day_span(d: chrono::NaiveDate) -> Option<(i64, i64, String)> {
    // Where the clocks jump forward at midnight that midnight doesn't exist: the day starts at
    // the first hour that does.
    let start_of = |d: chrono::NaiveDate| {
        (0..3).find_map(|h| {
            d.and_hms_opt(h, 0, 0)?
                .and_local_timezone(chrono::Local)
                .earliest()
                .map(|t| t.timestamp())
        })
    };
    Some((
        start_of(d)?,
        start_of(d.succ_opt()?)?,
        d.format("%Y-%m-%d").to_string(),
    ))
}

/// Rolls one day up from its minutes into perf_daily; a day with none is left alone.
fn roll_up(d: chrono::NaiveDate) {
    let Some((from, to, day)) = day_span(d) else {
        return;
    };
    // Never a row from fewer minutes than it has: once the start of a day has been pruned (a
    // history of one day), what is left of it would shrink the day it already recorded.
    let (now_n, had): (i64, Option<i64>) = with(|c| {
        Ok((
            c.query_row(
                "SELECT COUNT(*) FROM perf WHERE ts >= ?1 AND ts < ?2",
                params![from, to],
                |r| r.get(0),
            )?,
            c.query_row(
                "SELECT minutes FROM perf_daily WHERE day = ?1",
                params![day],
                |r| r.get(0),
            )
            .optional()?,
        ))
    });
    if now_n == 0 || had.is_some_and(|h| now_n < h) {
        return;
    }
    let apps = DayApps {
        cpu: day_apps(from, to - 1, By::Cpu, 8),
        mem: day_apps(from, to - 1, By::Mem, 8),
        energy: day_apps(from, to - 1, By::Energy, 8),
        disk: day_apps(from, to - 1, By::Disk, 8),
    };
    let apps = serde_json::to_string(&apps).unwrap_or_default();
    with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO perf_daily (day, ts, minutes, cpu, cpu_max, cpu_p, cpu_e,
               gpu, gpu_max, mem, mem_max, mem_total, swap_max, pressure, thermal, disk_r, disk_w,
               disk_max, net_in, net_out, net_in_max, net_out_max, power, power_max, read,
               written, down, up, energy, incidents, crashes, apps)
             SELECT ?3, ?1, COUNT(*), AVG(cpu), MAX(cpu_max), AVG(cpu_p), AVG(cpu_e), AVG(gpu),
                    MAX(gpu_max), AVG(mem), MAX(mem_max), MAX(mem_total), MAX(swap_max),
                    MAX(pressure), MAX(thermal), AVG(disk_r), AVG(disk_w), MAX(disk_max),
                    AVG(net_in), AVG(net_out), MAX(COALESCE(net_in_max, net_in)),
                    MAX(COALESCE(net_out_max, net_out)), AVG(power), MAX(power_max),
                    SUM(disk_r) * 60, SUM(disk_w) * 60, SUM(net_in) * 60, SUM(net_out) * 60,
                    SUM(power) / 60.0,
                    (SELECT COUNT(*) FROM incidents WHERE started >= ?1 AND started < ?2),
                    (SELECT COUNT(*) FROM crashes WHERE ts >= ?1 AND ts < ?2), ?4
             FROM perf WHERE ts >= ?1 AND ts < ?2 HAVING COUNT(*) > 0",
            params![from, to, day, apps],
        )?;
        Ok(())
    })
}

/// Rolls up today and yesterday, and any day still in the minutes that has no row yet. Run as
/// the minutes come in (every ten), at the recorder's start and before minutes are pruned.
pub fn roll_up_recent() {
    let have: std::collections::HashSet<String> = with(|c| {
        let mut st = c.prepare("SELECT day FROM perf_daily")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect()
    });
    let days: Vec<String> = with(|c| {
        let mut st =
            c.prepare("SELECT DISTINCT date(ts, 'unixepoch', 'localtime') FROM perf ORDER BY 1")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect()
    });
    let today = chrono::Local::now().date_naive();
    for day in days {
        let Ok(d) = chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d") else {
            continue;
        };
        if !have.contains(&day) || d >= today - chrono::Duration::days(1) {
            roll_up(d);
        }
    }
}

/// How many days perf_daily holds, and the first one's local midnight.
pub fn daily_extent() -> (i64, Option<i64>) {
    with(|c| {
        c.query_row("SELECT COUNT(*), MIN(ts) FROM perf_daily", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
    })
}

/// The days since `from` (unix seconds), oldest first.
pub fn perf_days_since(from: i64) -> Vec<PerfDay> {
    with(|c| {
        let mut st = c.prepare(
            "SELECT day, ts, minutes, cpu, cpu_max, cpu_p, cpu_e, gpu, gpu_max, mem, mem_max,
                    mem_total, swap_max, pressure, thermal, disk_r, disk_w, disk_max, net_in,
                    net_out, net_in_max, net_out_max, power, power_max, read, written, down, up,
                    energy, incidents, crashes, apps
             FROM perf_daily WHERE ts >= ?1 ORDER BY ts",
        )?;
        let rows = st.query_map(params![from], |r| {
            let apps: Option<String> = r.get(31)?;
            Ok(PerfDay {
                day: r.get(0)?,
                ts: r.get(1)?,
                minutes: r.get(2)?,
                cpu: r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                cpu_max: r.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                cpu_p: r.get(5)?,
                cpu_e: r.get(6)?,
                gpu: r.get(7)?,
                gpu_max: r.get(8)?,
                mem: r.get::<_, Option<f64>>(9)?.unwrap_or(0.0),
                mem_max: r.get::<_, Option<f64>>(10)?.unwrap_or(0.0),
                mem_total: r.get::<_, Option<f64>>(11)?.unwrap_or(0.0),
                swap_max: r.get::<_, Option<f64>>(12)?.unwrap_or(0.0),
                pressure: r.get::<_, Option<i64>>(13)?.unwrap_or(0),
                thermal: r.get::<_, Option<i64>>(14)?.unwrap_or(0),
                disk_r: r.get::<_, Option<f64>>(15)?.unwrap_or(0.0),
                disk_w: r.get::<_, Option<f64>>(16)?.unwrap_or(0.0),
                disk_max: r.get::<_, Option<f64>>(17)?.unwrap_or(0.0),
                net_in: r.get::<_, Option<f64>>(18)?.unwrap_or(0.0),
                net_out: r.get::<_, Option<f64>>(19)?.unwrap_or(0.0),
                net_in_max: r.get::<_, Option<f64>>(20)?.unwrap_or(0.0),
                net_out_max: r.get::<_, Option<f64>>(21)?.unwrap_or(0.0),
                power: r.get(22)?,
                power_max: r.get(23)?,
                read: r.get::<_, Option<f64>>(24)?.unwrap_or(0.0),
                written: r.get::<_, Option<f64>>(25)?.unwrap_or(0.0),
                down: r.get::<_, Option<f64>>(26)?.unwrap_or(0.0),
                up: r.get::<_, Option<f64>>(27)?.unwrap_or(0.0),
                energy: r.get(28)?,
                incidents: r.get::<_, Option<i64>>(29)?.unwrap_or(0),
                crashes: r.get::<_, Option<i64>>(30)?.unwrap_or(0),
                apps: apps
                    .and_then(|a| serde_json::from_str(&a).ok())
                    .unwrap_or_default(),
            })
        })?;
        rows.collect()
    })
}

/// A measure's average by weekday (0 = Sunday) and hour, local time, since `from`. `metric` is
/// one of cpu, gpu, mem, disk, down, up, power.
pub fn by_hour(metric: &str, from: i64) -> Vec<(u32, u32, f64)> {
    let expr = match metric {
        "gpu" => "AVG(gpu)",
        "mem" => "AVG(mem)",
        "disk" => "AVG(disk_r + disk_w)",
        "down" => "AVG(net_in)",
        "up" => "AVG(net_out)",
        "power" => "AVG(power)",
        _ => "AVG(cpu)",
    };
    with(|c| {
        let mut st = c.prepare(&format!(
            "SELECT CAST(strftime('%w', ts, 'unixepoch', 'localtime') AS INTEGER),
                    CAST(strftime('%H', ts, 'unixepoch', 'localtime') AS INTEGER), {expr}
             FROM perf WHERE ts >= ?1 GROUP BY 1, 2 HAVING {expr} IS NOT NULL"
        ))?;
        let rows = st.query_map(params![from], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect()
    })
}

// ---- upkeep ----

/// Forgets what is older than the history setting allows; daily totals after a year. Days are
/// rolled up first, so a day's row outlives its minutes.
pub fn prune(days: u32) {
    roll_up_recent();
    let cut = now() - days as i64 * 86_400;
    let year = (chrono::Local::now() - chrono::Duration::days(366))
        .format("%Y-%m-%d")
        .to_string();
    with(|c| {
        c.execute("DELETE FROM perf WHERE ts < ?1", params![cut])?;
        c.execute("DELETE FROM procs WHERE ts < ?1", params![cut])?;
        c.execute("DELETE FROM incidents WHERE started < ?1", params![cut])?;
        c.execute("DELETE FROM sleeps WHERE started < ?1", params![cut])?;
        c.execute("DELETE FROM crashes WHERE ts < ?1", params![cut])?;
        c.execute("DELETE FROM disk_used WHERE ts < ?1", params![cut])?;
        c.execute("DELETE FROM disk_snap WHERE ts < ?1", params![cut])?;
        c.execute("DELETE FROM disk_daily WHERE day < ?1", params![year])?;
        c.execute("DELETE FROM perf_daily WHERE day < ?1", params![year])?;
        Ok(())
    })
}

/// The history file's size on disk, WAL included.
pub fn bytes() -> u64 {
    let dir = crate::cache::dir();
    ["history.sqlite", "history.sqlite-wal"]
        .iter()
        .filter_map(|f| std::fs::metadata(dir.join(f)).ok())
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(path: &str, delta: f64) -> Growth {
        Growth {
            path: path.into(),
            size: 0.0,
            before: 0.0,
            delta,
        }
    }

    #[test]
    fn growth_lands_where_it_is_concentrated() {
        let all = vec![
            g("/Users", 9.4e9),
            g("/Users/me", 9.4e9),
            g("/Users/me/.colima", 6.1e9),
            g("/Users/me/.colima/_lima", 6.1e9),
            g("/Users/me/Library", 2.4e9),
            g("/Users/me/Library/Caches", 2.2e9),
            g("/Users/me/Downloads", 0.9e9),
        ];
        let l: Vec<String> = landed(&all, "/").into_iter().map(|g| g.path).collect();
        assert_eq!(
            l,
            vec![
                "/Users/me/.colima/_lima",
                "/Users/me/Library/Caches",
                "/Users/me/Downloads"
            ]
        );
    }

    #[test]
    fn a_folder_explained_by_folders_inside_it_is_not_listed_again() {
        let all = vec![
            g("/Users/me/Library", 9e9),
            g("/Users/me/Library/A", 4e9),
            g("/Users/me/Library/B", 4.5e9),
        ];
        let l: Vec<String> = landed(&all, "/").into_iter().map(|g| g.path).collect();
        assert_eq!(l, vec!["/Users/me/Library/B", "/Users/me/Library/A"]);
        // Growth that the folders inside don't explain keeps the parent, counted once.
        let all = vec![g("/L", 20e9), g("/L/A", 4e9), g("/L/B", 4.5e9)];
        let l = landed(&all, "/");
        assert_eq!(l.len(), 3);
        assert_eq!(landed_total(&l), 20e9);
    }

    #[test]
    fn snapshot_keeps_big_folders_only() {
        let leaf = |n: &str, s: u64| Node {
            name: n.into(),
            size: s,
            ..Default::default()
        };
        let tree = Node {
            name: "root".into(),
            size: 500_000_000,
            kids: vec![leaf("big", 400_000_000), leaf("small", 1_000_000)],
            ..Default::default()
        };
        let mut rows = Vec::new();
        snap_rows(&tree, "/x", 0, &mut rows);
        assert_eq!(
            rows,
            vec![
                ("/x".to_string(), 500_000_000),
                ("/x/big".to_string(), 400_000_000)
            ]
        );
    }

    #[test]
    fn a_day_spans_its_local_day() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 3, 29).unwrap();
        let (from, to, name) = day_span(d).unwrap();
        assert_eq!(name, "2026-03-29");
        // 23 to 25 hours: a day the clocks change on is still one day.
        assert!((23 * 3600..=25 * 3600).contains(&(to - from)));
        let next = day_span(d.succ_opt().unwrap()).unwrap();
        assert_eq!(next.0, to);
    }
}
