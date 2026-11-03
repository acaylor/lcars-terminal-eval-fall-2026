//! Metric collection, kept entirely separate from presentation.
//!
//! * `Collector` is the stable interface each OS implements with raw counters.
//! * `Sampler` turns consecutive raw `Snapshot`s into presentation-ready
//!   `Telemetry` (percentages, rates, histories) and is fully deterministic.
//!
//! Every metric is optional: a collector that cannot read something reports
//! `None` and the views draw an unavailable state instead of a number.

pub mod rates;
pub mod sampler;
pub mod units;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;

use std::time::{Duration, Instant};

/// Cumulative CPU time split into busy and idle. Units are opaque ticks;
/// only differences between two samples are meaningful.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTicks {
    pub busy: u64,
    pub idle: u64,
}

impl CpuTicks {
    pub fn total(&self) -> u64 {
        self.busy.saturating_add(self.idle)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub used: u64,
    pub available: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Swap {
    pub total: u64,
    pub used: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Disk {
    pub mount: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
}

/// Cumulative byte counters of one interface. Loopback is excluded by the
/// collectors so the headline rate reflects real traffic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Interface {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessSample {
    pub pid: i32,
    pub name: String,
    /// Cumulative user+system CPU time.
    pub cpu_time: Duration,
    pub rss: u64,
}

/// One raw reading. `at` is the monotonic time the reading was taken, which
/// is what every rate calculation uses (never the nominal refresh interval).
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub at: Instant,
    pub cpu_total: Option<CpuTicks>,
    pub cpu_cores: Vec<CpuTicks>,
    pub load: Option<[f64; 3]>,
    pub memory: Option<Memory>,
    pub swap: Option<Swap>,
    pub disk: Option<Disk>,
    pub interfaces: Option<Vec<Interface>>,
    pub uptime: Option<Duration>,
    pub process_count: Option<usize>,
    pub processes: Option<Vec<ProcessSample>>,
}

impl Snapshot {
    pub fn empty(at: Instant) -> Snapshot {
        Snapshot {
            at,
            cpu_total: None,
            cpu_cores: Vec::new(),
            load: None,
            memory: None,
            swap: None,
            disk: None,
            interfaces: None,
            uptime: None,
            process_count: None,
            processes: None,
        }
    }
}

/// Facts that do not change while the program runs.
#[derive(Clone, Debug, Default)]
pub struct StaticInfo {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub arch: String,
    pub cores: usize,
}

/// The interface OS-specific collectors implement.
pub trait Collector {
    fn static_info(&self) -> StaticInfo;
    /// Take a reading. Process scanning is comparatively expensive, so it is
    /// only requested while the diagnostic view is visible.
    fn sample(&mut self, want_processes: bool) -> Snapshot;
    /// Human-readable notes about metrics that could not be collected.
    fn warnings(&self) -> Vec<String>;
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
}

/// Hostname through the C library, which works identically on both platforms.
pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if rc != 0 {
        return "UNKNOWN".into();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

pub fn uname_release() -> String {
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut u) } != 0 {
        return String::new();
    }
    cstr_to_string(&u.release)
}

pub fn cstr_to_string(bytes: &[libc::c_char]) -> String {
    let b: Vec<u8> = bytes.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&b).into_owned()
}
