mod parse;
mod rate;
mod unix;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

pub use parse::{
    parse_loadavg, parse_meminfo, parse_net_dev, parse_os_release, parse_proc_pid_stat,
    parse_proc_stat, parse_uptime,
};
pub use rate::{cpu_percent, iface_rates, process_cpu_ns, process_cpu_ticks, IfaceSnap};

use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub hostname: String,
    pub os_name: String,
    pub arch: String,
    pub time: CivilTime,
    pub uptime: Option<Duration>,
    pub cpu_pct: Option<f64>,
    pub per_core: Vec<Option<f64>>,
    pub load: Option<[f64; 3]>,
    pub mem_used: Option<u64>,
    pub mem_available: Option<u64>,
    pub mem_total: Option<u64>,
    pub swap_used: Option<u64>,
    pub swap_total: Option<u64>,
    pub disk_used: Option<u64>,
    pub disk_total: Option<u64>,
    pub net_rx_bps: Option<f64>,
    pub net_tx_bps: Option<f64>,
    pub process_count: Option<usize>,
    pub top_cpu: Vec<ProcessRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: Option<f64>,
    pub mem_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CivilTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub min: u8,
    pub sec: u8,
}

impl CivilTime {
    pub fn hms(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.min, self.sec)
    }

    pub fn date_label(&self) -> String {
        const MONTHS: [&str; 12] = [
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
        ];
        let name = MONTHS
            .get(self.month.wrapping_sub(1) as usize)
            .copied()
            .unwrap_or("???");
        format!("{:02} {name} {}", self.day, self.year)
    }
}

impl Sample {
    pub fn unavailable(
        hostname: impl Into<String>,
        os_name: impl Into<String>,
        arch: impl Into<String>,
    ) -> Self {
        Self {
            hostname: hostname.into(),
            os_name: os_name.into(),
            arch: arch.into(),
            time: unix::civil_now(),
            uptime: None,
            cpu_pct: None,
            per_core: Vec::new(),
            load: None,
            mem_used: None,
            mem_available: None,
            mem_total: None,
            swap_used: None,
            swap_total: None,
            disk_used: None,
            disk_total: None,
            net_rx_bps: None,
            net_tx_bps: None,
            process_count: None,
            top_cpu: Vec::new(),
        }
    }
}

pub struct Collector {
    #[cfg(target_os = "linux")]
    inner: linux::LinuxCollector,
    #[cfg(target_os = "macos")]
    inner: macos::MacCollector,
}

impl Collector {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            inner: linux::LinuxCollector::new(),
            #[cfg(target_os = "macos")]
            inner: macos::MacCollector::new(),
        }
    }

    pub fn platform() -> &'static str {
        if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        }
    }

    pub fn sample(&mut self, processes: bool) -> Sample {
        #[cfg(target_os = "linux")]
        {
            self.inner.sample(processes)
        }
        #[cfg(target_os = "macos")]
        {
            self.inner.sample(processes)
        }
    }
}

impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}

pub fn demo_sample() -> Sample {
    Sample {
        hostname: "bridge-ops".into(),
        os_name: "Linux 6.8".into(),
        arch: "x86_64".into(),
        time: CivilTime {
            year: 2026,
            month: 9,
            day: 22,
            hour: 21,
            min: 14,
            sec: 7,
        },
        uptime: Some(Duration::from_secs(4 * 86400 + 2 * 3600 + 17 * 60 + 6)),
        cpu_pct: Some(18.4),
        per_core: vec![
            Some(42.0),
            Some(11.0),
            Some(7.5),
            Some(88.0),
            Some(3.0),
            Some(15.0),
            Some(22.0),
            Some(9.0),
            Some(61.0),
            Some(4.0),
            Some(18.0),
            Some(27.0),
            Some(2.0),
            Some(33.0),
            Some(12.0),
            Some(6.0),
            Some(54.0),
            Some(8.0),
            Some(19.0),
            Some(5.0),
            Some(41.0),
            Some(14.0),
            Some(9.5),
            Some(3.5),
        ],
        load: Some([0.42, 0.31, 0.22]),
        mem_used: Some(8_200_000_000),
        mem_available: Some(7_400_000_000),
        mem_total: Some(16_000_000_000),
        swap_used: Some(512_000_000),
        swap_total: Some(8_000_000_000),
        disk_used: Some(180_000_000_000),
        disk_total: Some(512_000_000_000),
        net_rx_bps: Some(1_250_000.0),
        net_tx_bps: Some(84_000.0),
        process_count: Some(248),
        top_cpu: vec![
            ProcessRow {
                pid: 1420,
                name: "warp-core".into(),
                cpu_pct: Some(24.6),
                mem_bytes: Some(180_000_000),
            },
            ProcessRow {
                pid: 88,
                name: "plasma-injector".into(),
                cpu_pct: Some(11.2),
                mem_bytes: Some(64_000_000),
            },
            ProcessRow {
                pid: 3311,
                name: "sensor-array".into(),
                cpu_pct: Some(6.4),
                mem_bytes: Some(42_000_000),
            },
            ProcessRow {
                pid: 12,
                name: "nav-comp".into(),
                cpu_pct: Some(3.1),
                mem_bytes: Some(28_000_000),
            },
            ProcessRow {
                pid: 904,
                name: "ユニコード".into(),
                cpu_pct: Some(1.8),
                mem_bytes: Some(12_000_000),
            },
        ],
    }
}

pub fn top_processes(mut rows: Vec<ProcessRow>) -> Vec<ProcessRow> {
    rows.sort_by(|a, b| {
        let ca = a.cpu_pct.unwrap_or(f64::NEG_INFINITY);
        let cb = b.cpu_pct.unwrap_or(f64::NEG_INFINITY);
        cb.partial_cmp(&ca)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.mem_bytes.unwrap_or(0).cmp(&a.mem_bytes.unwrap_or(0)))
            .then_with(|| a.pid.cmp(&b.pid))
    });
    rows.truncate(5);
    rows
}
