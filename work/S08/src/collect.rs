use std::ffi::CString;
use std::time::{Duration, Instant};

use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::model::{select_top_processes, LoadAvg, Metrics, ProcInfo, RateTracker};

/// Stable interface between the sampling loop and the host operating system.
pub trait Collector {
    fn sample(&mut self, elapsed: Duration, want_processes: bool) -> Metrics;
    fn initial_metrics(&self) -> Metrics;
    fn backend(&self) -> &'static str;
    fn reset_baseline(&mut self) {}
}

#[derive(Debug, Clone, Copy)]
struct RootDisk {
    total: u64,
    available: u64,
}

pub struct SysinfoCollector {
    system: System,
    networks: Networks,
    rates: RateTracker,
    process_cache: Vec<ProcInfo>,
    process_count: Option<usize>,
    hostname: Option<String>,
    os_name: Option<String>,
    os_version: Option<String>,
    kernel: Option<String>,
    arch: String,
    disk: Option<RootDisk>,
    last_disk_read: Instant,
    last_process_refresh: Instant,
    process_tick: u64,
    primed_cpu: bool,
    initial: Metrics,
}

impl Default for SysinfoCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl SysinfoCollector {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_memory();
        system.refresh_cpu_all();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );

        let networks = Networks::new_with_refreshed_list();
        let mut rates = RateTracker::new();
        rates.prime(network_totals(&networks));

        let now = Instant::now();
        let disk = read_root_disk();
        let process_cache = collect_processes(&system);
        let process_count = Some(system.processes().len());

        let mut collector = Self {
            system,
            networks,
            rates,
            process_cache,
            process_count,
            hostname: System::host_name(),
            os_name: System::name(),
            os_version: System::os_version(),
            kernel: System::kernel_version(),
            arch: std::env::consts::ARCH.to_string(),
            disk,
            last_disk_read: now,
            last_process_refresh: now,
            process_tick: 0,
            primed_cpu: true,
            initial: Metrics::default(),
        };
        collector.initial = collector.build_metrics(None, None);
        collector
    }

    fn build_metrics(&self, cpu: Option<(f32, Vec<f32>)>, net: Option<(f64, f64)>) -> Metrics {
        let total = self.system.total_memory();
        let available = self.system.available_memory();
        let mem = if total > 0 {
            Some((total, total.saturating_sub(available), available))
        } else {
            None
        };

        let swap_total = self.system.total_swap();
        let swap_used = self.system.used_swap();

        let load = System::load_average();
        let load = if load.one.is_finite()
            && load.five.is_finite()
            && load.fifteen.is_finite()
            && load.one >= 0.0
            && load.five >= 0.0
            && load.fifteen >= 0.0
        {
            Some(LoadAvg {
                one: load.one,
                five: load.five,
                fifteen: load.fifteen,
            })
        } else {
            None
        };

        let (cpu_value, per_core) = match cpu {
            Some((global, cores)) => (Some(global), cores),
            None => (None, Vec::new()),
        };

        Metrics {
            hostname: self.hostname.clone(),
            os_name: self.os_name.clone(),
            os_version: self.os_version.clone(),
            kernel: self.kernel.clone(),
            arch: self.arch.clone(),
            uptime_secs: Some(System::uptime()),
            load,
            cpu: cpu_value,
            per_core,
            mem_total: mem.map(|m| m.0),
            mem_used: mem.map(|m| m.1),
            mem_available: mem.map(|m| m.2),
            swap_total: Some(swap_total),
            swap_used: Some(swap_used.min(swap_total)),
            disk_mount: self.disk.map(|_| "/".to_string()),
            disk_total: self.disk.map(|d| d.total),
            disk_available: self.disk.map(|d| d.available),
            net_rx_rate: net.map(|n| n.0),
            net_tx_rate: net.map(|n| n.1),
            net_ifaces: self.networks.len(),
            process_count: self.process_count,
            top_processes: self.process_cache.clone(),
            collect_error: None,
        }
    }
}

impl Collector for SysinfoCollector {
    fn sample(&mut self, elapsed: Duration, want_processes: bool) -> Metrics {
        self.system.refresh_memory();
        self.system.refresh_cpu_all();

        self.process_tick = self.process_tick.wrapping_add(1);
        let refresh_processes = want_processes
            || self.last_process_refresh.elapsed() >= Duration::from_secs(5)
            || self.process_tick % 5 == 1;
        if refresh_processes {
            self.system.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::nothing().with_cpu().with_memory(),
            );
            self.process_cache = collect_processes(&self.system);
            self.process_count = Some(self.system.processes().len());
            self.last_process_refresh = Instant::now();
        }

        self.networks.refresh(true);
        let net = self.rates.update(network_totals(&self.networks), elapsed);

        if self.last_disk_read.elapsed() >= Duration::from_secs(5) {
            self.disk = read_root_disk();
            self.last_disk_read = Instant::now();
        }

        let global = self.system.global_cpu_usage();
        let cores = self
            .system
            .cpus()
            .iter()
            .map(|c| {
                let usage = c.cpu_usage();
                if usage.is_finite() {
                    usage.clamp(0.0, 100.0)
                } else {
                    0.0
                }
            })
            .collect();

        let cpu = if self.primed_cpu {
            if global.is_finite() {
                Some((global.clamp(0.0, 100.0), cores))
            } else {
                Some((0.0, cores))
            }
        } else {
            self.primed_cpu = true;
            None
        };

        self.build_metrics(cpu, net)
    }

    fn initial_metrics(&self) -> Metrics {
        self.initial.clone()
    }

    fn backend(&self) -> &'static str {
        "sysinfo + statvfs"
    }

    fn reset_baseline(&mut self) {
        self.networks.refresh(true);
        self.rates.prime(network_totals(&self.networks));
        self.primed_cpu = false;
        self.last_process_refresh = Instant::now();
    }
}

fn collect_processes(system: &System) -> Vec<ProcInfo> {
    let procs: Vec<ProcInfo> = system
        .processes()
        .iter()
        .map(|(pid, process)| ProcInfo {
            pid: pid.as_u32(),
            name: process.name().to_string_lossy().into_owned(),
            cpu: process.cpu_usage(),
            memory: process.memory(),
        })
        .collect();
    select_top_processes(procs, 5)
}

fn network_totals(networks: &Networks) -> Vec<(String, u64, u64)> {
    networks
        .iter()
        .map(|(name, data)| {
            (
                name.clone(),
                data.total_received(),
                data.total_transmitted(),
            )
        })
        .collect()
}

#[cfg(unix)]
fn read_root_disk() -> Option<RootDisk> {
    let path = CString::new("/").ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    if rc != 0 {
        return None;
    }
    let block = stat.f_frsize.max(stat.f_bsize) as u64;
    let total = (stat.f_blocks as u64).checked_mul(block)?;
    let available = (stat.f_bavail as u64).checked_mul(block)?;
    Some(RootDisk {
        total,
        available: available.min(total),
    })
}

#[cfg(not(unix))]
fn read_root_disk() -> Option<RootDisk> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_disk_is_plausible_when_available() {
        if let Some(disk) = read_root_disk() {
            assert!(disk.total > 0);
            assert!(disk.available <= disk.total);
        }
    }

    #[test]
    fn collector_exposes_static_data() {
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            return;
        }
        let collector = SysinfoCollector::new();
        let metrics = collector.initial_metrics();
        assert!(!metrics.arch.is_empty());
        assert!(metrics.uptime_secs.is_some());
        assert_eq!(collector.backend(), "sysinfo + statvfs");
    }
}
