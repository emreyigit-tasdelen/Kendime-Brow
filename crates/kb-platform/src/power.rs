//! Power and memory state (architecture §5.2/§5.4, task 2.9).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerState {
    /// Running on battery (not plugged in).
    pub on_battery: bool,
    /// Available physical memory in percent of total (0–100).
    pub memory_available_percent: u8,
}

impl PowerState {
    pub fn low_memory(&self) -> bool {
        self.memory_available_percent < 10
    }
}

#[cfg(windows)]
pub fn current() -> PowerState {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: plain out-structs of the documented size.
    unsafe {
        let mut p: SYSTEM_POWER_STATUS = std::mem::zeroed();
        let on_battery = GetSystemPowerStatus(&mut p) != 0 && p.ACLineStatus == 0;
        let mut m: MEMORYSTATUSEX = std::mem::zeroed();
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        let pct = if GlobalMemoryStatusEx(&mut m) != 0 && m.ullTotalPhys > 0 {
            (m.ullAvailPhys * 100 / m.ullTotalPhys) as u8
        } else {
            100
        };
        PowerState {
            on_battery,
            memory_available_percent: pct,
        }
    }
}

/// Linux: `/sys/class/power_supply` + `/proc/meminfo`; other platforms: plugged in, no pressure.
#[cfg(not(windows))]
pub fn current() -> PowerState {
    let mem = std::fs::read_to_string("/proc/meminfo").ok().and_then(|t| {
        let get = |k: &str| {
            t.lines()
                .find(|l| l.starts_with(k))?
                .split_whitespace()
                .nth(1)?
                .parse::<u64>()
                .ok()
        };
        Some((get("MemAvailable:")? * 100 / get("MemTotal:")?.max(1)) as u8)
    });
    let on_battery = std::fs::read_dir("/sys/class/power_supply")
        .map(|rd| {
            rd.filter_map(|e| e.ok()).any(|e| {
                std::fs::read_to_string(e.path().join("status"))
                    .is_ok_and(|s| s.trim() == "Discharging")
            })
        })
        .unwrap_or(false);
    PowerState {
        on_battery,
        memory_available_percent: mem.unwrap_or(100),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plausible_values() {
        let s = current();
        assert!(s.memory_available_percent <= 100);
        assert!(
            PowerState {
                on_battery: false,
                memory_available_percent: 5
            }
            .low_memory()
        );
        assert!(
            !PowerState {
                on_battery: true,
                memory_available_percent: 50
            }
            .low_memory()
        );
    }
}
