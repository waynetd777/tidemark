// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Energy: the power the Mac draws, the battery, and each app's share.
//!
//! The battery's own controller (AppleSmartBattery in the I/O Registry) reports the system's
//! load in milliwatts (PowerTelemetryData.SystemLoad), the charge, time remaining, cycles and
//! capacities; it is read through `ioreg`, every 10 seconds, which is cheap at that rate. A Mac
//! without a battery has none of this, and shows only the apps' energy.
//!
//! Each app's energy is the kernel's billed energy for its processes (proc_pid_rusage, the
//! counter Activity Monitor's Energy tab is built on), in nanojoules; the difference between two
//! samples over the time between them is watts. Only this user's processes can be read.
//!
//! Windows has neither: its battery is read from the driver (power_win.rs), and it doesn't
//! measure what each process uses (Task Manager's "Power usage" is its own estimate, which no
//! program can read), so there is no energy by app there.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Battery {
    /// Charge, 0-100.
    pub percent: f64,
    pub charging: bool,
    /// On a power adapter (charging or not).
    pub on_ac: bool,
    pub fully_charged: bool,
    /// What the Mac draws, watts (on battery or adapter).
    pub watts: Option<f64>,
    /// Minutes to empty (on battery) or to full (charging), when macOS has an estimate.
    pub minutes_left: Option<i64>,
    pub cycles: Option<i64>,
    /// Capacity now as a share of the design capacity, 0-100.
    pub health: Option<f64>,
}

fn num(d: &plist::Dictionary, k: &str) -> Option<f64> {
    match d.get(k)? {
        plist::Value::Integer(i) => i
            .as_signed()
            .map(|v| v as f64)
            .or(i.as_unsigned().map(|v| v as f64)),
        plist::Value::Real(r) => Some(*r),
        _ => None,
    }
}

fn flag(d: &plist::Dictionary, k: &str) -> bool {
    matches!(d.get(k), Some(plist::Value::Boolean(true)))
}

fn sub<'a>(d: &'a plist::Dictionary, k: &str) -> Option<&'a plist::Dictionary> {
    d.get(k)?.as_dictionary()
}

/// The battery as the controller reports it in `ioreg -a` output; None without a battery.
pub fn parse(xml: &[u8]) -> Option<Battery> {
    let v: plist::Value = plist::from_bytes(xml).ok()?;
    let d = v.as_array()?.first()?.as_dictionary()?;
    let percent = num(d, "CurrentCapacity")?;
    let max = num(d, "MaxCapacity").unwrap_or(100.0).max(1.0);
    let on_ac = flag(d, "ExternalConnected");
    let charging = flag(d, "IsCharging");
    let load = sub(d, "PowerTelemetryData")
        .and_then(|p| num(p, "SystemLoad"))
        .filter(|w| *w > 0.0)
        .map(|mw| mw / 1000.0);
    // Without telemetry (older Macs): what the battery gives, when it is the only source.
    let from_battery = || {
        let (v, a) = (
            num(d, "Voltage")?,
            num(d, "InstantAmperage").or(num(d, "Amperage"))?,
        );
        (a < 0.0 && !on_ac).then(|| v * -a / 1e6)
    };
    let minutes = if charging {
        num(d, "AvgTimeToFull")
    } else if !on_ac {
        num(d, "AvgTimeToEmpty").or(num(d, "TimeRemaining"))
    } else {
        None
    }
    .filter(|m| *m > 0.0 && *m < 6000.0)
    .map(|m| m as i64);
    let data = sub(d, "BatteryData");
    let design = data
        .and_then(|b| num(b, "DesignCapacity"))
        .or(num(d, "DesignCapacity"));
    let nominal = data
        .and_then(|b| num(b, "NominalChargeCapacity"))
        .or(num(d, "NominalChargeCapacity"))
        .or(num(d, "AppleRawMaxCapacity"));
    Some(Battery {
        percent: (100.0 * percent / max).clamp(0.0, 100.0),
        charging,
        on_ac,
        fully_charged: flag(d, "FullyCharged"),
        watts: load.or_else(from_battery),
        minutes_left: minutes,
        cycles: num(d, "CycleCount").map(|c| c as i64),
        health: match (nominal, design) {
            (Some(n), Some(dc)) if dc > 0.0 => Some((100.0 * n / dc).min(100.0)),
            _ => None,
        },
    })
}

#[cfg(windows)]
fn read_battery() -> Option<Battery> {
    crate::power_win::battery()
}

#[cfg(target_os = "macos")]
fn read_battery() -> Option<Battery> {
    let out = std::process::Command::new("/usr/sbin/ioreg")
        .args(["-rn", "AppleSmartBattery", "-a"])
        .output()
        .ok()?;
    parse(&out.stdout)
}

/// The battery, read at most every 10 seconds.
pub struct Meter {
    last: Option<(Instant, Option<Battery>)>,
    /// Each process's billed energy at the last sample, nanojoules.
    energy: HashMap<i32, u64>,
}

impl Meter {
    pub fn new() -> Self {
        Meter {
            last: None,
            energy: HashMap::new(),
        }
    }

    pub fn battery(&mut self) -> Option<Battery> {
        match &self.last {
            Some((t, b)) if t.elapsed() < Duration::from_secs(10) => b.clone(),
            _ => {
                let b = read_battery();
                self.last = Some((Instant::now(), b.clone()));
                b
            }
        }
    }

    /// A process's power since the last sample, watts; None the first time it is seen or when
    /// it can't be read (another user's).
    pub fn watts(&mut self, pid: i32, dt: f64) -> Option<f64> {
        let nj = billed_energy(pid)?;
        let before = self.energy.insert(pid, nj)?;
        (dt > 0.0).then(|| nj.saturating_sub(before) as f64 / 1e9 / dt)
    }

    /// Forgets processes that have gone, so a reused pid starts afresh.
    pub fn keep_only(&mut self, alive: &std::collections::HashSet<i32>) {
        self.energy.retain(|p, _| alive.contains(p));
    }
}

/// rusage_info_v6 (sys/resource.h, macOS 13+), which libc does not define: v4, then the fields
/// that carry ri_energy_nj, the energy the kernel measured the process using.
#[cfg(target_os = "macos")]
#[repr(C)]
struct RusageV6 {
    v4: libc::rusage_info_v4,
    ri_flags: u64,
    ri_user_ptime: u64,
    ri_system_ptime: u64,
    ri_pinstructions: u64,
    ri_pcycles: u64,
    ri_energy_nj: u64,
    ri_penergy_nj: u64,
    rest: [u64; 14],
}

/// The energy a process has used since it started, nanojoules: the kernel's measure where the
/// Mac has one (v6), else what it bills the process for (v4).
fn billed_energy(pid: i32) -> Option<u64> {
    #[cfg(target_os = "macos")]
    unsafe {
        let mut v6: RusageV6 = std::mem::zeroed();
        if libc::proc_pid_rusage(pid, 6, &mut v6 as *mut _ as *mut libc::rusage_info_t) == 0 {
            let e = v6.ri_energy_nj;
            return Some(if e > 0 { e } else { v6.v4.ri_billed_energy });
        }
        let mut info: libc::rusage_info_v4 = std::mem::zeroed();
        let rc = libc::proc_pid_rusage(
            pid,
            libc::RUSAGE_INFO_V4,
            &mut info as *mut _ as *mut libc::rusage_info_t,
        );
        (rc == 0).then_some(info.ri_billed_energy)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><array><dict>
<key>CurrentCapacity</key><integer>54</integer>
<key>MaxCapacity</key><integer>100</integer>
<key>IsCharging</key><false/>
<key>ExternalConnected</key><false/>
<key>CycleCount</key><integer>219</integer>
<key>AvgTimeToEmpty</key><integer>195</integer>
<key>PowerTelemetryData</key><dict><key>SystemLoad</key><integer>10524</integer></dict>
<key>BatteryData</key><dict>
<key>DesignCapacity</key><integer>6249</integer>
<key>NominalChargeCapacity</key><integer>5900</integer>
</dict>
</dict></array></plist>"#;

    #[test]
    fn reads_the_battery_controller() {
        let b = parse(XML.as_bytes()).unwrap();
        assert_eq!(b.percent, 54.0);
        assert!(!b.on_ac);
        assert_eq!(b.watts, Some(10.524));
        assert_eq!(b.minutes_left, Some(195));
        assert_eq!(b.cycles, Some(219));
        assert!((b.health.unwrap() - 94.4).abs() < 0.1);
    }

    #[test]
    fn no_battery_is_none() {
        assert_eq!(parse(b"<plist version=\"1.0\"><array/></plist>"), None);
    }

    // Windows doesn't measure each process's energy.
    #[cfg(target_os = "macos")]
    #[test]
    fn work_shows_as_energy() {
        let pid = std::process::id() as i32;
        let before = billed_energy(pid).unwrap();
        // A virtual Mac (CI's runners) has no energy model, so the kernel bills nothing: the
        // count stays at zero however hard this works. Only real hardware can show it.
        if before == 0 {
            return;
        }
        let t = Instant::now();
        let mut x = 0u64;
        while t.elapsed() < Duration::from_millis(300) {
            x = x.wrapping_mul(31).wrapping_add(7);
        }
        std::hint::black_box(x);
        assert!(billed_energy(pid).unwrap() > before);
    }
}
