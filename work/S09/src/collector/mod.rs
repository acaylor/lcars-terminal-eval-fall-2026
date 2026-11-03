pub mod linux;
pub mod macos;

use std::ffi::CStr;
use std::mem::MaybeUninit;

use crate::model::{LocalClock, RawSystemSample};

/// OS-agnostic interface for system metric collection.
/// Each metric group fails independently and gracefully without crashing the collector.
pub trait MetricCollector {
    fn collect(&mut self, include_processes: bool) -> RawSystemSample;
}

/// Instantiates the native collector for the current operating system.
pub fn create_platform_collector() -> Box<dyn MetricCollector> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxCollector::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOsCollector::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Box::new(FallbackPosixCollector::new())
    }
}

/// Fallback collector for other POSIX systems using standard `libc` calls.
pub struct FallbackPosixCollector {
    hostname: Option<String>,
    os_name: Option<String>,
    arch: Option<String>,
}

impl FallbackPosixCollector {
    pub fn new() -> Self {
        let (os_name, arch) = collect_uname();
        Self {
            hostname: collect_hostname(),
            os_name,
            arch,
        }
    }
}

impl Default for FallbackPosixCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricCollector for FallbackPosixCollector {
    fn collect(&mut self, _include_processes: bool) -> RawSystemSample {
        let (disk_total, disk_used, disk_avail) = collect_root_disk();
        RawSystemSample {
            clock: collect_local_clock(),
            hostname: self.hostname.clone().or_else(collect_hostname),
            os_name: self.os_name.clone(),
            arch: self.arch.clone(),
            load_avg: collect_loadavg(),
            disk_root_total_bytes: disk_total,
            disk_root_used_bytes: disk_used,
            disk_root_avail_bytes: disk_avail,
            ..RawSystemSample::default()
        }
    }
}

/// Reads local wall-clock time and date via POSIX `time` and `localtime_r`.
pub fn collect_local_clock() -> LocalClock {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        if now == -1 {
            return LocalClock::default();
        }
        let mut tm = MaybeUninit::<libc::tm>::zeroed();
        if libc::localtime_r(&now, tm.as_mut_ptr()).is_null() {
            return LocalClock::default();
        }
        let tm = tm.assume_init();
        LocalClock {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1).clamp(1, 12) as u32,
            day: tm.tm_mday.clamp(1, 31) as u32,
            hour: tm.tm_hour.clamp(0, 23) as u32,
            minute: tm.tm_min.clamp(0, 59) as u32,
            second: tm.tm_sec.clamp(0, 60) as u32,
            day_of_year: (tm.tm_yday + 1).clamp(1, 366) as u32,
        }
    }
}

/// Reads system hostname via POSIX `gethostname`.
pub fn collect_hostname() -> Option<String> {
    let mut buf = [0u8; 256];
    let res = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if res != 0 {
        return None;
    }
    buf[buf.len() - 1] = 0;
    let cstr = unsafe { CStr::from_ptr(buf.as_ptr() as *const libc::c_char) };
    let s = cstr.to_string_lossy().trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Reads operating system name, release, and CPU architecture via POSIX `uname`.
pub fn collect_uname() -> (Option<String>, Option<String>) {
    unsafe {
        let mut uts = MaybeUninit::<libc::utsname>::zeroed();
        if libc::uname(uts.as_mut_ptr()) != 0 {
            return (None, None);
        }
        let uts = uts.assume_init();
        let sysname = CStr::from_ptr(uts.sysname.as_ptr())
            .to_string_lossy()
            .trim()
            .to_string();
        let release = CStr::from_ptr(uts.release.as_ptr())
            .to_string_lossy()
            .trim()
            .to_string();
        let machine = CStr::from_ptr(uts.machine.as_ptr())
            .to_string_lossy()
            .trim()
            .to_string();

        let os = if sysname.is_empty() {
            None
        } else if release.is_empty() {
            Some(sysname)
        } else {
            Some(format!("{} {}", sysname, release))
        };
        let arch = if machine.is_empty() {
            None
        } else {
            Some(machine)
        };
        (os, arch)
    }
}

/// Reads 1m, 5m, and 15m load averages via `getloadavg`.
pub fn collect_loadavg() -> Option<[f64; 3]> {
    let mut loads = [0.0f64; 3];
    let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    if n == 3 && loads.iter().all(|v| v.is_finite() && *v >= 0.0) {
        Some(loads)
    } else {
        None
    }
}

/// Reads root filesystem (`/`) total, used, and available bytes via POSIX `statvfs`.
#[allow(clippy::unnecessary_cast)]
pub fn collect_root_disk() -> (Option<u64>, Option<u64>, Option<u64>) {
    unsafe {
        let mut stat = MaybeUninit::<libc::statvfs>::zeroed();
        let path = b"/\0";
        if libc::statvfs(path.as_ptr() as *const libc::c_char, stat.as_mut_ptr()) != 0 {
            return (None, None, None);
        }
        let stat = stat.assume_init();
        let frsize = if stat.f_frsize > 0 {
            stat.f_frsize as u64
        } else {
            stat.f_bsize as u64
        };
        let total = (stat.f_blocks as u64).saturating_mul(frsize);
        let free = (stat.f_bfree as u64).saturating_mul(frsize);
        let avail = (stat.f_bavail as u64).saturating_mul(frsize);
        if total == 0 {
            return (None, None, None);
        }
        let used = total.saturating_sub(free);
        (Some(total), Some(used), Some(avail))
    }
}
