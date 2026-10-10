// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: MIT (see LICENSE at the repository root)

//! The performance recorder. A thread samples the Mac every few seconds (2 s while a window is
//! open, 5 s otherwise), keeps the last 15 minutes in memory for the live charts, writes one row
//! a minute (averages and maxima) with the busiest apps of that minute to the history, and opens
//! an incident when something crosses a line: CPU above 90% for 30 s, memory pressure critical,
//! or swap growing by 1 GB within 10 minutes. Each incident keeps what every app was doing at
//! its peak.
//!
//! Processes are grouped by the app they belong to (the outermost .app in the executable's
//! path), so Chrome's dozens of helpers count as Google Chrome. CPU is a share of the whole
//! machine, not of one core.

use crate::energy::{Battery, Meter};
use crate::history::{self, AppUse, By, Minute};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// How much of the recent past the live charts get.
const LIVE_SECS: i64 = 15 * 60;
/// Apps kept per minute, by CPU and by memory (the union is stored).
const APPS_PER_MINUTE: usize = 8;

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

/// The app a process belongs to: the outermost bundle in its path, else its own name.
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

/// macOS memory pressure: 1 normal, 2 warning, 4 critical.
fn pressure() -> i64 {
    let mut v: libc::c_int = 1;
    let mut sz = std::mem::size_of::<libc::c_int>();
    let ok = unsafe {
        libc::sysctlbyname(
            c"kern.memorystatus_vm_pressure_level".as_ptr(),
            &mut v as *mut _ as *mut libc::c_void,
            &mut sz,
            std::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        v as i64
    } else {
        1
    }
}

/// 0 nominal, 1 fair, 2 serious, 3 critical.
fn thermal() -> i64 {
    #[cfg(target_os = "macos")]
    {
        objc2_foundation::NSProcessInfo::processInfo()
            .thermalState()
            .0 as i64
    }
    #[cfg(not(target_os = "macos"))]
    0
}

fn sysctl_string(name: &std::ffi::CStr) -> Option<String> {
    let mut sz = 0usize;
    unsafe {
        if libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut sz,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        let mut buf = vec![0u8; sz];
        if libc::sysctlbyname(
            name.as_ptr(),
            buf.as_mut_ptr() as *mut _,
            &mut sz,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        buf.truncate(sz.saturating_sub(1));
        String::from_utf8(buf).ok()
    }
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

/// The minute being gathered.
#[derive(Default)]
struct Acc {
    minute: i64,
    n: f64,
    m: Minute,
    apps: HashMap<String, (f64, f64, f64)>,
    /// Power readings in the minute (not every sample has one) and their sum.
    power_n: f64,
    gpu_n: f64,
    parts_n: f64,
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
        if let Some(p) = s.mem_parts {
            m.mem_app = Some(m.mem_app.unwrap_or(0.0) + p.app);
            m.mem_wired = Some(m.mem_wired.unwrap_or(0.0) + p.wired);
            m.mem_comp = Some(m.mem_comp.unwrap_or(0.0) + p.compressed);
            self.parts_n += 1.0;
        }
        for a in apps {
            let e = self.apps.entry(a.app.clone()).or_default();
            e.0 += a.cpu;
            e.1 += a.mem;
            e.2 += a.energy;
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
        self.gpu_n = 0.0;
        self.parts_n = 0.0;
        let mut apps: Vec<AppUse> = self
            .apps
            .drain()
            .map(|(app, (c, mm, e))| AppUse {
                app,
                cpu: c / n,
                mem: mm / n,
                energy: e / n,
            })
            .collect();
        apps.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
        let mut keep: Vec<AppUse> = apps.iter().take(APPS_PER_MINUTE).cloned().collect();
        apps.sort_by(|a, b| b.mem.total_cmp(&a.mem));
        for a in apps.iter().take(APPS_PER_MINUTE) {
            if !keep.iter().any(|k| k.app == a.app) {
                keep.push(a.clone());
            }
        }
        apps.sort_by(|a, b| b.energy.total_cmp(&a.energy));
        for a in apps.into_iter().take(APPS_PER_MINUTE) {
            if !keep.iter().any(|k| k.app == a.app) {
                keep.push(a);
            }
        }
        history::add_minute(&m, &keep);
        self.n = 0.0;
    }
}

// ---- one recorder at a time ----
//
// The app and the background helper (`Tidemark --record`, a launch agent: see recorder.rs) can
// both be running. Whichever holds recorder.lock records; the other reads what it shares in
// live.json. The lock goes with the process, so when the recorder stops, the other takes over
// within seconds.

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
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return false;
    }
    // Held for the rest of the process's life.
    std::mem::forget(f);
    true
}

fn live_path() -> std::path::PathBuf {
    crate::cache::dir().join("live.json")
}
/// Touched by a process with a window on screen, so the recorder samples faster.
fn watching_path() -> std::path::PathBuf {
    crate::cache::dir().join("watching")
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

/// The recorder's state, for the process that isn't recording.
fn share() {
    let bytes = live(|l| serde_json::to_vec(l).unwrap_or_default());
    let tmp = live_path().with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, live_path());
    }
}

/// Reads what the recorder shared, if it is recent (a stopped recorder's file is stale).
fn follow() {
    let fresh = std::fs::metadata(live_path())
        .and_then(|m| m.modified())
        .is_ok_and(|t| t.elapsed().is_ok_and(|e| e.as_secs() < 30));
    let shared: Option<Live> = fresh
        .then(|| std::fs::read(live_path()).ok())
        .flatten()
        .and_then(|b| serde_json::from_slice(&b).ok());
    if let Some(l) = shared {
        *LIVE.lock().unwrap_or_else(|e| e.into_inner()) = Some(l);
    }
    if FAST.load(Ordering::Relaxed) {
        let _ = std::fs::write(watching_path(), b"");
    }
}

/// Records if no other process is, else follows the one that does, taking over when it stops.
pub fn start() {
    std::thread::Builder::new()
        .name("monitor".into())
        .spawn(|| loop {
            if try_lock() {
                RECORDING.store(true, Ordering::SeqCst);
                history::close_dangling();
                // What the previous recorder had open is closed now; forget its copy.
                live(|l| l.open.clear());
                run();
            }
            follow();
            std::thread::sleep(Duration::from_secs(2));
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
    let mut last = Instant::now();
    // Instant stops while the Mac sleeps (CLOCK_UPTIME_RAW), so sleep shows only in wall-clock
    // time: gaps are measured with this, rates with `last`.
    let mut last_wall = history::now();
    let mut acc = Acc::default();
    let mut meter = Meter::new();
    // The battery when the last sample was taken, for what a sleep cost.
    let mut battery_then: Option<f64> = None;
    let mut watches = [
        Watch::new("cpu", 30, 30, false),
        Watch::new("pressure", 5, 30, true),
        Watch::new("swap", 0, 0, true),
    ];
    loop {
        // Faster while a window is on screen here, or in the other process.
        let watched = std::fs::metadata(watching_path())
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|e| e.as_secs() < 10));
        let every = if FAST.load(Ordering::Relaxed) || watched {
            2
        } else {
            5
        };
        std::thread::sleep(Duration::from_secs(every));
        let now = history::now();
        let paused = read_pause();
        live(|l| l.paused_until = paused);
        if paused.is_some() {
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
            last = Instant::now();
            last_procs = last;
            last_wall = now;
            acc.flush();
            continue;
        }
        let dt = last.elapsed().as_secs_f64();
        last = Instant::now();
        let wall_dt = (now - last_wall) as f64;
        last_wall = now;
        refresh(&mut sys, &mut nets);
        let procs_due = every == 2 || last_procs.elapsed() >= Duration::from_secs(15);
        // Asleep in between: the counters span the sleep, so this sample says nothing about
        // any moment. The gap stays a gap.
        if wall_dt.max(dt) > every as f64 * 3.0 + 5.0 {
            acc.flush();
            // Nothing that was building up before the gap carries across it.
            for w in watches.iter_mut() {
                w.since_on = None;
                w.since_off = None;
            }
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
            refresh_procs(&mut sys);
            // Rates over the time since the last per-process pass, whatever the sample rate.
            let dt = last_procs.elapsed().as_secs_f64();
            last_procs = Instant::now();
            let mut by_app: HashMap<String, AppUse> = HashMap::new();
            let (mut rd, mut wr) = (0u64, 0u64);
            for p in sys.processes().values() {
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
            }
            meter.keep_only(&sys.processes().keys().map(|p| p.as_u32() as i32).collect());
            let mut apps: Vec<AppUse> = by_app.into_values().collect();
            apps.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
            apps_then = apps;
            io_then = (rd as f64 / dt, wr as f64 / dt);
        }
        let apps = apps_then.clone();
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
        let s = Sample {
            t: chrono::Local::now().timestamp_millis(),
            cpu: (sys.global_cpu_usage() as f64).clamp(0.0, 100.0),
            mem: sys.used_memory() as f64,
            swap: sys.used_swap() as f64,
            pressure: pressure(),
            disk_r: io_then.0,
            disk_w: io_then.1,
            net_in: nin as f64 / dt,
            net_out: nout as f64 / dt,
            thermal: thermal(),
            power: battery.as_ref().and_then(|b| b.watts),
            battery: battery.as_ref().map(|b| b.percent),
            gpu: crate::system::gpu_busy(),
            mem_parts: crate::system::mem_parts(),
            on_ac: battery.as_ref().is_some_and(|b| b.on_ac),
        };

        let minute = now / 60;
        if minute != acc.minute {
            acc.flush();
            acc.minute = minute;
        }
        acc.add(&s, mem_total, &apps);

        // Swap grown by 1 GB from its lowest in the last 10 minutes.
        let swap_low = live(|l| {
            l.samples
                .iter()
                .filter(|x| x.t >= (now - 600) * 1000)
                .map(|x| x.swap)
                .fold(f64::MAX, f64::min)
        })
        .min(s.swap);
        let swap_rising = s.swap - swap_low >= 1e9;
        let swap_settled = live(|l| {
            l.samples
                .iter()
                .rev()
                .take_while(|x| x.t >= (now - 300) * 1000)
                .all(|x| x.swap >= s.swap - 1e6)
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
        });
        share();
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
}

fn gb(b: f64) -> String {
    if b >= 100e9 {
        format!("{:.0} GB", b / 1e9)
    } else if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else {
        format!("{:.0} MB", b / 1e6)
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
    let vol = crate::volume::info("/");
    let attention = |about, severity, label, headline: String, sub: String| Status {
        level: "attention",
        about,
        severity,
        label,
        headline,
        sub,
    };
    if let Some(s) = &latest {
        if open.contains_key("pressure") || s.pressure >= 4 {
            return attention(
                "memory",
                "critical",
                "Critical",
                "Memory is running high".into(),
                format!(
                    "{} of {} in use, {} in swap.",
                    gb(s.mem),
                    gb(mem_total),
                    gb(s.swap)
                ),
            );
        }
        if let Some((_, started, _)) = open.get("cpu") {
            let mins = ((history::now() - started) / 60).max(1);
            return attention(
                "cpu",
                "warn",
                "Busy",
                "The CPU is busy".into(),
                format!(
                    "{:.0}% for {mins} min{}.",
                    s.cpu,
                    top.map(|t| format!(", mostly {t}")).unwrap_or_default()
                ),
            );
        }
        if open.contains_key("swap") {
            return attention(
                "swap",
                "warn",
                "Swap growing",
                "Swap is growing".into(),
                format!("{} in swap: memory is short.", gb(s.swap)),
            );
        }
    }
    let battery = live(|l| l.battery.clone());
    if let Some(b) = battery.as_ref().filter(|b| !b.on_ac && b.percent <= 10.0) {
        return attention(
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
        );
    }
    if vol.total > 0 && (vol.free as f64) < vol.total as f64 * 0.1 {
        let critical = (vol.free as f64) < vol.total as f64 * 0.05;
        return attention(
            "disk",
            if critical { "critical" } else { "warn" },
            if critical {
                "Almost full"
            } else {
                "Nearly full"
            },
            "The disk is nearly full".into(),
            format!("{} free of {}.", gb(vol.free as f64), gb(vol.total as f64)),
        );
    }
    if latest.as_ref().is_some_and(|s| s.pressure == 2) {
        return attention(
            "memory",
            "warn",
            "Getting tight",
            "Memory is getting tight".into(),
            "macOS has started compressing memory.".into(),
        );
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
    }
}

#[derive(Serialize)]
pub struct LiveInfo {
    pub samples: Vec<Sample>,
    pub apps_cpu: Vec<AppUse>,
    pub apps_mem: Vec<AppUse>,
    pub apps_energy: Vec<AppUse>,
    pub battery: Option<Battery>,
    pub processes: usize,
    pub cores: usize,
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
        by_mem.truncate(8);
        let mut by_energy = l.apps.clone();
        by_energy.sort_by(|a, b| b.energy.total_cmp(&a.energy));
        by_energy.truncate(8);
        LiveInfo {
            samples: l.samples.iter().cloned().collect(),
            apps_cpu: l.apps.iter().take(8).cloned().collect(),
            apps_mem: by_mem,
            apps_energy: by_energy,
            battery: l.battery.clone(),
            processes: l.processes,
            cores: std::thread::available_parallelism().map_or(1, |n| n.get()),
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
        ("down", "net_in"),
        ("up", "net_out"),
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
    pub by_cpu: Vec<AppUse>,
    pub by_mem: Vec<AppUse>,
    pub by_energy: Vec<AppUse>,
}

/// The busiest apps between two times (a peak's minute, or a whole range).
#[tauri::command]
pub async fn perf_apps(from: i64, to: i64) -> AppsAt {
    AppsAt {
        by_cpu: history::top_apps(from, to, By::Cpu, 8),
        by_mem: history::top_apps(from, to, By::Mem, 8),
        by_energy: history::top_apps(from, to, By::Energy, 8),
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
                format!("{} {}", a.app, gb(a.mem))
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
                gb(s.mem),
                gb(l.mem_total),
                gb(s.swap),
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
                format!("{} at {}", gb(v), when(t))
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
    let energy = history::top_apps(day, now, By::Energy, 6);
    if energy.iter().any(|a| a.energy > 0.0) {
        out.push(format!(
            "Apps' shares of the energy apps used over the last 24 h (the display and macOS use more): {}.",
            {
                let total: f64 = energy.iter().map(|a| a.energy).sum::<f64>().max(1e-9);
                energy
                    .iter()
                    .map(|a| format!("{} {:.0}%", a.app, 100.0 * a.energy / total))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
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
                gb(m.app),
                gb(m.wired),
                gb(m.compressed)
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
                    gb(i.peak)
                ),
                _ => format!("swap grew, to {}", gb(i.peak)),
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
        chip: sysctl_string(c"machdep.cpu.brand_string").unwrap_or_default(),
        cores: std::thread::available_parallelism().map_or(1, |n| n.get()),
        memory: sys.total_memory() as f64,
        uptime_secs: System::uptime(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
}
