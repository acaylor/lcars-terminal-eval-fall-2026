use std::collections::BTreeMap;

/// Raw cumulative CPU tick counters for either overall CPU or a single logical core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTimes {
    pub active: u64,
    pub total: u64,
}

impl CpuTimes {
    pub fn utilization_delta(&self, prev: &CpuTimes) -> Option<f32> {
        if self.total < prev.total || self.active < prev.active {
            // Counter rollover or reset
            return None;
        }
        let d_total = self.total - prev.total;
        let d_active = self.active - prev.active;
        if d_total == 0 {
            return None;
        }
        let pct = (d_active as f64 / d_total as f64) * 100.0;
        Some(pct.clamp(0.0, 100.0) as f32)
    }
}

/// Raw network byte counters for a single non-loopback network interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RawNetBytes {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Raw process sample before delta rate calculation.
#[derive(Debug, Clone, PartialEq)]
pub struct RawProcessSample {
    pub pid: u32,
    pub name: String,
    /// Cumulative CPU time in nanoseconds (or converted from clock ticks).
    pub cpu_time_ns: u64,
    /// Resident set size in bytes.
    pub memory_bytes: u64,
    /// Optional process start time (seconds since boot) for first-frame estimation.
    pub start_uptime_secs: Option<f64>,
}

/// Local wall-clock time and date read via POSIX `localtime_r`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalClock {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub day_of_year: u32,
}

impl Default for LocalClock {
    fn default() -> Self {
        Self {
            year: 2026,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            day_of_year: 1,
        }
    }
}

impl LocalClock {
    pub fn format_time(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }

    pub fn format_date(&self) -> String {
        format!("{:04}.{:-02}.{:-02}", self.year, self.month, self.day)
    }

    /// Computes a deterministic Starfleet-style stardate from local date and time.
    pub fn stardate(&self) -> f64 {
        let day_fraction =
            (self.hour as f64 * 3600.0 + self.minute as f64 * 60.0 + self.second as f64) / 86400.0;
        let year_progress = (self.day_of_year.saturating_sub(1) as f64 + day_fraction) / 365.25;
        let base = 47000.0 + ((self.year - 2020).max(0) as f64) * 1000.0;
        base + year_progress * 1000.0
    }
}

/// Raw OS snapshot produced by a platform collector (`LinuxCollector` or `MacOsCollector`).
#[derive(Debug, Clone, Default)]
pub struct RawSystemSample {
    pub clock: LocalClock,
    pub hostname: Option<String>,
    pub os_name: Option<String>,
    pub arch: Option<String>,
    pub uptime_secs: Option<u64>,
    pub cpu_total: Option<CpuTimes>,
    pub cpu_cores: Vec<CpuTimes>,
    pub load_avg: Option<[f64; 3]>,
    pub mem_total_bytes: Option<u64>,
    pub mem_avail_bytes: Option<u64>,
    pub swap_total_bytes: Option<u64>,
    pub swap_used_bytes: Option<u64>,
    pub disk_root_total_bytes: Option<u64>,
    pub disk_root_used_bytes: Option<u64>,
    pub disk_root_avail_bytes: Option<u64>,
    pub net_interfaces: BTreeMap<String, RawNetBytes>,
    pub process_count: Option<usize>,
    pub processes: Vec<RawProcessSample>,
}

/// Memory telemetry ready for presentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoryMetrics {
    pub used_bytes: u64,
    pub avail_bytes: u64,
    pub total_bytes: u64,
    pub used_pct: f32,
}

/// Swap telemetry ready for presentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwapMetrics {
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub used_pct: f32,
}

/// Root filesystem telemetry ready for presentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiskMetrics {
    pub used_bytes: u64,
    pub avail_bytes: u64,
    pub total_bytes: u64,
    pub used_pct: f32,
}

/// Network rate telemetry ready for presentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetworkMetrics {
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub active_interfaces: usize,
}

/// Process entry for the top-5 CPU diagnostic view.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub memory_bytes: u64,
}

/// Overall system status classification for focal LCARS indicators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAlertState {
    Nominal,
    Elevated,
    Critical,
}

impl SystemAlertState {
    pub fn label(&self) -> &'static str {
        match self {
            SystemAlertState::Nominal => "NOMINAL",
            SystemAlertState::Elevated => "ELEVATED",
            SystemAlertState::Critical => "ALERT",
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            SystemAlertState::Nominal => "COND-GREEN // 01",
            SystemAlertState::Elevated => "COND-AMBER // 02",
            SystemAlertState::Critical => "COND-RED   // 03",
        }
    }
}

/// Complete processed telemetry state consumed by the UI renderer.
#[derive(Debug, Clone)]
pub struct SystemTelemetry {
    pub tick: u64,
    pub clock: LocalClock,
    pub hostname: Option<String>,
    pub os_name: Option<String>,
    pub arch: Option<String>,
    pub uptime_secs: Option<u64>,
    pub cpu_overall_pct: Option<f32>,
    pub cpu_cores_pct: Vec<f32>,
    pub load_avg: Option<[f64; 3]>,
    pub memory: Option<MemoryMetrics>,
    pub swap: Option<SwapMetrics>,
    pub disk_root: Option<DiskMetrics>,
    pub network: Option<NetworkMetrics>,
    pub process_count: Option<usize>,
    pub top_processes: Vec<ProcessEntry>,
    pub cpu_history: Vec<f32>,
    pub net_rx_history: Vec<f64>,
    pub net_tx_history: Vec<f64>,
    pub alert_state: SystemAlertState,
}

impl Default for SystemTelemetry {
    fn default() -> Self {
        Self {
            tick: 0,
            clock: LocalClock::default(),
            hostname: None,
            os_name: None,
            arch: None,
            uptime_secs: None,
            cpu_overall_pct: None,
            cpu_cores_pct: Vec::new(),
            load_avg: None,
            memory: None,
            swap: None,
            disk_root: None,
            network: None,
            process_count: None,
            top_processes: Vec::new(),
            cpu_history: Vec::new(),
            net_rx_history: Vec::new(),
            net_tx_history: Vec::new(),
            alert_state: SystemAlertState::Nominal,
        }
    }
}

/// Formats byte counts into compact engineering notation (`GiB`, `MiB`, `KiB`, `B`).
pub fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const TIB: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= TIB {
        format!("{:.2} TiB", b / TIB)
    } else if b >= GIB {
        format!("{:.1} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.1} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.0} KiB", b / KIB)
    } else {
        format!("{} B", bytes)
    }
}

/// Formats byte-per-second transfer rates (`MB/s`, `KB/s`, `B/s`).
pub fn format_rate(bytes_per_sec: f64) -> String {
    if !bytes_per_sec.is_finite() || bytes_per_sec < 0.0 {
        return "0 B/s".to_string();
    }
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    if bytes_per_sec >= GB {
        format!("{:.2} GB/s", bytes_per_sec / GB)
    } else if bytes_per_sec >= MB {
        format!("{:.2} MB/s", bytes_per_sec / MB)
    } else if bytes_per_sec >= KB {
        format!("{:.1} KB/s", bytes_per_sec / KB)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

/// Formats system uptime in seconds into `DDd HH:MM:SS` or `HH:MM:SS`.
pub fn format_uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hours = rem / 3600;
    let mins = (rem % 3600) / 60;
    let s = rem % 60;
    if days > 0 {
        format!("{}d {:02}:{:02}:{:02}", days, hours, mins, s)
    } else {
        format!("{:02}:{:02}:{:02}", hours, mins, s)
    }
}

/// Truncates a UTF-8 string safely to at most `max_chars` characters without panicking on multi-byte codepoints.
pub fn truncate_chars(input: &str, max_chars: usize, unicode_ellipsis: bool) -> String {
    if max_chars == 0 {
        return String::new();
    }
    // Sanitize control characters so stray newlines/tabs in process names cannot corrupt terminal output
    let sanitized: String = input
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect();
    let count = sanitized.chars().count();
    if count <= max_chars {
        return sanitized;
    }
    if unicode_ellipsis {
        if max_chars == 1 {
            return "…".to_string();
        }
        let mut s: String = sanitized.chars().take(max_chars - 1).collect();
        s.push('…');
        s
    } else {
        if max_chars <= 2 {
            return sanitized.chars().take(max_chars).collect();
        }
        let mut s: String = sanitized.chars().take(max_chars - 2).collect();
        s.push_str("..");
        s
    }
}
