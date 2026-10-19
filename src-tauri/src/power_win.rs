// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! Windows' power: the battery, the adapter, and how long the PC has had no input.
//!
//! The battery is read from its driver, as Windows' own battery report is: the battery device
//! (SetupDi, GUID_DEVCLASS_BATTERY) answers IOCTL_BATTERY_QUERY_INFORMATION (design and full
//! capacity, cycles) and IOCTL_BATTERY_QUERY_STATUS (power state, charge in mWh, and the rate in
//! mW, negative while it discharges). Unlike a Mac's controller it doesn't say what the whole PC
//! draws on the adapter: the rate then is what goes into the battery, so the power drawn is known
//! only on battery.

use crate::energy::Battery;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Power::{
    BatteryEstimatedTime, BatteryInformation, BATTERY_CHARGING, BATTERY_INFORMATION,
    BATTERY_POWER_ON_LINE, BATTERY_QUERY_INFORMATION, BATTERY_STATUS, BATTERY_UNKNOWN_RATE,
    BATTERY_UNKNOWN_TIME, BATTERY_WAIT_STATUS, IOCTL_BATTERY_QUERY_INFORMATION,
    IOCTL_BATTERY_QUERY_STATUS, IOCTL_BATTERY_QUERY_TAG,
};

/// The capacity figures are a percentage rather than mWh.
const BATTERY_CAPACITY_RELATIVE: u32 = 0x4000_0000;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// The first battery's device path (\\?\acpi#pnp0c0a#…), or None without one.
fn battery_path() -> Option<Vec<u16>> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        GUID_DEVCLASS_BATTERY, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    };
    unsafe {
        let set = SetupDiGetClassDevsW(
            Some(&GUID_DEVCLASS_BATTERY),
            windows::core::PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
        .ok()?;
        let mut iface = SP_DEVICE_INTERFACE_DATA {
            cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        let found = SetupDiEnumDeviceInterfaces(set, None, &GUID_DEVCLASS_BATTERY, 0, &mut iface);
        let path = found.ok().and_then(|_| {
            let mut need = 0u32;
            let _ = SetupDiGetDeviceInterfaceDetailW(set, &iface, None, 0, Some(&mut need), None);
            if need == 0 {
                return None;
            }
            let mut buf = vec![0u64; (need as usize).div_ceil(8)];
            let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            SetupDiGetDeviceInterfaceDetailW(set, &iface, Some(detail), need, None, None).ok()?;
            let start = std::ptr::addr_of!((*detail).DevicePath) as *const u16;
            let mut n = 0;
            while *start.add(n) != 0 {
                n += 1;
            }
            let mut p = std::slice::from_raw_parts(start, n).to_vec();
            p.push(0);
            Some(p)
        });
        let _ = SetupDiDestroyDeviceInfoList(set);
        path
    }
}

fn ioctl<I, O: Default>(h: &Handle, code: u32, input: &I) -> Option<O> {
    use windows::Win32::System::IO::DeviceIoControl;
    let mut out = O::default();
    let mut got = 0u32;
    unsafe {
        DeviceIoControl(
            h.0,
            code,
            Some(input as *const I as *const _),
            std::mem::size_of::<I>() as u32,
            Some(&mut out as *mut O as *mut _),
            std::mem::size_of::<O>() as u32,
            Some(&mut got),
            None,
        )
    }
    .ok()?;
    Some(out)
}

/// The battery now; None on a PC without one.
pub fn battery() -> Option<Battery> {
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    static PATH: std::sync::OnceLock<Option<Vec<u16>>> = std::sync::OnceLock::new();
    let path = PATH.get_or_init(battery_path).as_ref()?;
    let h = Handle(
        unsafe {
            CreateFileW(
                windows::core::PCWSTR(path.as_ptr()),
                0x8000_0000 | 0x4000_0000, // GENERIC_READ | GENERIC_WRITE
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
        .ok()?,
    );
    let tag: u32 = ioctl(&h, IOCTL_BATTERY_QUERY_TAG, &0u32)?;
    if tag == 0 {
        return None;
    }
    let ask = |level| BATTERY_QUERY_INFORMATION {
        BatteryTag: tag,
        InformationLevel: level,
        AtRate: 0,
    };
    let info: BATTERY_INFORMATION = ioctl(
        &h,
        IOCTL_BATTERY_QUERY_INFORMATION,
        &ask(BatteryInformation),
    )?;
    let status: BATTERY_STATUS = ioctl(
        &h,
        IOCTL_BATTERY_QUERY_STATUS,
        &BATTERY_WAIT_STATUS {
            BatteryTag: tag,
            ..Default::default()
        },
    )?;
    let secs: Option<u32> = ioctl(
        &h,
        IOCTL_BATTERY_QUERY_INFORMATION,
        &ask(BatteryEstimatedTime),
    );
    Some(from_driver(&info, &status, secs))
}

/// The Battery for what the driver said.
pub fn from_driver(info: &BATTERY_INFORMATION, s: &BATTERY_STATUS, secs: Option<u32>) -> Battery {
    let relative = info.Capabilities & BATTERY_CAPACITY_RELATIVE != 0;
    let full = if relative {
        100.0
    } else {
        info.FullChargedCapacity.max(1) as f64
    };
    let percent = (100.0 * s.Capacity as f64 / full).clamp(0.0, 100.0);
    let on_ac = s.PowerState & BATTERY_POWER_ON_LINE != 0;
    let charging = s.PowerState & BATTERY_CHARGING != 0;
    let rate = (s.Rate as u32 != BATTERY_UNKNOWN_RATE).then_some(s.Rate);
    Battery {
        percent,
        charging,
        on_ac,
        fully_charged: on_ac && !charging && percent >= 99.0,
        // Only on battery is the rate what the PC draws (mW).
        watts: rate
            .filter(|r| *r < 0 && !on_ac && !relative)
            .map(|r| -r as f64 / 1000.0),
        // Windows estimates the time to empty only.
        minutes_left: secs
            .filter(|s| *s != BATTERY_UNKNOWN_TIME && *s > 0 && !on_ac)
            .map(|s| (s / 60) as i64)
            .filter(|m| *m < 6000),
        cycles: (info.CycleCount > 0).then_some(info.CycleCount as i64),
        health: (!relative && info.DesignedCapacity > 0).then(|| {
            (100.0 * info.FullChargedCapacity as f64 / info.DesignedCapacity as f64).min(100.0)
        }),
    }
}

/// On the adapter (or a PC with no battery).
pub fn on_ac() -> bool {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s) }.is_err() || s.ACLineStatus != 0
}

/// Seconds since the last keyboard or mouse input in this session.
pub fn idle_secs() -> f64 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut li = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if !unsafe { GetLastInputInfo(&mut li) }.as_bool() {
        return 0.0;
    }
    // Both wrap after 49 days, together.
    unsafe { GetTickCount() }.wrapping_sub(li.dwTime) as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_driver() {
        let info = BATTERY_INFORMATION {
            DesignedCapacity: 42_000,
            FullChargedCapacity: 39_478,
            CycleCount: 0,
            ..Default::default()
        };
        let on_battery = BATTERY_STATUS {
            PowerState: 2,
            Capacity: 19_739,
            Voltage: 11_800,
            Rate: -9_500,
        };
        let b = from_driver(&info, &on_battery, Some(7_200));
        assert_eq!(b.percent, 50.0);
        assert!(!b.on_ac && !b.charging);
        assert_eq!(b.watts, Some(9.5));
        assert_eq!(b.minutes_left, Some(120));
        assert_eq!(b.cycles, None);
        assert!((b.health.unwrap() - 94.0).abs() < 0.1);
        let charging = BATTERY_STATUS {
            PowerState: BATTERY_POWER_ON_LINE | BATTERY_CHARGING,
            Capacity: 30_000,
            Voltage: 12_400,
            Rate: 20_000,
        };
        let b = from_driver(&info, &charging, Some(BATTERY_UNKNOWN_TIME));
        assert!(b.on_ac && b.charging && !b.fully_charged);
        assert_eq!(
            b.watts, None,
            "on the adapter the rate is the charge, not the PC's draw"
        );
        assert_eq!(b.minutes_left, None);
    }

    #[test]
    fn reads_this_pc() {
        let _ = battery();
        let _ = on_ac();
        assert!(idle_secs() >= 0.0);
    }
}
