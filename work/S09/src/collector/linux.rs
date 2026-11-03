use std::collections::BTreeMap;
use std::fs;

use crate::collector::{
    collect_hostname, collect_loadavg, collect_local_clock, collect_root_disk, collect_uname,
    MetricCollector,
};
use crate::model::{CpuTimes, RawNetBytes, RawProcessSample, RawSystemSample};

/// Native Linux collector reading directly from `/proc` and POSIX `libc` APIs.
pub struct LinuxCollector {
    hostname: Option<String>,
    os_name: Option<String>,
    arch: Option<String>,
    clk_tck: u64,
    page_size: u64,
}

impl LinuxCollector {
    pub fn new() -> Self {
        let (uname_os, arch) = collect_uname();
        let pretty_os = fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|s| parse_os_release(&s))
            .or(uname_os);

        let clk_tck = unsafe {
            let v = libc::sysconf(libc::_SC_CLK_TCK);
            if v > 0 {
                v as u64
            } else {
                100
            }
        };
        let page_size = unsafe {
            let v = libc::sysconf(libc::_SC_PAGESIZE);
            if v > 0 {
                v as u64
            } else {
                4096
            }
        };

        Self {
            hostname: collect_hostname(),
            os_name: pretty_os,
            arch,
            clk_tck,
            page_size,
        }
    }

    fn collect_processes(&self) -> (Option<usize>, Vec<RawProcessSample>) {
        let entries = match fs::read_dir("/proc") {
            Ok(e) => e,
            Err(_) => return (None, Vec::new()),
        };

        let mut procs = Vec::with_capacity(256);
        let mut count = 0usize;

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(name_str) = file_name.to_str() else {
                continue;
            };
            if !name_str.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let Ok(pid) = name_str.parse::<u32>() else {
                continue;
            };
            count += 1;

            let stat_path = format!("/proc/{}/stat", pid);
            // Process may disappear or be unreadable; ignore errors per process
            let Ok(stat_bytes) = fs::read(&stat_path) else {
                continue;
            };
            let stat_str = String::from_utf8_lossy(&stat_bytes);
            if let Some(sample) = parse_proc_pid_stat(&stat_str, self.clk_tck, self.page_size) {
                procs.push(sample);
            }
        }

        (Some(count), procs)
    }
}

impl Default for LinuxCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricCollector for LinuxCollector {
    fn collect(&mut self, include_processes: bool) -> RawSystemSample {
        let clock = collect_local_clock();
        if self.hostname.is_none() {
            self.hostname = collect_hostname();
        }

        let uptime_secs = fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| parse_proc_uptime(&s));

        let (cpu_total, cpu_cores) = fs::read_to_string("/proc/stat")
            .ok()
            .map(|s| parse_proc_stat(&s))
            .unwrap_or((None, Vec::new()));

        let (mem_total, mem_avail, swap_total, swap_used) = fs::read_to_string("/proc/meminfo")
            .ok()
            .map(|s| parse_proc_meminfo(&s))
            .unwrap_or((None, None, None, None));

        let (disk_total, disk_used, disk_avail) = collect_root_disk();

        let net_interfaces = fs::read_to_string("/proc/net/dev")
            .ok()
            .map(|s| parse_proc_net_dev(&s))
            .unwrap_or_default();

        let (process_count, processes) = if include_processes {
            self.collect_processes()
        } else {
            (None, Vec::new())
        };

        RawSystemSample {
            clock,
            hostname: self.hostname.clone(),
            os_name: self.os_name.clone(),
            arch: self.arch.clone(),
            uptime_secs,
            cpu_total,
            cpu_cores,
            load_avg: collect_loadavg(),
            mem_total_bytes: mem_total,
            mem_avail_bytes: mem_avail,
            swap_total_bytes: swap_total,
            swap_used_bytes: swap_used,
            disk_root_total_bytes: disk_total,
            disk_root_used_bytes: disk_used,
            disk_root_avail_bytes: disk_avail,
            net_interfaces,
            process_count,
            processes,
        }
    }
}

/// Parses `/etc/os-release` to extract `PRETTY_NAME` or `NAME`.
pub fn parse_os_release(content: &str) -> Option<String> {
    let mut name = None;
    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
            let clean = val.trim_matches('"').trim_matches('\'').trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        } else if let Some(val) = line.strip_prefix("NAME=") {
            let clean = val.trim_matches('"').trim_matches('\'').trim();
            if !clean.is_empty() {
                name = Some(clean.to_string());
            }
        }
    }
    name
}

/// Parses `/proc/uptime` content into whole uptime seconds.
pub fn parse_proc_uptime(content: &str) -> Option<u64> {
    let first = content.split_whitespace().next()?;
    let secs: f64 = first.parse().ok()?;
    if secs.is_finite() && secs >= 0.0 {
        Some(secs as u64)
    } else {
        None
    }
}

/// Parses `/proc/stat` into overall `CpuTimes` and per-core `Vec<CpuTimes>`.
pub fn parse_proc_stat(content: &str) -> (Option<CpuTimes>, Vec<CpuTimes>) {
    let mut overall = None;
    let mut cores = Vec::new();

    for line in content.lines() {
        let line = line.trim_start();
        if !line.starts_with("cpu") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(label) = parts.next() else {
            continue;
        };
        if label != "cpu" && !label[3..].bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }

        let vals: Vec<u64> = parts.filter_map(|p| p.parse::<u64>().ok()).collect();
        if vals.len() < 4 {
            continue;
        }
        let user = vals[0];
        let nice = vals[1];
        let system = vals[2];
        let idle = vals[3];
        let iowait = vals.get(4).copied().unwrap_or(0);
        let irq = vals.get(5).copied().unwrap_or(0);
        let softirq = vals.get(6).copied().unwrap_or(0);
        let steal = vals.get(7).copied().unwrap_or(0);

        let active = user
            .saturating_add(nice)
            .saturating_add(system)
            .saturating_add(irq)
            .saturating_add(softirq)
            .saturating_add(steal);
        let total = active.saturating_add(idle).saturating_add(iowait);
        let times = CpuTimes { active, total };

        if label == "cpu" {
            overall = Some(times);
        } else {
            cores.push(times);
        }
    }

    (overall, cores)
}

/// Parses `/proc/meminfo` into `(mem_total, mem_avail, swap_total, swap_used)` in bytes.
pub fn parse_proc_meminfo(
    content: &str,
) -> (Option<u64>, Option<u64>, Option<u64>, Option<u64>) {
    let mut mem_total_kb: Option<u64> = None;
    let mut mem_avail_kb: Option<u64> = None;
    let mut mem_free_kb: Option<u64> = None;
    let mut buffers_kb: u64 = 0;
    let mut cached_kb: u64 = 0;
    let mut sreclaimable_kb: u64 = 0;
    let mut shmem_kb: u64 = 0;
    let mut swap_total_kb: Option<u64> = None;
    let mut swap_free_kb: Option<u64> = None;

    for line in content.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let Some(val_str) = rest.split_whitespace().next() else {
            continue;
        };
        let Ok(val) = val_str.parse::<u64>() else {
            continue;
        };
        match key.trim() {
            "MemTotal" => mem_total_kb = Some(val),
            "MemAvailable" => mem_avail_kb = Some(val),
            "MemFree" => mem_free_kb = Some(val),
            "Buffers" => buffers_kb = val,
            "Cached" => cached_kb = val,
            "SReclaimable" => sreclaimable_kb = val,
            "Shmem" => shmem_kb = val,
            "SwapTotal" => swap_total_kb = Some(val),
            "SwapFree" => swap_free_kb = Some(val),
            _ => {}
        }
    }

    let mem_total = mem_total_kb.map(|kb| kb.saturating_mul(1024));
    let mem_avail = mem_avail_kb
        .or_else(|| {
            mem_free_kb.map(|free| {
                free.saturating_add(buffers_kb)
                    .saturating_add(cached_kb)
                    .saturating_add(sreclaimable_kb)
                    .saturating_sub(shmem_kb)
            })
        })
        .map(|kb| kb.saturating_mul(1024));

    let swap_total = swap_total_kb.map(|kb| kb.saturating_mul(1024));
    let swap_used = match (swap_total_kb, swap_free_kb) {
        (Some(tot), Some(free)) => Some(tot.saturating_sub(free).saturating_mul(1024)),
        _ => None,
    };

    (mem_total, mem_avail, swap_total, swap_used)
}

/// Parses `/proc/net/dev` into a map of active non-loopback interface names to `RawNetBytes`.
pub fn parse_proc_net_dev(content: &str) -> BTreeMap<String, RawNetBytes> {
    let mut map = BTreeMap::new();
    for line in content.lines() {
        let Some((iface, stats)) = line.split_once(':') else {
            continue;
        };
        let iface = iface.trim();
        if iface.is_empty() || iface == "lo" {
            continue;
        }
        let cols: Vec<&str> = stats.split_whitespace().collect();
        if cols.len() < 9 {
            continue;
        }
        let Ok(rx_bytes) = cols[0].parse::<u64>() else {
            continue;
        };
        let Ok(tx_bytes) = cols[8].parse::<u64>() else {
            continue;
        };
        map.insert(iface.to_string(), RawNetBytes { rx_bytes, tx_bytes });
    }
    map
}

/// Parses a single `/proc/[pid]/stat` line safely, even when the command name `comm`
/// contains spaces, parentheses, or non-ASCII UTF-8 characters.
pub fn parse_proc_pid_stat(
    content: &str,
    clk_tck: u64,
    page_size: u64,
) -> Option<RawProcessSample> {
    let open_paren = content.find('(')?;
    let close_paren = content.rfind(')')?;
    if close_paren <= open_paren {
        return None;
    }

    let pid_str = content[..open_paren].trim();
    let pid: u32 = pid_str.parse().ok()?;
    let name = content[open_paren + 1..close_paren].to_string();

    // Fields after `)` start at field 3 (`state`)
    let rest = content[close_paren + 1..].trim();
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // utime is field 14 (index 11 in `fields`), stime is field 15 (index 12),
    // starttime is field 22 (index 19), rss is field 24 (index 21)
    if fields.len() <= 21 {
        return None;
    }

    let utime_ticks: u64 = fields[11].parse().ok()?;
    let stime_ticks: u64 = fields[12].parse().ok()?;
    let start_ticks: u64 = fields[19].parse().ok()?;
    let rss_pages: i64 = fields[21].parse().ok()?;

    let total_ticks = utime_ticks.saturating_add(stime_ticks);
    let tck = clk_tck.max(1);
    let cpu_time_ns = (total_ticks as u128)
        .saturating_mul(1_000_000_000u128)
        .checked_div(tck as u128)
        .unwrap_or(0) as u64;

    let memory_bytes = (rss_pages.max(0) as u64).saturating_mul(page_size.max(1));
    let start_uptime_secs = Some((start_ticks as f64) / (tck as f64));

    Some(RawProcessSample {
        pid,
        name,
        cpu_time_ns,
        memory_bytes,
        start_uptime_secs,
    })
}
