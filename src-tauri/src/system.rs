// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Two readings sysinfo doesn't give: how busy the GPU is, and what the memory in use is made
//! of.
//!
//! The GPU's driver publishes its own utilisation in the I/O Registry
//! (IOAccelerator > PerformanceStatistics > "Device Utilization %"), read through `ioreg`; a Mac
//! with more than one GPU reports the busiest. Memory comes from the kernel's VM statistics
//! (host_statistics64), split as Activity Monitor splits it: app memory (anonymous pages, less
//! what can be purged), wired (the kernel's, never paged out) and compressed.

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
    fn memory_parts_add_up_to_something() {
        let m = mem_parts().unwrap();
        assert!(m.app > 0.0 && m.wired > 0.0);
    }
}
