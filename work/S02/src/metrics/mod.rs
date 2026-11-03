//! Metric collection. Collectors return raw counters; the platform-neutral
//! [`sampler::Sampler`] turns consecutive raw samples into rates and
//! percentages. Every field is individually fallible so one broken source
//! never takes down the display.

pub mod history;
pub mod sampler;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub mod unsupported;

use std::time::Duration;

/// Why a metric has no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Missing {
    /// The platform does not expose this metric.
    Unsupported,
    /// The OS refused access (e.g. other users' processes on macOS).
    Denied,
    /// Reading or parsing failed.
    Failed,
    /// A rate needs a second sample; not an error.
    Pending,
}

impl Missing {
    pub fn short(self) -> &'static str {
        match self {
            Missing::Unsupported => "N/A",
            Missing::Denied => "DENIED",
            Missing::Failed => "FAULT",
            Missing::Pending => "SYNC",
        }
    }
}

pub type Reading<T> = Result<T, Missing>;

impl From<std::io::Error> for Missing {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::PermissionDenied => Missing::Denied,
            std::io::ErrorKind::NotFound | std::io::ErrorKind::Unsupported => Missing::Unsupported,
            _ => Missing::Failed,
        }
    }
}

/// Cumulative CPU time split into busy and idle ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTicks {
    pub busy: u64,
    pub idle: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub available: u64,
}

impl Memory {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
    pub fn used_fraction(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.used() as f64 / self.total as f64
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swap {
    pub total: u64,
    pub used: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disk {
    /// Mount point shown to the user.
    pub mount: String,
    pub total: u64,
    pub used: u64,
    /// Space available to unprivileged users.
    pub available: u64,
}

impl Disk {
    /// Same definition as `df`: used / (used + available).
    pub fn used_fraction(&self) -> f64 {
        let denom = self.used + self.available;
        if denom == 0 {
            0.0
        } else {
            self.used as f64 / denom as f64
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfaceCounters {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    /// Counter modulus for interfaces with narrow (e.g. 32-bit) counters;
    /// `None` for 64-bit counters. Used to tell wrap-around from reset.
    pub modulus: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcRaw {
    pub pid: u32,
    /// Start time in platform units; disambiguates reused PIDs.
    pub start: u64,
    pub name: String,
    /// Cumulative CPU time (user + system) in nanoseconds.
    pub cpu_ns: u64,
    pub rss_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ProcTable {
    /// All processes the OS reported, readable or not.
    pub total: usize,
    /// Processes that vanished or refused inspection during this sample.
    pub unreadable: usize,
    pub procs: Vec<ProcRaw>,
}

/// One instantaneous reading of every counter.
#[derive(Clone, Debug)]
pub struct RawSample {
    pub hostname: Reading<String>,
    pub uptime: Reading<Duration>,
    pub load: Reading<[f64; 3]>,
    pub cpu: Reading<CpuTicks>,
    pub cores: Reading<Vec<CpuTicks>>,
    pub memory: Reading<Memory>,
    pub swap: Reading<Swap>,
    pub disk: Reading<Disk>,
    pub net: Reading<Vec<IfaceCounters>>,
    /// `None` when the caller did not ask for the (more expensive) process scan.
    pub procs: Option<Reading<ProcTable>>,
}

impl RawSample {
    pub fn empty(why: Missing) -> RawSample {
        RawSample {
            hostname: Err(why),
            uptime: Err(why),
            load: Err(why),
            cpu: Err(why),
            cores: Err(why),
            memory: Err(why),
            swap: Err(why),
            disk: Err(why),
            net: Err(why),
            procs: None,
        }
    }
}

/// Static facts gathered once at startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemInfo {
    /// e.g. "Ubuntu 24.04.1 LTS" or "macOS 15.2".
    pub os: String,
    /// Kernel name and release, e.g. "Linux 6.8.0".
    pub kernel: String,
    pub arch: String,
}

pub trait Collector {
    fn system_info(&mut self) -> SystemInfo;
    fn sample(&mut self, want_procs: bool) -> RawSample;
    /// A longer display name than the scan provides (Linux truncates `comm`
    /// to 15 bytes). Only called for the handful of processes shown.
    fn full_name(&mut self, _pid: u32) -> Option<String> {
        None
    }
}

pub fn platform_collector() -> Box<dyn Collector> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxCollector::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacCollector::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Box::new(unsupported::UnsupportedCollector)
    }
}

/// Portable helpers shared by the Unix collectors.
pub mod common {
    use super::*;
    use std::ffi::CStr;

    pub fn hostname() -> Reading<String> {
        let mut buf = [0u8; 256];
        // SAFETY: buffer and length are valid; gethostname NUL-terminates on success.
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if rc != 0 {
            return Err(Missing::Failed);
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let s = String::from_utf8_lossy(&buf[..end]).into_owned();
        if s.is_empty() {
            Err(Missing::Failed)
        } else {
            Ok(s)
        }
    }

    pub fn load() -> Reading<[f64; 3]> {
        let mut l = [0f64; 3];
        // SAFETY: l has room for 3 doubles.
        let n = unsafe { libc::getloadavg(l.as_mut_ptr(), 3) };
        if n == 3 {
            Ok(l)
        } else {
            Err(Missing::Unsupported)
        }
    }

    pub fn disk(path: &str, mount_label: &str) -> Reading<Disk> {
        let c = std::ffi::CString::new(path).map_err(|_| Missing::Failed)?;
        // SAFETY: zeroed statvfs is a valid out-parameter.
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let frsize = if s.f_frsize > 0 { s.f_frsize as u64 } else { s.f_bsize as u64 };
        let blocks = s.f_blocks as u64;
        let bfree = s.f_bfree as u64;
        let bavail = s.f_bavail as u64;
        Ok(Disk { mount: mount_label.to_string(), total: blocks * frsize, used: blocks.saturating_sub(bfree) * frsize, available: bavail * frsize })
    }

    /// (sysname, release, machine) from uname(2).
    pub fn uname() -> (String, String, String) {
        // SAFETY: zeroed utsname is a valid out-parameter.
        let mut u: libc::utsname = unsafe { std::mem::zeroed() };
        if unsafe { libc::uname(&mut u) } != 0 {
            return ("Unix".into(), "?".into(), std::env::consts::ARCH.into());
        }
        let f = |a: &[libc::c_char]| unsafe { CStr::from_ptr(a.as_ptr()) }.to_string_lossy().into_owned();
        (f(&u.sysname), f(&u.release), f(&u.machine))
    }
}
