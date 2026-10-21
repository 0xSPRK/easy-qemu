//! Per-VM statistics: host-process CPU sampling + QMP block/balloon data.

use serde::Serialize;

/// One statistics snapshot for a VM (UI-facing).
#[derive(Serialize, Clone, Debug, Default)]
pub struct VmStats {
    /// Host-process CPU usage in percent of one core (can exceed 100).
    pub cpu_pct: Option<f64>,
    /// Current balloon size reported by the guest (MB), when available.
    pub balloon_mb: Option<u64>,
    /// Total read/write bytes per drive since VM start.
    pub disks: Vec<DiskStat>,
}

#[derive(Serialize, Clone, Debug)]
pub struct DiskStat {
    pub id: String,
    pub path: String,
    pub rd_bytes: u64,
    pub wr_bytes: u64,
    pub rd_ops: u64,
    pub wr_ops: u64,
}

// ---------------------------------------------------------------------------
// Host process CPU time
// ---------------------------------------------------------------------------

/// Total consumed CPU time of a process, in nanoseconds.
/// Unsupported platforms (macOS) return None — CPU% is simply not shown.
pub fn process_cpu_ns(pid: i32) -> Option<u64> {
    #[cfg(windows)]
    {
        windows_cpu_ns(pid)
    }
    #[cfg(target_os = "linux")]
    {
        proc_stat_cpu_ns(pid)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = pid;
        None
    }
}

/// Percent of ONE core between two samples; can exceed 100 on multi-vCPU VMs.
pub fn cpu_pct(cpu_prev_ns: u64, wall_prev: std::time::Instant, cpu_now_ns: u64) -> Option<f64> {
    let wall = wall_prev.elapsed();
    if wall.as_nanos() == 0 {
        return None;
    }
    let d = cpu_now_ns.saturating_sub(cpu_prev_ns) as f64;
    let pct = d * 100.0 / (wall.as_nanos() as f64);
    Some((pct * 10.0).round() / 10.0)
}

#[cfg(windows)]
fn windows_cpu_ns(pid: i32) -> Option<u64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
        if h.is_null() {
            return None;
        }
        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut kernel = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut user = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let ok = GetProcessTimes(h, &mut creation, &mut exit, &mut kernel, &mut user);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        let to_u64 = |f: &FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
        // FILETIME is in 100 ns units.
        Some(to_u64(&kernel).saturating_add(to_u64(&user)) * 100)
    }
}

#[cfg(target_os = "linux")]
fn proc_stat_cpu_ns(pid: i32) -> Option<u64> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm may contain spaces/spaces-in-parens: parse after the last ')'.
    let rest = s.rsplit_once(')').map(|(_, r)| r.trim())?;
    let parts: Vec<&str> = rest.split_whitespace().collect();
    // After ')': state=parts[0]; utime=parts[11], stime=parts[12] (fields 14/15).
    let utime: u64 = parts.get(11)?.parse().ok()?;
    let stime: u64 = parts.get(12)?.parse().ok()?;
    // Standard USER_HZ = 100 -> 10 ms per tick.
    Some((utime + stime) * 10_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_pid_has_no_cpu_sample() {
        assert!(process_cpu_ns(4_000_000).is_none());
    }

    #[test]
    fn cpu_pct_math() {
        let t0 = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(cpu_pct(0, t0, 0), Some(0.0));
        // 50% of one core over an interval.
        let pct = cpu_pct(1_000_000, t0, 1_001_000_000).unwrap_or(0.0);
        assert!(pct > 0.0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn own_process_has_cpu_time_on_linux() {
        let me = std::process::id() as i32;
        assert!(process_cpu_ns(me).is_some());
    }

    #[cfg(windows)]
    #[test]
    fn own_process_has_cpu_time_on_windows() {
        let me = std::process::id() as i32;
        assert!(process_cpu_ns(me).is_some());
    }
}
