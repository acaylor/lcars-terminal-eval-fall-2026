//! Converts raw snapshots into what the views display.

use super::rates::{cpu_fraction, CounterWidth, History, RateTracker};
use super::{Disk, Memory, Snapshot, Swap};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const HISTORY: usize = 240;
pub const TOP_PROCESSES: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessView {
    pub pid: i32,
    pub name: String,
    /// Percent of one core, like `top`; `None` until two samples exist.
    pub cpu_pct: Option<f32>,
    pub rss: u64,
}

/// Presentation-ready state. Fractions are 0..=1, rates are bytes/second.
#[derive(Clone, Debug)]
pub struct Telemetry {
    pub cpu: Option<f32>,
    pub cores: Vec<Option<f32>>,
    pub cpu_history: History,
    pub load: Option<[f64; 3]>,
    pub memory: Option<Memory>,
    pub swap: Option<Swap>,
    pub disk: Option<Disk>,
    pub rx_rate: Option<f64>,
    pub tx_rate: Option<f64>,
    pub rx_history: History,
    pub tx_history: History,
    pub interfaces: usize,
    pub uptime: Option<Duration>,
    pub process_count: Option<usize>,
    pub top_processes: Vec<ProcessView>,
    /// Measured time between the last two samples.
    pub interval: Option<Duration>,
    pub samples: u64,
}

impl Telemetry {
    fn new() -> Telemetry {
        Telemetry {
            cpu: None,
            cores: Vec::new(),
            cpu_history: History::new(HISTORY),
            load: None,
            memory: None,
            swap: None,
            disk: None,
            rx_rate: None,
            tx_rate: None,
            rx_history: History::new(HISTORY),
            tx_history: History::new(HISTORY),
            interfaces: 0,
            uptime: None,
            process_count: None,
            top_processes: Vec::new(),
            interval: None,
            samples: 0,
        }
    }
}

struct IfaceRates {
    rx: RateTracker,
    tx: RateTracker,
}

pub struct Sampler {
    prev: Option<Snapshot>,
    ifaces: HashMap<String, IfaceRates>,
    proc_prev: HashMap<i32, (Duration, Instant)>,
    counter_width: CounterWidth,
    telemetry: Telemetry,
}

impl Sampler {
    pub fn new(counter_width: CounterWidth) -> Sampler {
        Sampler {
            prev: None,
            ifaces: HashMap::new(),
            proc_prev: HashMap::new(),
            counter_width,
            telemetry: Telemetry::new(),
        }
    }

    pub fn telemetry(&self) -> &Telemetry {
        &self.telemetry
    }

    /// Fold a new snapshot in. Rates are computed from the snapshot's own
    /// timestamps, so pauses and scheduling jitter do not distort them.
    pub fn push(&mut self, snap: Snapshot) -> &Telemetry {
        let t = &mut self.telemetry;
        t.samples += 1;
        t.load = snap.load;
        t.memory = snap.memory;
        t.swap = snap.swap;
        t.disk = snap.disk.clone();
        t.uptime = snap.uptime;
        if snap.process_count.is_some() {
            t.process_count = snap.process_count;
        }
        t.interval = self.prev.as_ref().map(|p| snap.at.saturating_duration_since(p.at));

        // CPU: needs two readings.
        t.cpu = match (&self.prev, &snap.cpu_total) {
            (Some(p), Some(cur)) => p.cpu_total.as_ref().and_then(|pt| cpu_fraction(pt, cur)),
            _ => None,
        };
        t.cpu_history.push(t.cpu);
        t.cores = snap
            .cpu_cores
            .iter()
            .enumerate()
            .map(|(i, cur)| {
                self.prev
                    .as_ref()
                    .and_then(|p| p.cpu_cores.get(i))
                    .and_then(|pt| cpu_fraction(pt, cur))
            })
            .collect();

        // Network: per-interface trackers so interfaces that appear or vanish
        // do not produce a bogus spike in the summed rate.
        match &snap.interfaces {
            Some(list) => {
                let mut rx_sum = 0.0;
                let mut tx_sum = 0.0;
                let mut any = false;
                let mut seen = std::collections::HashSet::new();
                for iface in list {
                    seen.insert(iface.name.clone());
                    let w = self.counter_width;
                    let e = self.ifaces.entry(iface.name.clone()).or_insert_with(|| IfaceRates {
                        rx: RateTracker::new(w),
                        tx: RateTracker::new(w),
                    });
                    if let Some(r) = e.rx.update(iface.rx_bytes, snap.at) {
                        rx_sum += r;
                        any = true;
                    }
                    if let Some(r) = e.tx.update(iface.tx_bytes, snap.at) {
                        tx_sum += r;
                        any = true;
                    }
                }
                self.ifaces.retain(|k, _| seen.contains(k));
                t.interfaces = list.len();
                t.rx_rate = any.then_some(rx_sum);
                t.tx_rate = any.then_some(tx_sum);
            }
            None => {
                t.rx_rate = None;
                t.tx_rate = None;
                t.interfaces = 0;
            }
        }
        t.rx_history.push(t.rx_rate.map(|r| r as f32));
        t.tx_history.push(t.tx_rate.map(|r| r as f32));

        // Processes: CPU% from the change in cumulative CPU time since the
        // last time we saw that pid. A pid that vanished simply drops out.
        if let Some(procs) = &snap.processes {
            let mut views: Vec<ProcessView> = procs
                .iter()
                .map(|p| {
                    let pct = self.proc_prev.get(&p.pid).and_then(|(prev_cpu, prev_at)| {
                        let dt = snap.at.saturating_duration_since(*prev_at).as_secs_f64();
                        if dt <= 0.0 {
                            return None;
                        }
                        let d = p.cpu_time.checked_sub(*prev_cpu)?.as_secs_f64();
                        Some((d / dt * 100.0) as f32)
                    });
                    ProcessView {
                        pid: p.pid,
                        name: p.name.clone(),
                        cpu_pct: pct,
                        rss: p.rss,
                    }
                })
                .collect();
            self.proc_prev = procs.iter().map(|p| (p.pid, (p.cpu_time, snap.at))).collect();
            views.sort_by(|a, b| {
                b.cpu_pct
                    .unwrap_or(-1.0)
                    .partial_cmp(&a.cpu_pct.unwrap_or(-1.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.rss.cmp(&a.rss))
                    .then(a.pid.cmp(&b.pid))
            });
            views.truncate(TOP_PROCESSES);
            t.top_processes = views;
        } else {
            // Keep the previous table while the diagnostic view is closed so
            // reopening it shows the last known state immediately.
        }

        self.prev = Some(snap);
        &self.telemetry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{CpuTicks, Interface, ProcessSample};

    fn snap(base: Instant, ms: u64) -> Snapshot {
        Snapshot::empty(base + Duration::from_millis(ms))
    }

    #[test]
    fn cpu_percentages_need_two_samples() {
        let base = Instant::now();
        let mut s = Sampler::new(CounterWidth::Bits64);
        let mut a = snap(base, 0);
        a.cpu_total = Some(CpuTicks { busy: 10, idle: 90 });
        a.cpu_cores = vec![CpuTicks { busy: 5, idle: 45 }, CpuTicks { busy: 5, idle: 45 }];
        s.push(a);
        assert_eq!(s.telemetry().cpu, None);
        let mut b = snap(base, 1000);
        b.cpu_total = Some(CpuTicks { busy: 60, idle: 140 });
        b.cpu_cores = vec![CpuTicks { busy: 55, idle: 45 }, CpuTicks { busy: 5, idle: 95 }];
        let t = s.push(b);
        assert!((t.cpu.unwrap() - 0.5).abs() < 1e-6);
        assert!((t.cores[0].unwrap() - 1.0).abs() < 1e-6);
        assert!((t.cores[1].unwrap() - 0.0).abs() < 1e-6);
        assert_eq!(t.interval, Some(Duration::from_secs(1)));
        assert_eq!(t.cpu_history.len(), 2);
    }

    #[test]
    fn interface_appearing_does_not_spike() {
        let base = Instant::now();
        let mut s = Sampler::new(CounterWidth::Bits64);
        let mut a = snap(base, 0);
        a.interfaces = Some(vec![Interface {
            name: "eth0".into(),
            rx_bytes: 1000,
            tx_bytes: 0,
        }]);
        s.push(a);
        let mut b = snap(base, 1000);
        b.interfaces = Some(vec![
            Interface {
                name: "eth0".into(),
                rx_bytes: 2000,
                tx_bytes: 0,
            },
            // Pre-existing counters on a newly visible interface must not count.
            Interface {
                name: "wlan0".into(),
                rx_bytes: 9_000_000,
                tx_bytes: 0,
            },
        ]);
        let t = s.push(b);
        assert!((t.rx_rate.unwrap() - 1000.0).abs() < 1e-9);
        assert_eq!(t.interfaces, 2);
        // Interface disappears: its tracker is dropped, the rest keeps working.
        let mut c = snap(base, 2000);
        c.interfaces = Some(vec![Interface {
            name: "eth0".into(),
            rx_bytes: 2500,
            tx_bytes: 0,
        }]);
        let t = s.push(c);
        assert!((t.rx_rate.unwrap() - 500.0).abs() < 1e-9);
        assert_eq!(s.ifaces.len(), 1);
    }

    #[test]
    fn missing_metrics_are_reported_as_none() {
        let base = Instant::now();
        let mut s = Sampler::new(CounterWidth::Bits64);
        s.push(snap(base, 0));
        let t = s.push(snap(base, 1000));
        assert_eq!(t.cpu, None);
        assert_eq!(t.rx_rate, None);
        assert_eq!(t.memory, None);
        assert_eq!(t.cpu_history.last_n(2), vec![None, None]);
    }

    #[test]
    fn processes_ranked_by_cpu_then_rss_and_vanishing_pids_drop_out() {
        let base = Instant::now();
        let mut s = Sampler::new(CounterWidth::Bits64);
        let p = |pid, name: &str, ms, rss| ProcessSample {
            pid,
            name: name.into(),
            cpu_time: Duration::from_millis(ms),
            rss,
        };
        let mut a = snap(base, 0);
        a.processes = Some(vec![
            p(1, "init", 0, 10),
            p(2, "busy", 0, 20),
            p(3, "gone", 0, 5),
            p(4, "日本語", 0, 1),
        ]);
        s.push(a);
        assert!(s.telemetry().top_processes.iter().all(|v| v.cpu_pct.is_none()));
        let mut b = snap(base, 2000);
        b.processes = Some(vec![
            p(1, "init", 100, 10),
            p(2, "busy", 1500, 20),
            p(4, "日本語", 100, 1),
        ]);
        let t = s.push(b);
        let names: Vec<&str> = t.top_processes.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["busy", "init", "日本語"]);
        assert!((t.top_processes[0].cpu_pct.unwrap() - 75.0).abs() < 1e-4);
        assert!(!s.proc_prev.contains_key(&3));
    }

    #[test]
    fn top_list_is_capped() {
        let base = Instant::now();
        let mut s = Sampler::new(CounterWidth::Bits64);
        let mut a = snap(base, 0);
        a.processes = Some(
            (0..50)
                .map(|i| ProcessSample {
                    pid: i,
                    name: format!("p{i}"),
                    cpu_time: Duration::ZERO,
                    rss: i as u64,
                })
                .collect(),
        );
        let t = s.push(a);
        assert_eq!(t.top_processes.len(), TOP_PROCESSES);
        assert_eq!(t.top_processes[0].rss, 49);
    }
}
