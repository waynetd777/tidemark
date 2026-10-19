// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! The performance recorder. A thread samples the Mac every few seconds (2 s while a window is
//! open, 5 s otherwise), keeps the last 15 minutes in memory for the live charts, writes one row
//! a minute (averages and maxima) with the busiest apps of that minute to the history, and opens
//! an incident when something crosses a line: CPU above 90% for 30 s, memory pressure critical,
//! or swap growing by 1 GB within 10 minutes. Each incident keeps what the busiest apps were
//! doing at its peak.
//!
//! Processes are grouped by the app they belong to (the outermost .app in the executable's
//! path), so Chrome's dozens of helpers count as Google Chrome. CPU is a share of the whole
//! machine, not of one core.

use crate::energy::{Battery, Meter};
use crate::history::{self, AppUse, By, Minute};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
// Windows has Unix sockets too (AF_UNIX, since Windows 10 1803); std has them only on Unix.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};

/// How much of the recent past the live charts get.
const LIVE_SECS: i64 = 15 * 60;
/// Apps kept per minute, by CPU and by memory (the union is stored).
const APPS_PER_MINUTE: usize = 8;
/// The apps each live list shows (by CPU, by memory, by energy).
const LIVE_APPS: usize = 8;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Sample {
    /// Unix milliseconds.
    pub t: i64,
    pub cpu: f64,
    pub mem: f64,
    pub swap: f64,
    pub pressure: i64,
    pub disk_r: f64,
    pub disk_w: f64,
    pub net_in: f64,
    pub net_out: f64,
    pub thermal: i64,
    /// What the Mac draws, watts; None without a battery controller.
    #[serde(default)]
    pub power: Option<f64>,
    /// Battery charge, 0-100; None without a battery.
    #[serde(default)]
    pub battery: Option<f64>,
    /// GPU utilisation, 0-100; None if no GPU reports it.
    #[serde(default)]
    pub gpu: Option<f64>,
    /// What memory in use is made of.
    #[serde(default)]
    pub mem_parts: Option<crate::system::MemParts>,
    /// On a power adapter.
    #[serde(default)]
    pub on_ac: bool,
    /// The performance cores' and the efficiency cores' average use, 0-100; None on a Mac that
    /// doesn't say which is which.
    #[serde(default)]
    pub cpu_p: Option<f64>,
    #[serde(default)]
    pub cpu_e: Option<f64>,
}

#[derive(Default, Serialize, Deserialize)]
struct Live {
    samples: VecDeque<Sample>,
    /// Every app at the latest sample, busiest CPU first.
    apps: Vec<AppUse>,
    processes: usize,
    mem_total: f64,
    /// The battery as last read.
    #[serde(default)]
    battery: Option<Battery>,
    /// Recording is paused until then (unix seconds).
    paused_until: Option<i64>,
    /// Incidents going on now: kind -> (id, started, peak).
    open: HashMap<String, (i64, i64, f64)>,
    /// Each core's use at the latest sample, 0-100, in the order of `core_kinds`.
    #[serde(default)]
    cores: Vec<f64>,
}

/// Each core's kind, 'P' or 'E', read once; empty where the Mac doesn't say.
fn core_kinds() -> &'static [char] {
    static KINDS: std::sync::OnceLock<Vec<char>> = std::sync::OnceLock::new();
    KINDS.get_or_init(crate::system::core_kinds)
}

/// The average use of the cores of one kind; None if the kinds don't match the cores.
fn kind_avg(cores: &[f64], kind: char) -> Option<f64> {
    let kinds = core_kinds();
    if kinds.len() != cores.len() {
        return None;
    }
    let v: Vec<f64> = cores
        .iter()
        .zip(kinds)
        .filter(|(_, k)| **k == kind)
        .map(|(c, _)| *c)
        .collect();
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);
/// A window is on screen: sample faster so the live charts move.
pub static FAST: AtomicBool = AtomicBool::new(true);

fn live<T>(f: impl FnOnce(&mut Live) -> T) -> T {
    let mut g = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Live::default))
}

/// Names macOS gives its own helper processes, as people would say them.
const FRIENDLY: [(&str, &str); 6] = [
    (
        "com.apple.Virtualization.VirtualMachine",
        "Virtual machine (Virtualization)",
    ),
    ("com.apple.WebKit.WebContent", "Web content (WebKit)"),
    ("com.apple.WebKit.GPU", "Web graphics (WebKit)"),
    ("com.apple.WebKit.Networking", "Web networking (WebKit)"),
    ("mds_stores", "Spotlight (mds_stores)"),
    ("mdworker_shared", "Spotlight (mdworker)"),
];

/// The app a process belongs to: the outermost bundle in its path, else its own name. On
/// Windows, its program's description (system_win.rs).
#[cfg(windows)]
fn app_of(exe: Option<&std::path::Path>, name: &str) -> String {
    crate::system_win::app_name(exe, name)
}

#[cfg(not(windows))]
fn app_of(exe: Option<&std::path::Path>, name: &str) -> String {
    if let Some((_, f)) = FRIENDLY.iter().find(|(n, _)| *n == name) {
        return f.to_string();
    }
    if let Some(p) = exe.and_then(|p| p.to_str()) {
        if let Some(i) = p.find(".app/") {
            let start = p[..i].rfind('/').map_or(0, |j| j + 1);
            return p[start..i].to_string();
        }
    }
    name.to_string()
}

/// One condition watched for incidents.
struct Watch {
    kind: &'static str,
    /// How long the condition must hold before it is an incident, and must be off before it ends.
    on_for: i64,
    off_for: i64,
    /// Sort the snapshot by memory rather than CPU.
    by_mem: bool,
    since_on: Option<i64>,
    since_off: Option<i64>,
}

impl Watch {
    fn new(kind: &'static str, on_for: i64, off_for: i64, by_mem: bool) -> Self {
        Watch {
            kind,
            on_for,
            off_for,
            by_mem,
            since_on: None,
            since_off: None,
        }
    }

    /// `on`: the condition holds; `off`: it has clearly stopped (a lower line, so it doesn't
    /// flicker). `value` is what the peak is measured in.
    fn step(&mut self, now: i64, on: bool, off: bool, value: f64, apps: &[AppUse]) {
        let open = live(|l| l.open.get(self.kind).copied());
        let snapshot = || {
            let mut a = apps.to_vec();
            if self.by_mem {
                a.sort_by(|x, y| y.mem.total_cmp(&x.mem));
            }
            a.truncate(12);
            a
        };
        match open {
            None => {
                if on {
                    let since = *self.since_on.get_or_insert(now);
                    if now - since >= self.on_for {
                        let id = history::open_incident(self.kind, since, value, &snapshot());
                        live(|l| l.open.insert(self.kind.to_string(), (id, since, value)));
                        self.since_off = None;
                    }
                } else {
                    self.since_on = None;
                }
            }
            Some((id, started, peak)) => {
                if value > peak {
                    history::update_incident(id, value, now, Some(&snapshot()));
                    live(|l| l.open.insert(self.kind.to_string(), (id, started, value)));
                }
                if off {
                    let since = *self.since_off.get_or_insert(now);
                    if now - since >= self.off_for {
                        history::close_incident(id, since);
                        live(|l| l.open.remove(self.kind));
                        self.since_on = None;
                    }
                } else {
                    self.since_off = None;
                }
            }
        }
    }
}

/// Ends every incident going on at `at` (the last reading before a pause or a gap), and starts
/// every watch afresh: nothing carries across time that wasn't recorded.
fn end_all(at: i64, watches: &mut [Watch]) {
    for (id, _, _) in live(|l| l.open.drain().map(|(_, v)| v).collect::<Vec<_>>()) {
        history::close_incident(id, at);
    }
    for w in watches.iter_mut() {
        w.since_on = None;
        w.since_off = None;
    }
}

/// The minute being gathered.
#[derive(Default)]
struct Acc {
    minute: i64,
    n: f64,
    m: Minute,
    /// Each app's sums over the minute, and its busiest reading (the *_max fields). A reading of
    /// an app covers the time since the last pass over every process: 2 s while a window is
    /// open, 15 s otherwise, so a peak is the busiest 2 or 15 seconds.
    apps: HashMap<String, AppUse>,
    /// Power readings in the minute (not every sample has one) and their sum.
    power_n: f64,
    gpu_n: f64,
    parts_n: f64,
    kinds_n: f64,
}

impl Acc {
    fn add(&mut self, s: &Sample, mem_total: f64, apps: &[AppUse]) {
        let m = &mut self.m;
        m.cpu += s.cpu;
        m.cpu_max = m.cpu_max.max(s.cpu);
        m.mem += s.mem;
        m.mem_max = m.mem_max.max(s.mem);
        m.mem_total = mem_total;
        m.swap += s.swap;
        m.swap_max = m.swap_max.max(s.swap);
        m.pressure = m.pressure.max(s.pressure);
        m.disk_r += s.disk_r;
        m.disk_w += s.disk_w;
        m.disk_max = m.disk_max.max(s.disk_r + s.disk_w);
        m.net_in += s.net_in;
        m.net_out += s.net_out;
        m.net_max = m.net_max.max(s.net_in + s.net_out);
        m.net_in_max = m.net_in_max.max(s.net_in);
        m.net_out_max = m.net_out_max.max(s.net_out);
        m.thermal = m.thermal.max(s.thermal);
        if let Some(w) = s.power {
            m.power = Some(m.power.unwrap_or(0.0) + w);
            m.power_max = Some(m.power_max.unwrap_or(0.0).max(w));
            self.power_n += 1.0;
        }
        if s.battery.is_some() {
            m.battery = s.battery;
        }
        m.on_ac |= s.on_ac;
        if let Some(g) = s.gpu {
            m.gpu = Some(m.gpu.unwrap_or(0.0) + g);
            m.gpu_max = Some(m.gpu_max.unwrap_or(0.0).max(g));
            self.gpu_n += 1.0;
        }
        if let (Some(p), Some(e)) = (s.cpu_p, s.cpu_e) {
            m.cpu_p = Some(m.cpu_p.unwrap_or(0.0) + p);
            m.cpu_e = Some(m.cpu_e.unwrap_or(0.0) + e);
            self.kinds_n += 1.0;
        }
        if let Some(p) = s.mem_parts {
            m.mem_app = Some(m.mem_app.unwrap_or(0.0) + p.app);
            m.mem_wired = Some(m.mem_wired.unwrap_or(0.0) + p.wired);
            m.mem_comp = Some(m.mem_comp.unwrap_or(0.0) + p.compressed);
            self.parts_n += 1.0;
        }
        for a in apps {
            let e = self.apps.entry(a.app.clone()).or_default();
            e.cpu += a.cpu;
            e.mem += a.mem;
            e.energy += a.energy;
            e.disk += a.disk;
            e.cpu_max = e.cpu_max.max(a.cpu);
            e.mem_max = e.mem_max.max(a.mem);
            e.energy_max = e.energy_max.max(a.energy);
            e.disk_max = e.disk_max.max(a.disk);
        }
        self.n += 1.0;
    }

    fn flush(&mut self) {
        if self.n == 0.0 {
            return;
        }
        let n = self.n;
        let mut m = std::mem::take(&mut self.m);
        m.ts = self.minute * 60;
        for v in [
            &mut m.cpu,
            &mut m.mem,
            &mut m.swap,
            &mut m.disk_r,
            &mut m.disk_w,
            &mut m.net_in,
            &mut m.net_out,
        ] {
            *v /= n;
        }
        if let Some(p) = m.power.as_mut() {
            *p /= self.power_n.max(1.0);
        }
        self.power_n = 0.0;
        if let Some(g) = m.gpu.as_mut() {
            *g /= self.gpu_n.max(1.0);
        }
        for v in [&mut m.mem_app, &mut m.mem_wired, &mut m.mem_comp]
            .into_iter()
            .flatten()
        {
            *v /= self.parts_n.max(1.0);
        }
        for v in [&mut m.cpu_p, &mut m.cpu_e].into_iter().flatten() {
            *v /= self.kinds_n.max(1.0);
        }
        self.gpu_n = 0.0;
        self.parts_n = 0.0;
        self.kinds_n = 0.0;
        let mut apps: Vec<AppUse> = self
            .apps
            .drain()
            .map(|(app, a)| AppUse {
                app,
                cpu: a.cpu / n,
                mem: a.mem / n,
                energy: a.energy / n,
                disk: a.disk / n,
                ..a
            })
            .collect();
        // The busiest by each measure, on average and at a moment (a short spike's app too).
        let mut keep: Vec<AppUse> = Vec::new();
        let by: [fn(&AppUse) -> f64; 8] = [
            |a| a.cpu,
            |a| a.cpu_max,
            |a| a.mem,
            |a| a.mem_max,
            |a| a.energy,
            |a| a.energy_max,
            |a| a.disk,
            |a| a.disk_max,
        ];
        for f in by {
            apps.sort_by(|a, b| f(b).total_cmp(&f(a)));
            for a in apps.iter().take(APPS_PER_MINUTE) {
                if !keep.iter().any(|k| k.app == a.app) {
                    keep.push(a.clone());
                }
            }
        }
        history::add_minute(&m, &keep);
        self.n = 0.0;
    }
}

// ---- one recorder at a time ----
//
// The app and the background helper (`Tidemark --record`, a launch agent: see recorder.rs) can
// both be running. Whichever holds recorder.lock records; the app, when it doesn't, follows the
// helper's live readings down a Unix socket (live.sock), which costs no disk writes. The lock goes
// with the process, so when the recorder stops, the other takes over within seconds.

/// This process holds the lock and is recording.
static RECORDING: AtomicBool = AtomicBool::new(false);

pub fn recording() -> bool {
    RECORDING.load(Ordering::SeqCst)
}

fn try_lock() -> bool {
    let dir = crate::cache::dir();
    let _ = std::fs::create_dir_all(&dir);
    let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("recorder.lock"))
    else {
        return false;
    };
    if f.try_lock().is_err() {
        return false;
    }
    // Held for the rest of the process's life.
    std::mem::forget(f);
    true
}

/// Where the recorder serves its live readings to the app: a Unix socket, so nothing is written
/// to disk however often they change.
fn socket_path() -> std::path::PathBuf {
    crate::cache::dir().join("live.sock")
}
/// When recording is paused until, as the pause control left it (unix seconds; absent: not).
fn pause_path() -> std::path::PathBuf {
    crate::cache::dir().join("paused-until")
}

fn read_pause() -> Option<i64> {
    std::fs::read_to_string(pause_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .filter(|&t| t > history::now())
}

/// This process is the background helper: no window, no menu bar, nothing to follow for.
pub static HEADLESS: AtomicBool = AtomicBool::new(false);

/// A process following the recorder: the stream its readings go down, and whether it has a window
/// on screen, which it says down the same stream ("1" or "0", a line each time it changes).
struct Follower {
    out: UnixStream,
    watched: Arc<AtomicBool>,
}

static FOLLOWERS: Mutex<Vec<Follower>> = Mutex::new(Vec::new());

/// A write to a follower that has gone fails rather than raising SIGPIPE (which Windows doesn't
/// have).
fn no_sigpipe(s: &UnixStream) {
    #[cfg(windows)]
    let _ = s;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let on: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                s.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_NOSIGPIPE,
                &on as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
    }
}

/// Opens live.sock for followers, once this process holds the lock. Each follower gets a thread
/// that reads what it says (whether a window is on screen); readings go out from `share`.
fn serve() {
    let dir = crate::cache::dir();
    // Files earlier versions shared readings through.
    for old in ["live.json", "live.tmp", "following", "watching"] {
        let _ = std::fs::remove_file(dir.join(old));
    }
    let path = socket_path();
    // One left by a recorder that died: the lock says nobody is serving it.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("live readings: {e}");
            return;
        }
    };
    let _ = std::thread::Builder::new()
        .name("live".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(out) = stream else { continue };
                let Ok(input) = out.try_clone() else { continue };
                no_sigpipe(&out);
                // A follower that stops reading is dropped rather than stalling the recorder.
                let _ = out.set_write_timeout(Some(Duration::from_secs(1)));
                let watched = Arc::new(AtomicBool::new(false));
                let said = watched.clone();
                let _ = std::thread::Builder::new()
                    .name("live-follower".into())
                    .spawn(move || {
                        for line in BufReader::new(input).lines() {
                            let Ok(line) = line else { break };
                            said.store(line.trim() == "1", Ordering::Relaxed);
                        }
                        // Gone: `share` drops it at the next failed write.
                        said.store(false, Ordering::Relaxed);
                    });
                FOLLOWERS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(Follower { out, watched });
            }
        });
}

/// A follower has a window on screen: sample faster so its live charts move.
fn followers_watching() -> bool {
    FOLLOWERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|f| f.watched.load(Ordering::Relaxed))
}

/// Sends the recorder's state to each follower, a line of JSON: in full to one with a window on
/// screen (the live charts), else just the latest reading and the busiest apps, which is all the
/// menu bar's status needs. Nothing to do, and nothing serialised, when nobody follows.
fn share() {
    let mut followers = FOLLOWERS.lock().unwrap_or_else(|e| e.into_inner());
    if followers.is_empty() {
        return;
    }
    let line = |l: &Live| {
        let mut b = serde_json::to_vec(l).unwrap_or_default();
        b.push(b'\n');
        b
    };
    let (mut full, mut brief): (Option<Vec<u8>>, Option<Vec<u8>>) = (None, None);
    followers.retain_mut(|f| {
        let bytes = if f.watched.load(Ordering::Relaxed) {
            full.get_or_insert_with(|| live(|l| line(l)))
        } else {
            brief.get_or_insert_with(|| {
                live(|l| {
                    line(&Live {
                        samples: l.samples.back().cloned().into_iter().collect(),
                        apps: busiest(&l.apps),
                        processes: l.processes,
                        mem_total: l.mem_total,
                        battery: l.battery.clone(),
                        paused_until: l.paused_until,
                        open: l.open.clone(),
                        cores: l.cores.clone(),
                    })
                })
            })
        };
        // A failed or timed-out write may have sent part of a line: the stream is done with.
        f.out.write_all(bytes).is_ok()
    });
}

/// The busiest apps by CPU, by memory, by energy and by disk I/O, each list as long as
/// `perf_live` shows, so the follower can sort them each way (Now's "Using the most" cards, the
/// menu bar's list).
/// In CPU order, as `Live::apps` is.
fn busiest(apps: &[AppUse]) -> Vec<AppUse> {
    let mut keep: Vec<&AppUse> = Vec::new();
    let by: [fn(&AppUse) -> f64; 4] = [|a| a.cpu, |a| a.mem, |a| a.energy, |a| a.disk];
    for f in by {
        let mut sorted: Vec<&AppUse> = apps.iter().collect();
        sorted.sort_by(|a, b| f(b).total_cmp(&f(a)));
        for a in sorted.into_iter().take(LIVE_APPS) {
            if !keep.iter().any(|k| k.app == a.app) {
                keep.push(a);
            }
        }
    }
    keep.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    keep.into_iter().cloned().collect()
}

/// The app's connection to the recorder in the other process.
struct Link {
    input: BufReader<UnixStream>,
    out: UnixStream,
    /// What it last told the recorder about its window.
    said: Option<bool>,
    /// Part of a line, kept across reads that time out.
    buf: Vec<u8>,
    /// The last reading: a recorder silent for 30 s is hung, and is let go.
    heard: Instant,
}

impl Link {
    fn open() -> Option<Link> {
        let out = UnixStream::connect(socket_path()).ok()?;
        no_sigpipe(&out);
        let _ = out.set_write_timeout(Some(Duration::from_secs(1)));
        Some(Link {
            input: BufReader::new(out.try_clone().ok()?),
            out,
            said: None,
            buf: Vec::new(),
            heard: Instant::now(),
        })
    }
}

/// Follows the recorder in the other process for about 2 s, then returns so the caller can take
/// over if it has stopped: says whether a window is on screen here, and keeps the latest reading.
/// Returns at once when the recorder goes. The helper, which shows nothing, only waits.
fn follow(link: &mut Option<Link>) {
    let wait = Duration::from_secs(2);
    if HEADLESS.load(Ordering::Relaxed) {
        std::thread::sleep(wait);
        return;
    }
    if link.is_none() {
        *link = Link::open();
    }
    let Some(l) = link.as_mut() else {
        std::thread::sleep(wait);
        return;
    };
    let fast = FAST.load(Ordering::Relaxed);
    if l.said != Some(fast) {
        if l.out.write_all(if fast { b"1\n" } else { b"0\n" }).is_err() {
            *link = None;
            return;
        }
        l.said = Some(fast);
    }
    let until = Instant::now() + wait;
    let mut latest: Option<Live> = None;
    let gone = loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break l.heard.elapsed() > Duration::from_secs(30);
        }
        let _ = l.input.get_ref().set_read_timeout(Some(left));
        match l.input.read_until(b'\n', &mut l.buf) {
            Ok(0) => break true,
            Ok(_) if l.buf.ends_with(b"\n") => {
                if let Ok(v) = serde_json::from_slice(&l.buf) {
                    latest = Some(v);
                }
                l.buf.clear();
                l.heard = Instant::now();
            }
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break l.heard.elapsed() > Duration::from_secs(30);
            }
            Err(_) => break true,
        }
    };
    if let Some(v) = latest {
        *LIVE.lock().unwrap_or_else(|e| e.into_inner()) = Some(v);
    }
    if gone {
        *link = None;
    }
}

/// Records if no other process is, else follows the one that does, taking over when it stops.
pub fn start() {
    std::thread::Builder::new()
        .name("monitor".into())
        .spawn(|| {
            // Screenshots follow no recorder: their live readings are the demo's live.json.
            if crate::scene::active() {
                let path = crate::cache::dir().join("live.json");
                match std::fs::read(&path).map(|b| serde_json::from_slice::<Live>(&b)) {
                    Ok(Ok(v)) => *LIVE.lock().unwrap_or_else(|e| e.into_inner()) = Some(v),
                    Ok(Err(e)) => eprintln!("{}: {e}", path.display()),
                    Err(e) => eprintln!("{}: {e}", path.display()),
                }
                return;
            }
            let mut link: Option<Link> = None;
            loop {
                if try_lock() {
                    RECORDING.store(true, Ordering::SeqCst);
                    link = None;
                    serve();
                    history::close_dangling();
                    // Any day still in the minutes without its daily row gets one (Trends' record).
                    history::roll_up_recent();
                    // What the previous recorder had open is closed now; forget its copy.
                    live(|l| l.open.clear());
                    run();
                }
                follow(&mut link);
            }
        })
        .expect("the monitor thread starts");
}

fn run() {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
    let mut sys = System::new();
    let mut nets = Networks::new_with_refreshed_list();
    let kind = ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_disk_usage()
        .with_exe(UpdateKind::OnlyIfNotSet);
    // The whole Mac (cheap) every sample; every process (the costly part) every sample while
    // someone is looking, else every 15 seconds.
    let refresh = |sys: &mut System, nets: &mut Networks| {
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        nets.refresh(true);
    };
    let refresh_procs = |sys: &mut System| {
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    };
    refresh(&mut sys, &mut nets);
    refresh_procs(&mut sys);
    let mut last_procs = Instant::now();
    // The last per-process pass: every app, and the disk's read and write rates it gave.
    let mut apps_then: Vec<AppUse> = Vec::new();
    let mut io_then = (0.0f64, 0.0f64);
    let mut gpu_then: Option<f64> = None;
    let mut last = Instant::now();
    // Instant stops while the Mac sleeps (CLOCK_UPTIME_RAW), so sleep shows only in wall-clock
    // time: gaps are measured with this, rates with `last`.
    let mut last_wall = history::now();
    // The end of the last sleep or pause: nothing before it is compared with what follows.
    let mut since_gap = last_wall;
    // A sleep or a pause left the last per-process pass's apps, disk rates and GPU behind: the
    // first sample after it takes a fresh pass, whatever the rate.
    let mut procs_stale = false;
    let mut meter = Meter::new();
    // The battery when the last sample was taken, for what a sleep cost.
    let mut battery_then: Option<f64> = None;
    let mut watches = [
        Watch::new("cpu", 30, 30, false),
        Watch::new("pressure", 5, 30, true),
        Watch::new("swap", 0, 0, true),
    ];
    // A minute the last recorder saved as it stopped (`stop`) is left as it is: gathered again
    // from here, this part of it would replace the whole minute's row. Recording starts with the
    // next minute.
    let mut saved_minute = {
        let m = history::now() / 60;
        history::has_minute(m * 60).then_some(m)
    };
    loop {
        // Faster while a window is on screen here, or in a follower.
        let every = if FAST.load(Ordering::Relaxed) || followers_watching() {
            2
        } else {
            5
        };
        std::thread::sleep(Duration::from_secs(every));
        // Held for the rest of this pass, so `stop` waits for it to finish and nothing is
        // recorded after it.
        let _step = STEP.lock().unwrap_or_else(|e| e.into_inner());
        if STOPPING.load(Ordering::SeqCst) {
            drop(_step);
            loop {
                std::thread::park();
            }
        }
        let now = history::now();
        let paused = read_pause();
        live(|l| l.paused_until = paused);
        if paused.is_some() {
            end_all(last_wall, &mut watches);
            share();
            // Keep the counters current so the first sample after the pause covers 2-5 s, not
            // the whole pause.
            refresh(&mut sys, &mut nets);
            refresh_procs(&mut sys);
            // The energy counters too, or the first reading after the pause would count the
            // whole pause's energy as a few seconds'.
            let pdt = last_procs.elapsed().as_secs_f64();
            for p in sys.processes().keys() {
                let _ = meter.watts(p.as_u32() as i32, pdt);
            }
            meter.keep_only(&sys.processes().keys().map(|p| p.as_u32() as i32).collect());
            last = Instant::now();
            last_procs = last;
            last_wall = now;
            since_gap = now;
            procs_stale = true;
            // The minute in hand is saved when the next one starts, not here: saved now, a
            // pause that ends within the same minute would start that minute again, and the
            // later part would replace the earlier.
            continue;
        }
        let dt = last.elapsed().as_secs_f64();
        last = Instant::now();
        let wall_dt = (now - last_wall) as f64;
        let before_gap = last_wall;
        last_wall = now;
        refresh(&mut sys, &mut nets);
        let procs_due =
            every == 2 || procs_stale || last_procs.elapsed() >= Duration::from_secs(15);
        // Asleep in between: the counters span the sleep, so this sample says nothing about
        // any moment. The gap stays a gap.
        if wall_dt.max(dt) > every as f64 * 3.0 + 5.0 {
            // The minute in hand is saved when the next one starts (see the pause above).
            since_gap = now;
            procs_stale = true;
            // An incident ends where the record stops; nothing building up carries across.
            end_all(before_gap, &mut watches);
            // Remember the gap, with the battery either side: what the sleep cost.
            let after = meter.battery().map(|b| b.percent);
            if wall_dt >= 120.0 {
                history::add_sleep(&history::Sleep {
                    started: now - wall_dt as i64,
                    ended: now,
                    battery_before: battery_then,
                    battery_after: after,
                });
            }
            battery_then = after;
            // The per-process counters span the gap too: start them afresh.
            refresh_procs(&mut sys);
            let pdt = last_procs.elapsed().as_secs_f64();
            last_procs = Instant::now();
            for p in sys.processes().keys() {
                let _ = meter.watts(p.as_u32() as i32, pdt);
            }
            continue;
        }

        if procs_due {
            procs_stale = false;
            refresh_procs(&mut sys);
            // Rates over the time since the last per-process pass, whatever the sample rate.
            let dt = last_procs.elapsed().as_secs_f64();
            last_procs = Instant::now();
            let mut by_app: HashMap<String, AppUse> = HashMap::new();
            let (mut rd, mut wr) = (0u64, 0u64);
            for p in sys.processes().values() {
                // Windows' System Idle Process (pid 0) is the time no process ran.
                if cfg!(windows) && p.pid().as_u32() == 0 {
                    continue;
                }
                let d = p.disk_usage();
                rd += d.read_bytes;
                wr += d.written_bytes;
                let name = app_of(p.exe(), &p.name().to_string_lossy());
                let e = by_app.entry(name.clone()).or_insert_with(|| AppUse {
                    app: name,
                    ..Default::default()
                });
                e.cpu += p.cpu_usage() as f64 / cores;
                e.mem += p.memory() as f64;
                e.energy += meter.watts(p.pid().as_u32() as i32, dt).unwrap_or(0.0);
                e.disk += (d.read_bytes + d.written_bytes) as f64 / dt;
            }
            meter.keep_only(&sys.processes().keys().map(|p| p.as_u32() as i32).collect());
            let mut apps: Vec<AppUse> = by_app.into_values().collect();
            apps.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
            apps_then = apps;
            io_then = (rd as f64 / dt, wr as f64 / dt);
        }
        let apps = apps_then.clone();
        // The GPU through ioreg costs a process each time: read with the apps (every sample
        // while someone is looking, else every 15 s), and carried between.
        if procs_due {
            gpu_then = crate::system::gpu_busy();
        }
        // Windows counts the disk itself; the processes' counters there take in the network too.
        let disk = crate::system::disk_rates().unwrap_or(io_then);
        let battery = meter.battery();
        battery_then = battery.as_ref().map(|b| b.percent);
        let (mut nin, mut nout) = (0u64, 0u64);
        for (name, n) in nets.iter() {
            if name != "lo0" {
                nin += n.received();
                nout += n.transmitted();
            }
        }
        let mem_total = sys.total_memory() as f64;
        let cores: Vec<f64> = sys
            .cpus()
            .iter()
            .map(|c| (c.cpu_usage() as f64).clamp(0.0, 100.0))
            .collect();
        let s = Sample {
            t: chrono::Local::now().timestamp_millis(),
            cpu: (sys.global_cpu_usage() as f64).clamp(0.0, 100.0),
            mem: sys.used_memory() as f64,
            swap: sys.used_swap() as f64,
            pressure: crate::system::pressure(),
            disk_r: disk.0,
            disk_w: disk.1,
            net_in: nin as f64 / dt,
            net_out: nout as f64 / dt,
            thermal: crate::system::thermal(),
            power: battery.as_ref().and_then(|b| b.watts),
            battery: battery.as_ref().map(|b| b.percent),
            gpu: gpu_then,
            mem_parts: crate::system::mem_parts(),
            on_ac: battery.as_ref().is_some_and(|b| b.on_ac),
            cpu_p: kind_avg(&cores, 'P'),
            cpu_e: kind_avg(&cores, 'E'),
        };

        let minute = now / 60;
        if saved_minute != Some(minute) {
            saved_minute = None;
            with_acc(|acc| {
                // Only ever forward: should the clock step back, the readings go on into the
                // minute in hand rather than reopen a saved minute, whose row they would replace.
                if minute > acc.minute {
                    acc.flush();
                    acc.minute = minute;
                }
                acc.add(&s, mem_total, &apps);
            });
        }

        // Swap grown by 1 GB from its lowest in the last 10 minutes, since the last sleep or
        // pause (what swap did across one is no sign of anything now).
        let swap_low = live(|l| {
            l.samples
                .iter()
                .filter(|x| x.t >= (now - 600).max(since_gap) * 1000)
                .map(|x| x.swap)
                .fold(f64::MAX, f64::min)
        })
        .min(s.swap);
        let swap_rising = s.swap - swap_low >= GIB;
        let swap_settled = live(|l| {
            l.samples
                .iter()
                .rev()
                .take_while(|x| x.t >= (now - 300).max(since_gap) * 1000)
                .all(|x| x.swap >= s.swap - MIB)
        });
        watches[0].step(now, s.cpu >= 90.0, s.cpu < 80.0, s.cpu, &apps);
        watches[1].step(now, s.pressure >= 4, s.pressure < 4, s.mem, &apps);
        watches[2].step(
            now,
            swap_rising,
            swap_settled && !swap_rising,
            s.swap,
            &apps,
        );

        live(|l| {
            l.samples.push_back(s);
            while l
                .samples
                .front()
                .is_some_and(|x| x.t < (now - LIVE_SECS) * 1000)
            {
                l.samples.pop_front();
            }
            l.processes = sys.processes().len();
            l.mem_total = mem_total;
            l.apps = apps;
            l.battery = battery;
            l.cores = cores;
        });
        share();
    }
}

/// Held by `run` through each pass and by `stop`, which sets STOPPING: once `stop` has saved the
/// minute and ended the incidents, no pass opens or updates one again, or gathers another minute.
static STEP: Mutex<()> = Mutex::new(());
static STOPPING: AtomicBool = AtomicBool::new(false);

/// The minute being gathered, shared with `stop`.
static ACC: Mutex<Option<Acc>> = Mutex::new(None);

fn with_acc<T>(f: impl FnOnce(&mut Acc) -> T) -> T {
    let mut g = ACC.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Acc::default))
}

/// The app is quitting, or the helper was told to stop: saves the minute in hand and ends the
/// incidents going on now, rather than leaving the next recorder to guess where they ended.
/// Nothing when this process isn't the recorder.
///
/// Waits at most two seconds for the pass under way (a roll-up, or the database busy with the
/// other process, can hold it much longer: the app would hang on quitting). Past that, it only
/// stops further passes and saves nothing: the pass's writes are each a transaction, so one cut
/// short by the exit is undone whole, and the next recorder ends the incidents left open
/// (`close_dangling`). What is lost is the part of a minute in hand.
pub fn stop() {
    if !recording() {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let _step = loop {
        match STEP.try_lock() {
            Ok(g) => break g,
            Err(std::sync::TryLockError::Poisoned(e)) => break e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                STOPPING.store(true, Ordering::SeqCst);
                return;
            }
        }
    };
    if STOPPING.swap(true, Ordering::SeqCst) {
        return;
    }
    with_acc(|a| a.flush());
    let now = history::now();
    for (id, _, _) in live(|l| l.open.drain().map(|(_, v)| v).collect::<Vec<_>>()) {
        history::close_incident(id, now);
    }
}

// ---- what the UI asks ----

#[derive(Serialize, Clone, Debug)]
pub struct Status {
    /// "calm" or "attention".
    pub level: &'static str,
    /// Which metric the attention is about: "cpu", "memory", "swap", "disk", "energy" or "".
    pub about: &'static str,
    /// How bad: "warn" (getting tight, busy), "critical", or "" when calm. The screens colour
    /// the measure it is about amber or red, with `label` beside it.
    pub severity: &'static str,
    /// A word or two for the measure's card: "Getting tight", "Critical", "Busy".
    pub label: &'static str,
    pub headline: String,
    pub sub: String,
    /// The other measures needing attention, worst first, each as its own status (with no
    /// `also` of its own): the headline is about the first, but every one colours its card.
    pub also: Vec<Status>,
}

/// Bytes on disk or over the network, in decimal units (as Finder counts them).
fn gb(b: f64) -> String {
    if b >= 100e9 {
        format!("{:.0} GB", b / 1e9)
    } else if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else {
        format!("{:.0} MB", b / 1e6)
    }
}

/// A gigabyte and a megabyte of memory or swap, in binary units (see `mem_gb`).
const GIB: f64 = (1u64 << 30) as f64;
const MIB: f64 = (1u64 << 20) as f64;

/// Memory and swap, in binary units (as Activity Monitor counts them), so a 16 GB Mac says 16 GB.
fn mem_gb(b: f64) -> String {
    if b >= 100.0 * GIB {
        format!("{:.0} GB", b / GIB)
    } else if b >= GIB {
        format!("{:.1} GB", b / GIB)
    } else {
        format!("{:.0} MB", b / MIB)
    }
}

/// How things stand, in a sentence: the tray's and the Now page's headline.
pub fn status() -> Status {
    let (latest, open, mem_total, top) = live(|l| {
        (
            l.samples.back().cloned(),
            l.open.clone(),
            l.mem_total,
            l.apps.first().map(|a| a.app.clone()),
        )
    });
    // statfs only: the menu bar asks every few seconds, and the volume's name isn't needed.
    let (vol_total, vol_free) = crate::volume::stats("/").unwrap_or((0, 0));
    let attention = |about, severity, label, headline: String, sub: String| Status {
        level: "attention",
        about,
        severity,
        label,
        headline,
        sub,
        also: Vec::new(),
    };
    // Everything that needs attention, worst first: the first is the headline, the rest still
    // colour their own cards.
    let mut found: Vec<Status> = Vec::new();
    if let Some(s) = &latest {
        if open.contains_key("pressure") || s.pressure >= 4 {
            found.push(attention(
                "memory",
                "critical",
                "Critical",
                "Memory is running high".into(),
                format!(
                    "{} of {} in use, {} in swap.",
                    mem_gb(s.mem),
                    mem_gb(mem_total),
                    mem_gb(s.swap)
                ),
            ));
        }
    }
    let battery = live(|l| l.battery.clone());
    if let Some(b) = battery.as_ref().filter(|b| !b.on_ac && b.percent <= 10.0) {
        found.push(attention(
            "energy",
            "critical",
            "Battery low",
            "The battery is low".into(),
            format!(
                "{:.0}% left{}.",
                b.percent,
                b.minutes_left
                    .map(|m| format!(", about {} min", m))
                    .unwrap_or_default()
            ),
        ));
    }
    if vol_total > 0 && (vol_free as f64) < vol_total as f64 * 0.05 {
        found.push(attention(
            "disk",
            "critical",
            "Almost full",
            "The disk is almost full".into(),
            format!("{} free of {}.", gb(vol_free as f64), gb(vol_total as f64)),
        ));
    }
    // Warnings after anything critical, so a busy CPU never heads the list over a flat battery.
    if let Some(s) = &latest {
        if let Some((_, started, _)) = open.get("cpu") {
            let mins = ((history::now() - started) / 60).max(1);
            found.push(attention(
                "cpu",
                "warn",
                "Busy",
                "The CPU is busy".into(),
                format!(
                    "{:.0}% for {mins} min{}.",
                    s.cpu,
                    top.map(|t| format!(", mostly {t}")).unwrap_or_default()
                ),
            ));
        }
        if open.contains_key("swap") {
            found.push(attention(
                "swap",
                "warn",
                "Swap growing",
                "Swap is growing".into(),
                format!("{} in swap: memory is short.", mem_gb(s.swap)),
            ));
        }
    }
    if vol_total > 0 && (vol_free as f64) < vol_total as f64 * 0.1 {
        found.push(attention(
            "disk",
            "warn",
            "Nearly full",
            "The disk is nearly full".into(),
            format!("{} free of {}.", gb(vol_free as f64), gb(vol_total as f64)),
        ));
    }
    if latest.as_ref().is_some_and(|s| s.pressure == 2) {
        found.push(attention(
            "memory",
            "warn",
            "Getting tight",
            "Memory is getting tight".into(),
            "macOS has started compressing memory.".into(),
        ));
    }
    // One flag a measure: the worst of it, which comes first.
    let mut seen: Vec<&str> = Vec::new();
    found.retain(|f| {
        let fresh = !seen.contains(&f.about);
        seen.push(f.about);
        fresh
    });
    if !found.is_empty() {
        let mut first = found.remove(0);
        first.also = found;
        return first;
    }
    let week = history::now() - 7 * 86_400;
    let today = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|d| d.and_local_timezone(chrono::Local).single())
        .map_or(history::now() - 86_400, |d| d.timestamp());
    // Four scans of a week of minutes: worked out once a minute, not on every call (the menu
    // bar asks every few seconds).
    static NEAR: Mutex<Option<(i64, Option<&'static str>)>> = Mutex::new(None);
    let minute = history::now() / 60;
    let cached = *NEAR.lock().unwrap_or_else(|e| e.into_inner());
    let near = match cached {
        Some((m, n)) if m == minute => n,
        _ => {
            let n = [("cpu_max", "CPU"), ("mem_max", "memory")]
                .into_iter()
                .find(|(col, _)| {
                    matches!((history::high(col, today), history::high(col, week)),
                        (Some((t, _)), Some((w, _))) if w > 0.0 && t >= 0.9 * w)
                })
                .map(|(_, what)| what);
            *NEAR.lock().unwrap_or_else(|e| e.into_inner()) = Some((minute, n));
            n
        }
    };
    Status {
        level: "calm",
        about: "",
        severity: "",
        label: "",
        headline: if latest.is_some() {
            "Your Mac is calm".into()
        } else {
            "Starting to record".into()
        },
        sub: match near {
            _ if latest.is_none() => "The first readings arrive in a few seconds.".into(),
            Some(what) => format!("Today reached this week's {what} tidemark."),
            None => "Nothing today is near this week's tidemarks.".into(),
        },
        also: Vec::new(),
    }
}

#[derive(Serialize)]
pub struct LiveInfo {
    pub samples: Vec<Sample>,
    pub apps_cpu: Vec<AppUse>,
    pub apps_mem: Vec<AppUse>,
    pub apps_energy: Vec<AppUse>,
    pub apps_disk: Vec<AppUse>,
    pub battery: Option<Battery>,
    pub processes: usize,
    pub cores: usize,
    /// Each core's use now, 0-100, and its kind ('P' or 'E'; empty where the Mac doesn't say).
    pub core_use: Vec<f64>,
    pub core_kinds: String,
    pub mem_total: f64,
    pub paused_until: Option<i64>,
    pub status: Status,
}

#[tauri::command]
pub async fn perf_live() -> LiveInfo {
    let status = status();
    live(|l| {
        let mut by_mem = l.apps.clone();
        by_mem.sort_by(|a, b| b.mem.total_cmp(&a.mem));
        by_mem.truncate(LIVE_APPS);
        let mut by_energy = l.apps.clone();
        by_energy.sort_by(|a, b| b.energy.total_cmp(&a.energy));
        by_energy.truncate(LIVE_APPS);
        let mut by_disk = l.apps.clone();
        by_disk.sort_by(|a, b| b.disk.total_cmp(&a.disk));
        by_disk.truncate(LIVE_APPS);
        LiveInfo {
            samples: l.samples.iter().cloned().collect(),
            apps_cpu: l.apps.iter().take(LIVE_APPS).cloned().collect(),
            apps_mem: by_mem,
            apps_energy: by_energy,
            apps_disk: by_disk,
            battery: l.battery.clone(),
            processes: l.processes,
            cores: std::thread::available_parallelism().map_or(1, |n| n.get()),
            core_use: l.cores.clone(),
            core_kinds: if core_kinds().len() == l.cores.len() {
                core_kinds().iter().collect()
            } else {
                String::new()
            },
            mem_total: l.mem_total,
            paused_until: l.paused_until,
            status,
        }
    })
}

#[derive(Serialize)]
pub struct Mark {
    pub value: f64,
    pub at: i64,
}

#[derive(Serialize)]
pub struct PerfHistory {
    pub points: Vec<Minute>,
    /// The highest of each metric over the last `mark_days` days (7 by default): the tidemarks.
    pub tidemarks: HashMap<&'static str, Mark>,
    /// When the record starts, so a chart can say "recording since".
    pub since: Option<i64>,
}

/// Minutes over the last `secs` seconds, as at most `points` points, and the tidemarks of the
/// last `mark_days` days (the week by default).
#[tauri::command]
pub async fn perf_history(secs: i64, points: Option<i64>, mark_days: Option<i64>) -> PerfHistory {
    let to = history::now();
    let week = to - mark_days.unwrap_or(7) * 86_400;
    let mut tidemarks = HashMap::new();
    for (k, col) in [
        ("cpu", "cpu_max"),
        ("mem", "mem_max"),
        ("swap", "swap_max"),
        ("disk", "disk_max"),
        ("net", "net_max"),
        ("power", "power_max"),
        ("gpu", "gpu_max"),
        // Download and upload apart: net_max is the busiest moment of both together.
        ("down", "net_in_max"),
        ("up", "net_out_max"),
    ] {
        if let Some((value, at)) = history::high(col, week) {
            tidemarks.insert(k, Mark { value, at });
        }
    }
    PerfHistory {
        points: history::perf_range(to - secs, to, points.unwrap_or(480)),
        tidemarks,
        since: history::first_minute(),
    }
}

#[derive(Serialize)]
pub struct AppsAt {
    /// Busiest on average over the span.
    pub by_cpu: Vec<AppUse>,
    pub by_mem: Vec<AppUse>,
    pub by_energy: Vec<AppUse>,
    pub by_disk: Vec<AppUse>,
    /// Busiest at their busiest moment in the span (each app's *_max).
    pub peak_cpu: Vec<AppUse>,
    pub peak_mem: Vec<AppUse>,
    pub peak_energy: Vec<AppUse>,
    pub peak_disk: Vec<AppUse>,
}

/// The busiest apps between two times (a moment's span, or a whole range).
#[tauri::command]
pub async fn perf_apps(from: i64, to: i64) -> AppsAt {
    AppsAt {
        by_cpu: history::top_apps(from, to, By::Cpu, 8),
        by_mem: history::top_apps(from, to, By::Mem, 8),
        by_energy: history::top_apps(from, to, By::Energy, 8),
        by_disk: history::top_apps(from, to, By::Disk, 8),
        peak_cpu: history::peak_apps(from, to, By::Cpu, 8),
        peak_mem: history::peak_apps(from, to, By::Mem, 8),
        peak_energy: history::peak_apps(from, to, By::Energy, 8),
        peak_disk: history::peak_apps(from, to, By::Disk, 8),
    }
}

/// Times the Mac slept (or wasn't recorded) in the last `days` days, with the battery either side.
#[tauri::command]
pub async fn sleeps(days: i64) -> Vec<history::Sleep> {
    history::sleeps(history::now() - days * 86_400)
}

#[tauri::command]
pub async fn incidents(days: i64) -> Vec<history::Incident> {
    history::incidents(history::now() - days * 86_400)
}

/// Stops recording for `minutes` (0 resumes).
#[tauri::command]
pub fn pause_recording(minutes: i64) -> Option<i64> {
    let until = (minutes > 0).then(|| history::now() + minutes * 60);
    // A file, so the recorder sees it whichever process it is.
    match until {
        Some(t) => {
            let _ = std::fs::write(pause_path(), t.to_string());
        }
        None => {
            let _ = std::fs::remove_file(pause_path());
        }
    }
    live(|l| l.paused_until = until);
    until
}

fn when(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%a %-d %b %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn apps_line(apps: &[AppUse], by_mem: bool) -> String {
    apps.iter()
        .take(6)
        .map(|a| {
            if by_mem {
                format!("{} {}", a.app, mem_gb(a.mem))
            } else {
                format!("{} {:.0}% CPU", a.app, a.cpu)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The performance record as text, for Ask: now, today, the week's tidemarks, incidents and
/// the apps that used the most.
#[tauri::command]
pub async fn perf_brief() -> String {
    let now = history::now();
    let mut out = Vec::new();
    let st = status();
    out.push(format!("Performance now: {} — {}", st.headline, st.sub));
    live(|l| {
        if let Some(s) = l.samples.back() {
            let p = match s.pressure {
                4 => "critical",
                2 => "warning",
                _ => "normal",
            };
            out.push(format!(
                "CPU {:.0}% of {} cores; memory {} used of {}, pressure {p}; swap {}; {} processes.",
                s.cpu,
                std::thread::available_parallelism().map_or(1, |n| n.get()),
                mem_gb(s.mem),
                mem_gb(l.mem_total),
                mem_gb(s.swap),
                l.processes
            ));
        }
        if !l.apps.is_empty() {
            out.push(format!(
                "Busiest apps now by CPU: {}.",
                apps_line(&l.apps, false)
            ));
            let mut m = l.apps.clone();
            m.sort_by(|a, b| b.mem.total_cmp(&a.mem));
            out.push(format!(
                "Biggest apps now by memory: {}.",
                apps_line(&m, true)
            ));
        }
    });
    let day = now - 86_400;
    let week = now - 7 * 86_400;
    for (label, col, is_mem) in [
        ("CPU", "cpu_max", false),
        ("Memory", "mem_max", true),
        ("Swap", "swap_max", true),
    ] {
        let fmt = |(v, t): (f64, i64)| {
            if is_mem {
                format!("{} at {}", mem_gb(v), when(t))
            } else {
                format!("{v:.0}% at {}", when(t))
            }
        };
        let d = history::high(col, day)
            .map(fmt)
            .unwrap_or_else(|| "no record".into());
        let w = history::high(col, week)
            .map(fmt)
            .unwrap_or_else(|| "no record".into());
        out.push(format!(
            "{label}: highest in the last 24 h {d}; this week's high-water mark {w}."
        ));
    }
    let day_pts = history::perf_range(day, now, 1);
    if let Some(d) = day_pts.first() {
        let secs = d.minutes as f64 * 60.0;
        let rate = |b: f64| {
            if b >= 1e6 {
                format!("{:.1} MB/s", b / 1e6)
            } else {
                format!("{:.0} KB/s", b / 1e3)
            }
        };
        let now_net = live(|l| l.samples.back().map(|s| (s.net_in, s.net_out)));
        out.push(format!(
            "Network: {}over the last 24 h {} downloaded and {} uploaded; busiest minute {} (both ways).",
            now_net
                .map(|(i, o)| format!("now {} down and {} up; ", rate(i), rate(o)))
                .unwrap_or_default(),
            gb(d.net_in * secs),
            gb(d.net_out * secs),
            rate(d.net_max)
        ));
    }
    if let Some(b) = live(|l| l.battery.clone()) {
        out.push(format!(
            "Energy: {}battery {:.0}%{}{}{}.",
            b.watts
                .map(|w| format!("drawing {w:.1} W, "))
                .unwrap_or_default(),
            b.percent,
            if b.charging {
                ", charging"
            } else if b.on_ac {
                ", on power"
            } else {
                ", on battery"
            },
            b.minutes_left
                .map(|m| format!(", {} h {} min left", m / 60, m % 60))
                .unwrap_or_default(),
            match (b.health, b.cycles) {
                (Some(h), Some(c)) => format!("; battery health {h:.0}% after {c} cycles"),
                _ => String::new(),
            }
        ));
    }
    let energy = history::energy_shares(day, now, 6);
    if !energy.is_empty() {
        out.push(format!(
            "Apps' shares of the energy apps used over the last 24 h (the display and macOS use more): {}.",
            energy
                .iter()
                .map(|(app, share)| format!("{app} {:.0}%", 100.0 * share))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for sl in history::sleeps(week).iter().take(3) {
        if let (Some(b), Some(a)) = (sl.battery_before, sl.battery_after) {
            out.push(format!(
                "Asleep {} for {} h {} min: battery {b:.0}% to {a:.0}%.",
                when(sl.started),
                (sl.ended - sl.started) / 3600,
                (sl.ended - sl.started) % 3600 / 60
            ));
        }
    }
    if let Some(s) = live(|l| l.samples.back().cloned()) {
        if let Some(g) = s.gpu {
            out.push(format!(
                "GPU {g:.0}% busy now; highest in the last 24 h {}.",
                history::high("gpu_max", day)
                    .map(|(v, t)| format!("{v:.0}% at {}", when(t)))
                    .unwrap_or_else(|| "no record".into())
            ));
        }
        if let Some(m) = s.mem_parts {
            out.push(format!(
                "Memory in use is {} of apps, {} wired (the system's own) and {} compressed.",
                mem_gb(m.app),
                mem_gb(m.wired),
                mem_gb(m.compressed)
            ));
        }
    }
    let crashes = history::crashes(week);
    if crashes.is_empty() {
        out.push("No app crashed or hung in the last 7 days.".into());
    } else {
        let mut by: std::collections::BTreeMap<(String, String), (usize, i64)> = Default::default();
        for c in &crashes {
            let e = by
                .entry((c.app.clone(), c.kind.clone()))
                .or_insert((0, c.ts));
            e.0 += 1;
            e.1 = e.1.max(c.ts);
        }
        out.push(format!(
            "Crashes and hangs in the last 7 days: {}.",
            by.iter()
                .map(|((app, kind), (n, last))| format!(
                    "{app} {} {n} time{} (last {})",
                    if kind == "hang" { "hung" } else { "crashed" },
                    if *n == 1 { "" } else { "s" },
                    when(*last)
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let inc = history::incidents(week);
    if inc.is_empty() {
        out.push("No incidents in the last 7 days.".into());
    } else {
        out.push("Incidents in the last 7 days:".into());
        for i in inc.iter().take(12) {
            let what = match i.kind.as_str() {
                "cpu" => format!("CPU above 90%, peak {:.0}%", i.peak),
                "pressure" => format!(
                    "memory pressure critical, {} in use at the peak",
                    mem_gb(i.peak)
                ),
                _ => format!("swap grew, to {}", mem_gb(i.peak)),
            };
            let mins = i.ended.map(|e| ((e - i.started) / 60).max(1));
            let by_mem = i.kind != "cpu";
            out.push(format!(
                "- {}: {what}{}. Busiest then: {}.",
                when(i.started),
                mins.map(|m| format!(", {m} min"))
                    .unwrap_or_else(|| ", still going on".into()),
                apps_line(&i.snapshot, by_mem)
            ));
        }
    }
    out.push(format!(
        "Apps that used the most CPU over the last 24 h (average while running): {}.",
        apps_line(&history::top_apps(day, now, By::Cpu, 6), false)
    ));
    out.push(format!(
        "Apps that held the most memory over the last 24 h: {}.",
        apps_line(&history::top_apps(day, now, By::Mem, 6), true)
    ));
    out.join("\n")
}

#[derive(Serialize)]
pub struct MacInfo {
    pub chip: String,
    pub cores: usize,
    pub memory: f64,
    pub uptime_secs: u64,
}

#[tauri::command]
pub async fn mac_info() -> MacInfo {
    let mut sys = System::new();
    sys.refresh_memory();
    MacInfo {
        chip: crate::system::chip(),
        cores: std::thread::available_parallelism().map_or(1, |n| n.get()),
        memory: sys.total_memory() as f64,
        uptime_secs: System::uptime(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[cfg(not(windows))]
    #[test]
    fn helpers_count_as_their_app() {
        let p = Path::new("/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Helpers/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)");
        assert_eq!(
            app_of(Some(p), "Google Chrome Helper (Renderer)"),
            "Google Chrome"
        );
        assert_eq!(
            app_of(Some(Path::new("/usr/sbin/mds_stores")), "mds_stores"),
            "Spotlight (mds_stores)"
        );
        assert_eq!(app_of(None, "kernel_task"), "kernel_task");
    }

    /// The shared list keeps the busiest by memory, energy and disk I/O too, not only by CPU.
    #[test]
    fn shared_apps_keep_each_measures_busiest() {
        let mut apps: Vec<AppUse> = (0..20)
            .map(|i| AppUse {
                app: format!("cpu{i}"),
                cpu: 50.0 - i as f64,
                ..Default::default()
            })
            .collect();
        apps.push(AppUse {
            app: "hog".into(),
            mem: 8e9,
            ..Default::default()
        });
        apps.push(AppUse {
            app: "drain".into(),
            energy: 9.0,
            ..Default::default()
        });
        apps.push(AppUse {
            app: "writer".into(),
            disk: 5e8,
            ..Default::default()
        });
        let kept = busiest(&apps);
        assert!(kept.iter().any(|a| a.app == "hog"));
        assert!(kept.iter().any(|a| a.app == "drain"));
        assert!(kept.iter().any(|a| a.app == "writer"));
        assert_eq!(kept[0].app, "cpu0");
        assert!(kept.len() <= 4 * LIVE_APPS);
    }

    #[test]
    fn memory_is_binary_disk_is_decimal() {
        assert_eq!(mem_gb(16.0 * (1u64 << 30) as f64), "16.0 GB");
        assert_eq!(gb(16.0 * (1u64 << 30) as f64), "17.2 GB");
    }

    #[test]
    fn a_minute_keeps_its_peak() {
        let mut a = Acc::default();
        for cpu in [10.0, 90.0, 20.0] {
            a.add(
                &Sample {
                    cpu,
                    ..Default::default()
                },
                1.0,
                &[],
            );
        }
        assert_eq!(a.m.cpu_max, 90.0);
        assert_eq!(a.m.cpu / a.n, 40.0);
    }

    #[test]
    fn an_app_keeps_its_peak_in_the_minute() {
        let mut a = Acc::default();
        for (cpu, disk) in [(5.0, 0.0), (80.0, 4e6), (5.0, 0.0)] {
            let app = AppUse {
                app: "Xcode".into(),
                cpu,
                disk,
                ..Default::default()
            };
            a.add(&Sample::default(), 1.0, &[app]);
        }
        let x = &a.apps["Xcode"];
        assert_eq!((x.cpu, x.cpu_max), (90.0, 80.0)); // the sum, and the busiest reading
        assert_eq!(x.disk_max, 4e6);
    }
}
