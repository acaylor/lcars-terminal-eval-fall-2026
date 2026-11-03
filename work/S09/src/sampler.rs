use std::collections::BTreeMap;
use std::time::Duration;

use crate::model::{
    CpuTimes, DiskMetrics, MemoryMetrics, NetworkMetrics, ProcessEntry, RawNetBytes,
    RawSystemSample, SwapMetrics, SystemAlertState, SystemTelemetry,
};

pub const MAX_HISTORY_LEN: usize = 64;
const MIN_VALID_DT_SECS: f64 = 0.001;
const MAX_VALID_DT_SECS: f64 = 120.0;

/// Stateful, deterministic sampler that converts consecutive `RawSystemSample` snapshots
/// and elapsed durations into presentation-ready `SystemTelemetry`.
#[derive(Debug, Clone, Default)]
pub struct TelemetrySampler {
    tick: u64,
    prev_cpu_total: Option<CpuTimes>,
    prev_cpu_cores: Vec<CpuTimes>,
    prev_net: BTreeMap<String, RawNetBytes>,
    prev_procs: BTreeMap<u32, (u64, String)>,
    last_cpu_overall: Option<f32>,
    last_cpu_cores: Vec<f32>,
    cpu_history: Vec<f32>,
    net_rx_history: Vec<f64>,
    net_tx_history: Vec<f64>,
}

impl TelemetrySampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes a new raw system snapshot given the elapsed time `dt` since the previous sample.
    /// On the first sample, `dt` may be `Duration::ZERO`.
    pub fn ingest(&mut self, sample: RawSystemSample, dt: Duration) -> SystemTelemetry {
        self.tick = self.tick.wrapping_add(1);
        let dt_secs = dt.as_secs_f64();
        let valid_dt = (MIN_VALID_DT_SECS..=MAX_VALID_DT_SECS).contains(&dt_secs);

        // 1. Overall CPU utilization
        let cpu_overall_pct = match (sample.cpu_total, self.prev_cpu_total) {
            (Some(curr), Some(prev)) => curr.utilization_delta(&prev).or(self.last_cpu_overall),
            (Some(curr), None) => {
                // First sample fallback: ratio of cumulative active/total if non-zero
                if curr.total > 0 {
                    Some(((curr.active as f64 / curr.total as f64) * 100.0).clamp(0.0, 100.0) as f32)
                } else {
                    None
                }
            }
            (None, _) => None,
        };
        if let Some(curr) = sample.cpu_total {
            self.prev_cpu_total = Some(curr);
        }
        self.last_cpu_overall = cpu_overall_pct;

        // 2. Per-core CPU utilization
        let mut cpu_cores_pct = Vec::with_capacity(sample.cpu_cores.len());
        for (idx, curr_core) in sample.cpu_cores.iter().enumerate() {
            let pct = if let Some(prev_core) = self.prev_cpu_cores.get(idx) {
                curr_core
                    .utilization_delta(prev_core)
                    .or_else(|| self.last_cpu_cores.get(idx).copied())
                    .unwrap_or(0.0)
            } else if curr_core.total > 0 {
                ((curr_core.active as f64 / curr_core.total as f64) * 100.0).clamp(0.0, 100.0) as f32
            } else {
                0.0
            };
            cpu_cores_pct.push(pct);
        }
        self.prev_cpu_cores = sample.cpu_cores.clone();
        self.last_cpu_cores = cpu_cores_pct.clone();

        // 3. Memory metrics
        let memory = match (sample.mem_total_bytes, sample.mem_avail_bytes) {
            (Some(total), Some(avail)) if total > 0 => {
                let clamped_avail = avail.min(total);
                let used = total.saturating_sub(clamped_avail);
                let used_pct = ((used as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32;
                Some(MemoryMetrics {
                    used_bytes: used,
                    avail_bytes: clamped_avail,
                    total_bytes: total,
                    used_pct,
                })
            }
            _ => None,
        };

        // 4. Swap metrics
        let swap = match (sample.swap_total_bytes, sample.swap_used_bytes) {
            (Some(total), Some(used)) if total > 0 => {
                let clamped_used = used.min(total);
                let used_pct = ((clamped_used as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32;
                Some(SwapMetrics {
                    used_bytes: clamped_used,
                    total_bytes: total,
                    used_pct,
                })
            }
            _ => None,
        };

        // 5. Root disk metrics
        let disk_root = match (
            sample.disk_root_total_bytes,
            sample.disk_root_used_bytes,
            sample.disk_root_avail_bytes,
        ) {
            (Some(total), Some(used), Some(avail)) if total > 0 => {
                let clamped_used = used.min(total);
                let used_pct = ((clamped_used as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32;
                Some(DiskMetrics {
                    used_bytes: clamped_used,
                    avail_bytes: avail.min(total),
                    total_bytes: total,
                    used_pct,
                })
            }
            _ => None,
        };

        // 6. Network receive & transmit rates across active interfaces
        let network = if sample.net_interfaces.is_empty() {
            self.prev_net.clear();
            None
        } else {
            let mut total_rx_delta: u64 = 0;
            let mut total_tx_delta: u64 = 0;
            if valid_dt && !self.prev_net.is_empty() {
                for (if_name, curr_net) in &sample.net_interfaces {
                    if let Some(prev_net) = self.prev_net.get(if_name) {
                        total_rx_delta = total_rx_delta
                            .saturating_add(counter_delta(prev_net.rx_bytes, curr_net.rx_bytes));
                        total_tx_delta = total_tx_delta
                            .saturating_add(counter_delta(prev_net.tx_bytes, curr_net.tx_bytes));
                    }
                }
            }
            let active_interfaces = sample.net_interfaces.len();
            self.prev_net = sample.net_interfaces;

            let (rx_rate, tx_rate) = if valid_dt && self.tick > 1 {
                (
                    (total_rx_delta as f64) / dt_secs,
                    (total_tx_delta as f64) / dt_secs,
                )
            } else {
                (0.0, 0.0)
            };

            Some(NetworkMetrics {
                rx_bytes_per_sec: rx_rate,
                tx_bytes_per_sec: tx_rate,
                active_interfaces,
            })
        };

        // 7. Process CPU utilization & Top-5 sorting
        let mut next_prev_procs = BTreeMap::new();
        let mut computed_procs = Vec::with_capacity(sample.processes.len());
        let max_cpu_pct = (sample.cpu_cores.len().max(1) as f32) * 100.0;

        for proc_sample in sample.processes {
            let cpu_pct = if let Some((prev_ns, prev_name)) = self.prev_procs.get(&proc_sample.pid)
            {
                if valid_dt && prev_name == &proc_sample.name && proc_sample.cpu_time_ns >= *prev_ns
                {
                    let delta_ns = proc_sample.cpu_time_ns - *prev_ns;
                    let delta_secs = (delta_ns as f64) / 1_000_000_000.0;
                    (((delta_secs / dt_secs) * 100.0) as f32).clamp(0.0, max_cpu_pct)
                } else {
                    0.0
                }
            } else if let (Some(sys_up), Some(start_up)) =
                (sample.uptime_secs, proc_sample.start_uptime_secs)
            {
                // First sample estimation based on lifetime CPU usage
                let alive_secs = (sys_up as f64 - start_up).max(0.5);
                let cpu_secs = (proc_sample.cpu_time_ns as f64) / 1_000_000_000.0;
                (((cpu_secs / alive_secs) * 100.0) as f32).clamp(0.0, max_cpu_pct)
            } else {
                0.0
            };

            next_prev_procs.insert(
                proc_sample.pid,
                (proc_sample.cpu_time_ns, proc_sample.name.clone()),
            );
            computed_procs.push(ProcessEntry {
                pid: proc_sample.pid,
                name: proc_sample.name,
                cpu_pct,
                memory_bytes: proc_sample.memory_bytes,
            });
        }
        // Replace `prev_procs` wholesale so terminated PIDs are immediately freed
        self.prev_procs = next_prev_procs;

        // Sort descending by CPU usage, breaking ties by memory usage then PID
        computed_procs.sort_by(|a, b| {
            b.cpu_pct
                .partial_cmp(&a.cpu_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
                .then_with(|| a.pid.cmp(&b.pid))
        });
        computed_procs.truncate(5);

        // 8. Bounded history ring buffers
        push_bounded(&mut self.cpu_history, cpu_overall_pct.unwrap_or(0.0));
        push_bounded(
            &mut self.net_rx_history,
            network.map(|n| n.rx_bytes_per_sec).unwrap_or(0.0),
        );
        push_bounded(
            &mut self.net_tx_history,
            network.map(|n| n.tx_bytes_per_sec).unwrap_or(0.0),
        );

        // 9. Determine overall alert status
        let cpu_val = cpu_overall_pct.unwrap_or(0.0);
        let mem_val = memory.map(|m| m.used_pct).unwrap_or(0.0);
        let disk_val = disk_root.map(|d| d.used_pct).unwrap_or(0.0);
        let alert_state = if cpu_val >= 90.0 || mem_val >= 92.0 || disk_val >= 95.0 {
            SystemAlertState::Critical
        } else if cpu_val >= 70.0 || mem_val >= 80.0 || disk_val >= 85.0 {
            SystemAlertState::Elevated
        } else {
            SystemAlertState::Nominal
        };

        SystemTelemetry {
            tick: self.tick,
            clock: sample.clock,
            hostname: sample.hostname,
            os_name: sample.os_name,
            arch: sample.arch,
            uptime_secs: sample.uptime_secs,
            cpu_overall_pct,
            cpu_cores_pct,
            load_avg: sample.load_avg,
            memory,
            swap,
            disk_root,
            network,
            process_count: sample.process_count,
            top_processes: computed_procs,
            cpu_history: self.cpu_history.clone(),
            net_rx_history: self.net_rx_history.clone(),
            net_tx_history: self.net_tx_history.clone(),
            alert_state,
        }
    }
}

/// Computes counter delta with safe handling for 32-bit wraparound and counter resets.
pub fn counter_delta(prev: u64, curr: u64) -> u64 {
    if curr >= prev {
        curr - prev
    } else if prev <= u32::MAX as u64 {
        // Check if this is a plausible 32-bit counter rollover (e.g. BSD `if_data` 32-bit counters)
        let wrapped = (u32::MAX as u64 - prev)
            .saturating_add(curr)
            .saturating_add(1);
        // Only treat as 32-bit rollover if the delta is reasonable (< 512 MiB in one tick);
        // otherwise treat as an interface reset (0 delta).
        if wrapped <= 512 * 1024 * 1024 {
            wrapped
        } else {
            0
        }
    } else {
        // Counter reset -> do not fabricate a spike
        0
    }
}

fn push_bounded<T>(buf: &mut Vec<T>, val: T) {
    if buf.len() >= MAX_HISTORY_LEN {
        buf.remove(0);
    }
    buf.push(val);
}
