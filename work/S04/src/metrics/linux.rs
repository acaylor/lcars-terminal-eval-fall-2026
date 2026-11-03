use super::parse::{
    memory_fields, parse_loadavg, parse_meminfo, parse_net_dev, parse_os_release,
    parse_proc_pid_stat, parse_proc_stat, parse_uptime, swap_fields,
};
use super::rate::{cpu_percent, iface_rates, process_cpu_ticks, IfaceSnap};
use super::unix::{self, clk_tck, page_size};
use super::{top_processes, ProcessRow, Sample};
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

pub struct LinuxCollector {
    prev_cpu: Option<(u64, u64)>,
    prev_cores: Vec<(u64, u64)>,
    prev_net: Vec<IfaceSnap>,
    prev_procs: HashMap<(u32, u64), u64>,
    prev_at: Option<Instant>,
    clk: f64,
    page: u64,
    os_name: String,
    arch: String,
}

impl LinuxCollector {
    pub fn new() -> Self {
        let (sys, rel, arch) = unix::uname_parts();
        let os_name = fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|t| parse_os_release(&t))
            .unwrap_or_else(|| {
                if rel.is_empty() {
                    sys
                } else {
                    format!("{sys} {rel}")
                }
            });
        Self {
            prev_cpu: None,
            prev_cores: Vec::new(),
            prev_net: Vec::new(),
            prev_procs: HashMap::new(),
            prev_at: None,
            clk: clk_tck(),
            page: page_size(),
            os_name,
            arch,
        }
    }

    pub fn sample(&mut self, processes: bool) -> Sample {
        let now = Instant::now();
        let elapsed = self
            .prev_at
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        let mut sample =
            Sample::unavailable(unix::hostname(), self.os_name.clone(), self.arch.clone());
        sample.time = unix::civil_now();
        sample.uptime = read_uptime();
        sample.load = read_load();
        if let Some((used, total)) = unix::disk_root() {
            sample.disk_used = Some(used);
            sample.disk_total = Some(total);
        }
        self.fill_cpu(&mut sample, elapsed);
        self.fill_mem(&mut sample);
        self.fill_net(&mut sample, elapsed);
        if processes {
            self.fill_procs(&mut sample, elapsed);
        } else {
            self.prev_procs.clear();
        }
        self.prev_at = Some(now);
        sample
    }

    fn fill_cpu(&mut self, sample: &mut Sample, elapsed: f64) {
        let text = match fs::read_to_string("/proc/stat") {
            Ok(t) => t,
            Err(_) => return,
        };
        let Some(stat) = parse_proc_stat(&text) else {
            return;
        };
        if elapsed > 0.0 {
            if let Some((pb, pt)) = self.prev_cpu {
                sample.cpu_pct = cpu_percent(pb, pt, stat.busy, stat.total);
            }
        }
        let mut cores = Vec::with_capacity(stat.cores.len());
        for (i, (busy, total)) in stat.cores.iter().copied().enumerate() {
            let pct = if elapsed > 0.0 {
                self.prev_cores
                    .get(i)
                    .and_then(|(pb, pt)| cpu_percent(*pb, *pt, busy, total))
            } else {
                None
            };
            cores.push(pct);
        }
        sample.per_core = cores;
        self.prev_cpu = Some((stat.busy, stat.total));
        self.prev_cores = stat.cores;
    }

    fn fill_mem(&self, sample: &mut Sample) {
        let Ok(text) = fs::read_to_string("/proc/meminfo") else {
            return;
        };
        let info = parse_meminfo(&text);
        let (used, avail, total) = memory_fields(&info);
        sample.mem_used = used;
        sample.mem_available = avail;
        sample.mem_total = total;
        let (su, st) = swap_fields(&info);
        sample.swap_used = su;
        sample.swap_total = st;
    }

    fn fill_net(&mut self, sample: &mut Sample, elapsed: f64) {
        let Ok(text) = fs::read_to_string("/proc/net/dev") else {
            return;
        };
        let now = parse_net_dev(&text);
        if elapsed > 0.0 {
            if let Some((rx, tx)) = iface_rates(&self.prev_net, &now, elapsed) {
                sample.net_rx_bps = Some(rx);
                sample.net_tx_bps = Some(tx);
            }
        }
        self.prev_net = now;
    }

    fn fill_procs(&mut self, sample: &mut Sample, elapsed: f64) {
        let Ok(dir) = fs::read_dir("/proc") else {
            sample.process_count = None;
            sample.top_cpu = Vec::new();
            return;
        };
        let mut count = 0usize;
        let mut rows = Vec::new();
        let mut next_map = HashMap::new();
        for ent in dir.flatten() {
            let name = ent.file_name();
            let Some(pid_str) = name.to_str() else {
                continue;
            };
            if !pid_str.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            count += 1;
            let Ok(pid) = pid_str.parse::<u32>() else {
                continue;
            };
            let path = format!("/proc/{pid}/stat");
            let Ok(bytes) = fs::read(&path) else { continue };
            let Some(stat) = parse_proc_pid_stat(&bytes) else {
                continue;
            };
            let ticks = stat.utime.saturating_add(stat.stime);
            let key = (stat.pid, stat.starttime);
            let cpu = if elapsed > 0.0 {
                self.prev_procs
                    .get(&key)
                    .and_then(|prev| process_cpu_ticks(*prev, ticks, elapsed, self.clk))
            } else {
                None
            };
            next_map.insert(key, ticks);
            let mem = stat.rss_pages.saturating_mul(self.page);
            rows.push(ProcessRow {
                pid: stat.pid,
                name: if stat.name.is_empty() {
                    format!("pid-{pid}")
                } else {
                    stat.name
                },
                cpu_pct: cpu,
                mem_bytes: Some(mem),
            });
        }
        sample.process_count = Some(count);
        sample.top_cpu = top_processes(rows);
        self.prev_procs = next_map;
    }
}

fn read_uptime() -> Option<std::time::Duration> {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|t| parse_uptime(&t))
}

fn read_load() -> Option<[f64; 3]> {
    fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|t| parse_loadavg(&t))
}
