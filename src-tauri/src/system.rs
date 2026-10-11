// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Readings sysinfo doesn't give: how busy the GPU is, what the memory in use is made of, and
//! which cores are performance cores and which efficiency cores.
//!
//! The GPU's driver publishes its own utilisation in the I/O Registry
//! (IOAccelerator > PerformanceStatistics > "Device Utilization %"), read through `ioreg`; a Mac
//! with more than one GPU reports the busiest. Memory comes from the kernel's VM statistics
//! (host_statistics64), split as Activity Monitor splits it: app memory (anonymous pages, less
//! what can be purged), wired (the kernel's, never paged out) and compressed. Each core's kind is
//! the device tree's `cluster-type` ("P" or "E") on its cpu node, by `logical-cpu-id`, which is
//! the order sysinfo lists cores in.

use serde::{Deserialize, Serialize};

/// The busiest GPU's utilisation, 0-100; None if no GPU reports one.
pub fn gpu_busy() -> Option<f64> {
    let out = std::process::Command::new("/usr/sbin/ioreg")
        .args(["-r", "-d", "1", "-c", "IOAccelerator", "-a"])
        .output()
        .ok()?;
    parse_gpu(&out.stdout)
}

pub fn parse_gpu(xml: &[u8]) -> Option<f64> {
    let v: plist::Value = plist::from_bytes(xml).ok()?;
    v.as_array()?
        .iter()
        .filter_map(|e| {
            let stats = e
                .as_dictionary()?
                .get("PerformanceStatistics")?
                .as_dictionary()?;
            match stats.get("Device Utilization %")? {
                plist::Value::Integer(i) => i.as_signed().map(|v| v as f64),
                plist::Value::Real(r) => Some(*r),
                _ => None,
            }
        })
        .reduce(f64::max)
        .map(|v| v.clamp(0.0, 100.0))
}

/// Each core's kind, 'P' or 'E', in sysinfo's order; empty where the Mac doesn't say (Intel).
pub fn core_kinds() -> Vec<char> {
    std::process::Command::new("/usr/sbin/ioreg")
        .args([
            "-p",
            "IODeviceTree",
            "-c",
            "IOPlatformDevice",
            "-k",
            "cluster-type",
            "-r",
            "-d",
            "1",
            "-a",
        ])
        .output()
        .map(|o| parse_core_kinds(&o.stdout))
        .unwrap_or_default()
}

pub fn parse_core_kinds(xml: &[u8]) -> Vec<char> {
    let Ok(v) = plist::from_bytes::<plist::Value>(xml) else {
        return Vec::new();
    };
    let mut cores: Vec<(u64, char)> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let d = e.as_dictionary()?;
            let id = d.get("logical-cpu-id")?.as_unsigned_integer()?;
            let kind = match d.get("cluster-type")? {
                plist::Value::Data(b) => *b.first()? as char,
                plist::Value::String(s) => s.chars().next()?,
                _ => return None,
            };
            matches!(kind, 'P' | 'E').then_some((id, kind))
        })
        .collect();
    cores.sort();
    // Ids with a hole in them would put a kind against the wrong core: trust none of it.
    if cores.iter().enumerate().any(|(i, (id, _))| *id != i as u64) {
        return Vec::new();
    }
    cores.into_iter().map(|(_, k)| k).collect()
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct MemParts {
    /// Bytes.
    pub app: f64,
    pub wired: f64,
    pub compressed: f64,
}

/// What the memory in use is made of, from the kernel's VM statistics.
pub fn mem_parts() -> Option<MemParts> {
    #[cfg(target_os = "macos")]
    unsafe {
        let mut info: libc::vm_statistics64 = std::mem::zeroed();
        let mut count = (std::mem::size_of::<libc::vm_statistics64>()
            / std::mem::size_of::<libc::integer_t>())
            as libc::mach_msg_type_number_t;
        // One send right for the life of the process, not a new one per reading.
        static HOST: std::sync::OnceLock<libc::mach_port_t> = std::sync::OnceLock::new();
        #[allow(deprecated)]
        let host = *HOST.get_or_init(|| libc::mach_host_self());
        let rc = libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            &mut info as *mut _ as libc::host_info64_t,
            &mut count,
        );
        if rc != 0 {
            return None;
        }
        #[allow(deprecated)]
        let page = libc::vm_page_size as f64;
        let pages = |n: u64| n as f64 * page;
        Some(MemParts {
            app: pages(
                (info.internal_page_count as u64).saturating_sub(info.purgeable_count as u64),
            ),
            wired: pages(info.wire_count as u64),
            compressed: pages(info.compressor_page_count as u64),
        })
    }
    #[cfg(not(target_os = "macos"))]
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_busiest_gpu() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><array>
<dict><key>PerformanceStatistics</key><dict><key>Device Utilization %</key><integer>13</integer></dict></dict>
<dict><key>PerformanceStatistics</key><dict><key>Device Utilization %</key><integer>40</integer></dict></dict>
<dict><key>IOClass</key><string>NoStats</string></dict>
</array></plist>"#;
        assert_eq!(parse_gpu(xml), Some(40.0));
        assert_eq!(parse_gpu(b"<plist version=\"1.0\"><array/></plist>"), None);
    }

    #[test]
    fn reads_core_kinds_in_order() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><array>
<dict><key>logical-cpu-id</key><integer>1</integer><key>cluster-type</key><data>UAA=</data></dict>
<dict><key>logical-cpu-id</key><integer>0</integer><key>cluster-type</key><data>RQA=</data></dict>
</array></plist>"#;
        assert_eq!(parse_core_kinds(xml), vec!['E', 'P']);
        let holed = br#"<plist version="1.0"><array>
<dict><key>logical-cpu-id</key><integer>2</integer><key>cluster-type</key><data>UAA=</data></dict>
</array></plist>"#;
        assert!(parse_core_kinds(holed).is_empty());
    }

    #[test]
    fn memory_parts_add_up_to_something() {
        let m = mem_parts().unwrap();
        assert!(m.app > 0.0 && m.wired > 0.0);
    }
}
