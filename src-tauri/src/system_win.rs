// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Windows' side of system.rs: the readings sysinfo doesn't give, under the names the Mac's have.
//!
//! - The GPU, the disk's read and write rates and the cooling limit come from Windows' performance
//!   counters (PDH), the ones Task Manager and Resource Monitor show: "GPU Engine(*)\Utilization
//!   Percentage" (the busiest engine, as Task Manager's GPU figure is), "PhysicalDisk(_Total)\Disk
//!   Read Bytes/sec" and "Disk Write Bytes/sec", and "Thermal Zone Information(*)\% Passive Limit".
//!   One query holds them all, collected at most once a second; a rate needs two collections,
//!   so the first readings are None.
//! - Windows has no memory pressure. It is worked out as the Mac's three levels from how much is
//!   committed of the commit limit (physical memory plus the page file: past it, programs fail
//!   to get memory), how much physical memory is free, and Windows' own low-memory signal
//!   (CreateMemoryResourceNotification), which is when it starts trimming every program.
//! - What memory in use is made of: the kernel's non-paged pool (never paged out: what macOS
//!   calls wired), the compression store (the Memory Compression process), and the rest.
//! - Thermal: Windows doesn't say how hot the PC is in a way programs can trust (most laptops'
//!   ACPI zones report a fixed figure), but it does say when it has slowed the processor to cool
//!   it (the passive limit, under 100%). That is the Mac's "serious" (slowed) or, below half,
//!   "critical".
//! - Core kinds: GetLogicalProcessorInformationEx's efficiency class, where the processor has two
//!   kinds of core (Intel's 12th generation and later): the higher class is P, the lower E.

use crate::system::MemParts;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhGetFormattedCounterValue, PdhOpenQueryW, PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA,
    PDH_FMT, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER,
    PDH_HQUERY, PDH_MORE_DATA,
};

/// Values over 100 (a busy engine counted twice over an interval) are kept, then clamped.
const NOCAP100: u32 = 0x0000_8000;

struct Counters {
    query: PDH_HQUERY,
    gpu: Option<PDH_HCOUNTER>,
    disk_r: Option<PDH_HCOUNTER>,
    disk_w: Option<PDH_HCOUNTER>,
    passive: Option<PDH_HCOUNTER>,
    /// When last collected, and how many collections there have been (rates need two).
    at: Option<Instant>,
    n: u32,
}

// The handles are only used under the mutex.
unsafe impl Send for Counters {}

static PDH: Mutex<Option<Counters>> = Mutex::new(None);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

impl Counters {
    fn open() -> Option<Counters> {
        let mut query = PDH_HQUERY::default();
        if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) } != 0 {
            return None;
        }
        // English names, so the counters are found whatever language Windows speaks.
        let add = |path: &str| {
            let p = wide(path);
            let mut c = PDH_HCOUNTER::default();
            (unsafe { PdhAddEnglishCounterW(query, PCWSTR(p.as_ptr()), 0, &mut c) } == 0)
                .then_some(c)
        };
        Some(Counters {
            query,
            gpu: add(r"\GPU Engine(*)\Utilization Percentage"),
            disk_r: add(r"\PhysicalDisk(_Total)\Disk Read Bytes/sec"),
            disk_w: add(r"\PhysicalDisk(_Total)\Disk Write Bytes/sec"),
            passive: add(r"\Thermal Zone Information(*)\% Passive Limit"),
            at: None,
            n: 0,
        })
    }

    /// Collects, unless that was done within the last second.
    fn fresh(&mut self) {
        if self
            .at
            .is_some_and(|t| t.elapsed() < Duration::from_millis(900))
        {
            return;
        }
        if unsafe { PdhCollectQueryData(self.query) } == 0 {
            self.n += 1;
        }
        self.at = Some(Instant::now());
    }

    fn value(&self, c: Option<PDH_HCOUNTER>) -> Option<f64> {
        if self.n < 2 {
            return None;
        }
        let mut v = PDH_FMT_COUNTERVALUE::default();
        let rc = unsafe {
            PdhGetFormattedCounterValue(c?, PDH_FMT(PDH_FMT_DOUBLE.0 | NOCAP100), None, &mut v)
        };
        (rc == 0 && ok(v.CStatus)).then_some(unsafe { v.Anonymous.doubleValue })
    }

    /// Every instance of a wildcard counter: (instance name, value).
    fn values(&self, c: Option<PDH_HCOUNTER>) -> Vec<(String, f64)> {
        let Some(c) = c.filter(|_| self.n >= 2) else {
            return Vec::new();
        };
        let fmt = PDH_FMT(PDH_FMT_DOUBLE.0 | NOCAP100);
        let (mut size, mut count) = (0u32, 0u32);
        let rc = unsafe { PdhGetFormattedCounterArrayW(c, fmt, &mut size, &mut count, None) };
        if rc != PDH_MORE_DATA || size == 0 {
            return Vec::new();
        }
        // u64s, so the items (which hold doubles and pointers) are aligned.
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        let rc =
            unsafe { PdhGetFormattedCounterArrayW(c, fmt, &mut size, &mut count, Some(items)) };
        if rc != 0 {
            return Vec::new();
        }
        (0..count as usize)
            .filter_map(|i| {
                // SAFETY: PDH wrote `count` items, their names inside the same buffer.
                let it = unsafe { &*items.add(i) };
                if !ok(it.FmtValue.CStatus) {
                    return None;
                }
                let name = unsafe { it.szName.to_string() }.ok()?;
                Some((name, unsafe { it.FmtValue.Anonymous.doubleValue }))
            })
            .collect()
    }
}

fn ok(status: u32) -> bool {
    status == PDH_CSTATUS_VALID_DATA || status == PDH_CSTATUS_NEW_DATA
}

/// Runs `f` on the counters, collected within the last second.
fn counters<T>(f: impl FnOnce(&Counters) -> T) -> Option<T> {
    let mut g = PDH.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Counters::open();
    }
    let c = g.as_mut()?;
    c.fresh();
    Some(f(c))
}

/// The GPU's utilisation, 0-100, as Task Manager gives it: each engine's use summed over the
/// processes using it, and the busiest engine of any GPU. None until the counters have a rate,
/// or if no GPU reports one.
pub fn gpu_busy() -> Option<f64> {
    counters(|c| busiest_engine(&c.values(c.gpu)))?
}

/// The busiest engine in "GPU Engine" instances, named
/// pid_1624_luid_0x00000000_0x00010384_phys_0_eng_7_engtype_3D: the same engine is in one
/// instance per process using it.
pub fn busiest_engine(items: &[(String, f64)]) -> Option<f64> {
    let mut engines: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for (name, v) in items {
        let engine = name.find("_luid_").map_or(name.as_str(), |i| &name[i..]);
        *engines.entry(engine).or_default() += v;
    }
    engines
        .into_values()
        .reduce(f64::max)
        .map(|v| v.clamp(0.0, 100.0))
}

/// The disk's read and write rates, bytes a second, over the last interval; None until there
/// are two collections. Windows counts the disk itself here, where the processes' I/O counters
/// count the network and devices too.
pub fn disk_rates() -> Option<(f64, f64)> {
    counters(|c| Some((c.value(c.disk_r)?, c.value(c.disk_w)?)))?
}

/// 0 nominal, 2 slowed to cool (the Mac's "serious"), 3 slowed by more than half ("critical").
pub fn thermal() -> i64 {
    counters(|c| {
        let limit = c
            .values(c.passive)
            .into_iter()
            .map(|(_, v)| v)
            .filter(|v| *v > 0.0)
            .reduce(f64::min)?;
        Some(level_for_limit(limit))
    })
    .flatten()
    .unwrap_or(0)
}

pub fn level_for_limit(limit: f64) -> i64 {
    if limit >= 99.5 {
        0
    } else if limit >= 50.0 {
        2
    } else {
        3
    }
}

/// Memory pressure as the Mac's levels: 1 normal, 2 warning, 4 critical.
pub fn pressure() -> i64 {
    use windows::Win32::System::Memory::{
        CreateMemoryResourceNotification, LowMemoryResourceNotification,
        QueryMemoryResourceNotification,
    };
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    static LOW: std::sync::OnceLock<Option<isize>> = std::sync::OnceLock::new();
    let low = LOW.get_or_init(|| {
        unsafe { CreateMemoryResourceNotification(LowMemoryResourceNotification) }
            .ok()
            .map(|h| h.0 as isize)
    });
    let signalled = low.is_some_and(|h| {
        let mut state = windows::core::BOOL(0);
        let h = windows::Win32::Foundation::HANDLE(h as *mut _);
        unsafe { QueryMemoryResourceNotification(h, &mut state) }.is_ok() && state.as_bool()
    });
    let mut m = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_err() {
        return 1;
    }
    let committed = m.ullTotalPageFile.saturating_sub(m.ullAvailPageFile);
    level(
        signalled,
        committed as f64 / m.ullTotalPageFile.max(1) as f64,
        m.ullAvailPhys as f64 / m.ullTotalPhys.max(1) as f64,
    )
}

/// The level for Windows' low-memory signal, the share of the commit limit in use and the share
/// of physical memory free.
pub fn level(low_signal: bool, commit: f64, free: f64) -> i64 {
    if low_signal || commit >= 0.95 || free < 0.03 {
        4
    } else if commit >= 0.85 || free < 0.10 {
        2
    } else {
        1
    }
}

/// What the memory in use is made of: the non-paged pool (wired), the compression store, and
/// the rest (apps and the system's paged memory).
pub fn mem_parts() -> Option<MemParts> {
    use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    let mut p = PERFORMANCE_INFORMATION {
        cb: std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32,
        ..Default::default()
    };
    unsafe { GetPerformanceInfo(&mut p, p.cb) }.ok()?;
    let page = p.PageSize as f64;
    let used = (p.PhysicalTotal.saturating_sub(p.PhysicalAvailable)) as f64 * page;
    let wired = p.KernelNonpaged as f64 * page;
    let compressed = compression_store().unwrap_or(0.0);
    Some(MemParts {
        app: (used - wired - compressed).max(0.0),
        wired,
        compressed,
    })
}

/// The Memory Compression process's working set: the physical memory the compressed pages take.
/// Its pid is looked for once (it lives as long as Windows); None where it can't be read.
fn compression_store() -> Option<f64> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    static PID: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    let pid = (*PID.get_or_init(|| {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing(),
        );
        sys.processes()
            .values()
            .find(|p| p.name().eq_ignore_ascii_case("Memory Compression"))
            .map(|p| p.pid().as_u32())
    }))?;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut c = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        let got = GetProcessMemoryInfo(h, &mut c, c.cb);
        let _ = CloseHandle(h);
        got.ok().map(|_| c.WorkingSetSize as f64)
    }
}

/// Each logical processor's kind, 'P' or 'E', in sysinfo's order (the processors' numbers); empty
/// unless the processor has two kinds of core.
pub fn core_kinds() -> Vec<char> {
    use windows::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, RelationProcessorCore,
    };
    let mut len = 0u32;
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len) };
    if len == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    if unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(buf.as_mut_ptr() as *mut _),
            &mut len,
        )
    }
    .is_err()
    {
        return Vec::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, len as usize) };
    kinds_from(bytes)
}

/// Reads SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX records for cores: Relationship and Size (u32
/// each), then PROCESSOR_RELATIONSHIP's Flags, EfficiencyClass, 20 reserved bytes, GroupCount and
/// its GROUP_AFFINITYs (a 64-bit mask, the group, 6 reserved bytes) from offset 32.
pub fn kinds_from(bytes: &[u8]) -> Vec<char> {
    let mut cpus: Vec<(usize, u8)> = Vec::new();
    let mut off = 0;
    while off + 32 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap()) as usize;
        if size == 0 || off + size > bytes.len() {
            break;
        }
        let class = bytes[off + 9];
        let groups = u16::from_le_bytes(bytes[off + 30..off + 32].try_into().unwrap()) as usize;
        for g in 0..groups {
            let at = off + 32 + g * 16;
            if at + 10 > off + size {
                break;
            }
            let mask = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
            let group = u16::from_le_bytes(bytes[at + 8..at + 10].try_into().unwrap()) as usize;
            for bit in 0..64 {
                if mask & (1 << bit) != 0 {
                    cpus.push((group * 64 + bit, class));
                }
            }
        }
        off += size;
    }
    cpus.sort();
    let top = cpus.iter().map(|c| c.1).max();
    let one_kind = cpus.iter().all(|c| Some(c.1) == top);
    // Numbers with a hole in them would put a kind against the wrong core: trust none of it.
    if one_kind || cpus.iter().enumerate().any(|(i, (n, _))| *n != i) {
        return Vec::new();
    }
    cpus.into_iter()
        .map(|(_, c)| if Some(c) == top { 'P' } else { 'E' })
        .collect()
}

/// The app a program belongs to, as Task Manager names it: its version information's
/// description ("Google Chrome", "Windows Explorer"), read once per program; else its file name
/// without .exe. Chrome's many chrome.exe processes are then one app, as on macOS.
pub fn app_name(exe: Option<&std::path::Path>, name: &str) -> String {
    static NAMES: Mutex<Option<std::collections::HashMap<std::path::PathBuf, Option<String>>>> =
        Mutex::new(None);
    let plain = || {
        name.strip_suffix(".exe")
            .or_else(|| name.strip_suffix(".EXE"))
            .unwrap_or(name)
            .to_string()
    };
    let Some(exe) = exe.filter(|p| p.is_absolute()) else {
        return plain();
    };
    let mut g = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    let names = g.get_or_insert_with(Default::default);
    let found = names
        .entry(exe.to_path_buf())
        .or_insert_with(|| description(exe))
        .clone();
    found.unwrap_or_else(plain)
}

/// A program's FileDescription, in the first language its version information has.
fn description(exe: &std::path::Path) -> Option<String> {
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    let path = crate::volume::wide(exe);
    let p = PCWSTR(path.as_ptr());
    let size = unsafe { GetFileVersionInfoSizeW(p, None) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    unsafe { GetFileVersionInfoW(p, None, size, data.as_mut_ptr() as *mut _) }.ok()?;
    let query = |sub: &str| -> Option<(*const u8, u32)> {
        let w = wide(sub);
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        unsafe {
            VerQueryValueW(
                data.as_ptr() as *const _,
                PCWSTR(w.as_ptr()),
                &mut ptr,
                &mut len,
            )
        }
        .as_bool()
        .then_some((ptr as *const u8, len))
        .filter(|(p, l)| !p.is_null() && *l > 0)
    };
    // Language and code page pairs; the usual US English pair when there are none.
    let (ptr, len) = query(r"\VarFileInfo\Translation").unwrap_or((std::ptr::null(), 0));
    let mut langs: Vec<(u16, u16)> = (0..(len as usize / 4))
        .map(|i| unsafe {
            let w = ptr.add(i * 4) as *const u16;
            (w.read_unaligned(), w.add(1).read_unaligned())
        })
        .collect();
    langs.push((0x0409, 0x04b0));
    langs.into_iter().find_map(|(lang, cp)| {
        let (p, n) = query(&format!(
            r"\StringFileInfo\{lang:04x}{cp:04x}\FileDescription"
        ))?;
        let chars = unsafe { std::slice::from_raw_parts(p as *const u16, n as usize) };
        let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
        let s = String::from_utf16_lossy(&chars[..end]).trim().to_string();
        (!s.is_empty()).then_some(s)
    })
}

/// The processor, as Windows names it ("Intel(R) Core(TM) i7-7600U CPU @ 2.80GHz").
pub fn chip() -> String {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let key = wide(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0");
    let value = wide("ProcessorNameString");
    let mut buf = [0u16; 256];
    let mut size = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    };
    if rc.is_err() {
        return String::new();
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    String::from_utf16_lossy(&buf[..n]).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_is_the_busiest_engine() {
        let items = [
            (
                "pid_10_luid_0x0_0x1_phys_0_eng_0_engtype_3D".to_string(),
                20.0,
            ),
            (
                "pid_11_luid_0x0_0x1_phys_0_eng_0_engtype_3D".to_string(),
                15.0,
            ),
            (
                "pid_10_luid_0x0_0x1_phys_0_eng_4_engtype_VideoDecode".to_string(),
                30.0,
            ),
        ];
        assert_eq!(busiest_engine(&items), Some(35.0));
        assert_eq!(busiest_engine(&[]), None);
    }

    #[test]
    fn levels() {
        assert_eq!(level(false, 0.5, 0.5), 1);
        assert_eq!(level(false, 0.9, 0.5), 2);
        assert_eq!(level(false, 0.5, 0.05), 2);
        assert_eq!(level(true, 0.5, 0.5), 4);
        assert_eq!(level(false, 0.97, 0.5), 4);
        assert_eq!(level_for_limit(100.0), 0);
        assert_eq!(level_for_limit(80.0), 2);
        assert_eq!(level_for_limit(30.0), 3);
    }

    /// Two cores of two threads each, the first of efficiency class 1 (P), the second 0 (E).
    #[test]
    fn core_kinds_from_records() {
        let rec = |class: u8, mask: u64| {
            let mut r = vec![0u8; 48];
            r[0..4].copy_from_slice(&0u32.to_le_bytes());
            r[4..8].copy_from_slice(&48u32.to_le_bytes());
            r[9] = class;
            r[30..32].copy_from_slice(&1u16.to_le_bytes());
            r[32..40].copy_from_slice(&mask.to_le_bytes());
            r
        };
        let mut b = rec(1, 0b0011);
        b.extend(rec(0, 0b1100));
        assert_eq!(kinds_from(&b), vec!['P', 'P', 'E', 'E']);
        let mut same = rec(0, 0b0011);
        same.extend(rec(0, 0b1100));
        assert!(
            kinds_from(&same).is_empty(),
            "one kind of core says nothing"
        );
    }

    #[test]
    fn apps_are_named_by_their_description() {
        let windir = std::env::var("SystemRoot").unwrap();
        let explorer = std::path::Path::new(&windir).join("explorer.exe");
        assert_eq!(
            app_name(Some(&explorer), "explorer.exe"),
            "Windows Explorer"
        );
        assert_eq!(app_name(None, "MsMpEng.exe"), "MsMpEng");
        assert_eq!(
            app_name(Some(std::path::Path::new("C:\\nowhere\\x.exe")), "x.exe"),
            "x"
        );
    }

    #[test]
    fn readings_on_this_pc() {
        assert!(!chip().is_empty());
        let m = mem_parts().unwrap();
        assert!(m.app > 0.0 && m.wired > 0.0);
        assert!([1, 2, 4].contains(&pressure()));
        assert!(counters(|_| ()).is_some(), "the performance counters open");
    }
}
