//! Turns raw counter samples into rates. Pure and deterministic: time is
//! passed in explicitly, so tests can exercise zero-length, very long, and
//! out-of-order intervals as well as counter wrap and reset.

use super::history::History;
use super::*;
use std::collections::HashMap;
use std::time::Duration;

/// Intervals shorter than this are too noisy to divide by; the previous
/// rates are kept and the baseline is left untouched.
pub const MIN_DT: f64 = 0.05;

pub const TOP_N: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetRates {
    pub rx_bps: f64,
    pub tx_bps: f64,
    /// Interfaces that contributed to the rate.
    pub ifaces: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcTop {
    pub pid: u32,
    pub name: String,
    /// Percent of one CPU (like `top`); may exceed 100 on multi-core systems.
    pub cpu_pct: f64,
    pub rss_bytes: u64,
    pub mem_pct: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcSummary {
    pub total: usize,
    pub unreadable: usize,
    pub top: Vec<ProcTop>,
    /// No previous process sample yet, so CPU figures are not meaningful.
    pub warming: bool,
}

/// Everything the UI needs, derived from the latest samples.
#[derive(Clone, Debug)]
pub struct Telemetry {
    pub hostname: Reading<String>,
    pub uptime: Reading<Duration>,
    pub load: Reading<[f64; 3]>,
    pub cpu_pct: Reading<f64>,
    pub core_pct: Reading<Vec<f64>>,
    pub memory: Reading<Memory>,
    pub swap: Reading<Swap>,
    pub disk: Reading<Disk>,
    pub net: Reading<NetRates>,
    pub procs: Option<Reading<ProcSummary>>,
    /// Number of samples ingested so far.
    pub samples: u64,
}

impl Default for Telemetry {
    fn default() -> Self {
        let p = Missing::Pending;
        Telemetry {
            hostname: Err(p),
            uptime: Err(p),
            load: Err(p),
            cpu_pct: Err(p),
            core_pct: Err(p),
            memory: Err(p),
            swap: Err(p),
            disk: Err(p),
            net: Err(p),
            procs: None,
            samples: 0,
        }
    }
}

type ProcKey = (u32, u64);

#[derive(Clone, Debug)]
struct Baseline {
    at: f64,
    cpu: Option<CpuTicks>,
    cores: Option<Vec<CpuTicks>>,
    net: HashMap<String, (u64, u64, Option<u64>)>,
    net_ok: bool,
    procs: Option<HashMap<ProcKey, u64>>,
}

#[derive(Default)]
pub struct Sampler {
    base: Option<Baseline>,
    last: Telemetry,
    pub history: History,
}

/// Difference between two readings of a monotonically increasing counter.
/// A decrease is either a wrap of a narrow counter (when `modulus` is known
/// and the implied delta is plausible) or a reset, which yields `None`.
pub fn counter_delta(prev: u64, cur: u64, modulus: Option<u64>) -> Option<u64> {
    if cur >= prev {
        return Some(cur - prev);
    }
    let m = modulus?;
    if prev >= m {
        return None;
    }
    let wrapped = m - prev + cur;
    // A "wrap" that implies more than half the counter range is far more
    // likely to be a reset (interface re-created, driver reloaded).
    if wrapped <= m / 2 {
        Some(wrapped)
    } else {
        None
    }
}

/// Busy percentage between two tick readings; `None` if counters went
/// backwards (CPU hot-unplug, suspend quirks) or nothing elapsed.
pub fn cpu_percent(prev: CpuTicks, cur: CpuTicks) -> Option<f64> {
    let busy = cur.busy.checked_sub(prev.busy)?;
    let idle = cur.idle.checked_sub(prev.idle)?;
    let total = busy + idle;
    if total == 0 {
        return None;
    }
    Some((busy as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

pub fn per_second(delta: u64, dt: f64) -> Option<f64> {
    if dt < MIN_DT || !dt.is_finite() {
        None
    } else {
        Some(delta as f64 / dt)
    }
}

impl Sampler {
    pub fn new() -> Sampler {
        Sampler::default()
    }

    pub fn telemetry(&self) -> &Telemetry {
        &self.last
    }

    /// Feed a raw sample taken at monotonic time `at`.
    pub fn ingest(&mut self, raw: RawSample, at: Duration) -> &Telemetry {
        let at_s = at.as_secs_f64();
        let dt = self.base.as_ref().map(|b| at_s - b.at);
        let mut t = Telemetry {
            hostname: raw.hostname.clone(),
            uptime: raw.uptime,
            load: raw.load,
            memory: raw.memory,
            swap: raw.swap,
            disk: raw.disk.clone(),
            samples: self.last.samples + 1,
            ..Telemetry::default()
        };

        match (self.base.as_ref(), dt) {
            (Some(_), Some(dt)) if dt < MIN_DT => {
                // Too soon (or clock went backwards): reuse the previous rates.
                t.cpu_pct = self.last.cpu_pct;
                t.core_pct = self.last.core_pct.clone();
                t.net = self.last.net;
                t.procs = self.last.procs.clone();
                self.last = t;
                return &self.last;
            }
            (Some(base), Some(dt)) => {
                t.cpu_pct = rate_cpu(base.cpu, &raw.cpu);
                t.core_pct = rate_cores(base.cores.as_deref(), &raw.cores);
                t.net = rate_net(base, &raw.net, dt);
                t.procs = raw.procs.as_ref().map(|p| summarize_procs(p, base.procs.as_ref(), dt, &raw.memory));
            }
            _ => {
                t.procs = raw.procs.as_ref().map(|p| summarize_procs(p, None, 0.0, &raw.memory));
            }
        }

        self.push_history(&t);
        self.base = Some(Baseline {
            at: at_s,
            cpu: raw.cpu.ok(),
            cores: raw.cores.ok(),
            net_ok: raw.net.is_ok(),
            net: raw.net.map(|v| v.into_iter().map(|i| (i.name, (i.rx_bytes, i.tx_bytes, i.modulus))).collect()).unwrap_or_default(),
            procs: match raw.procs {
                Some(Ok(table)) => Some(table.procs.iter().map(|p| ((p.pid, p.start), p.cpu_ns)).collect()),
                _ => None,
            },
        });
        self.last = t;
        &self.last
    }

    fn push_history(&mut self, t: &Telemetry) {
        let h = &mut self.history;
        if let Ok(c) = t.cpu_pct {
            h.cpu.push(c as f32);
        }
        if let Ok(m) = &t.memory {
            h.mem.push((m.used_fraction() * 100.0) as f32);
        }
        if let Ok(n) = t.net {
            h.rx.push(n.rx_bps as f32);
            h.tx.push(n.tx_bps as f32);
        }
        if let Ok(l) = t.load {
            h.load.push(l[0] as f32);
        }
    }

    /// Replace the display name of a process in the current summary.
    pub fn rename_process(&mut self, pid: u32, name: String) {
        if let Some(Ok(s)) = self.last.procs.as_mut() {
            for p in s.top.iter_mut().filter(|p| p.pid == pid) {
                p.name = name.clone();
            }
        }
    }
}

fn rate_cpu(prev: Option<CpuTicks>, cur: &Reading<CpuTicks>) -> Reading<f64> {
    let cur = (*cur)?;
    let prev = prev.ok_or(Missing::Pending)?;
    cpu_percent(prev, cur).ok_or(Missing::Pending)
}

fn rate_cores(prev: Option<&[CpuTicks]>, cur: &Reading<Vec<CpuTicks>>) -> Reading<Vec<f64>> {
    let cur = cur.as_ref().map_err(|e| *e)?;
    let prev = prev.ok_or(Missing::Pending)?;
    if prev.len() != cur.len() {
        return Err(Missing::Pending); // CPUs came or went; resync
    }
    Ok(prev.iter().zip(cur).map(|(p, c)| cpu_percent(*p, *c).unwrap_or(0.0)).collect())
}

fn rate_net(base: &Baseline, cur: &Reading<Vec<IfaceCounters>>, dt: f64) -> Reading<NetRates> {
    let cur = cur.as_ref().map_err(|e| *e)?;
    if !base.net_ok {
        return Err(Missing::Pending);
    }
    let (mut rx, mut tx, mut n) = (0u64, 0u64, 0usize);
    for i in cur {
        // Interfaces that just appeared have no baseline; they join next time.
        let Some(&(prx, ptx, _)) = base.net.get(&i.name) else { continue };
        if let (Some(dr), Some(dt_)) = (counter_delta(prx, i.rx_bytes, i.modulus), counter_delta(ptx, i.tx_bytes, i.modulus)) {
            rx += dr;
            tx += dt_;
            n += 1;
        }
    }
    match (per_second(rx, dt), per_second(tx, dt)) {
        (Some(r), Some(t)) => Ok(NetRates { rx_bps: r, tx_bps: t, ifaces: n }),
        _ => Err(Missing::Pending),
    }
}

fn summarize_procs(table: &Reading<ProcTable>, prev: Option<&HashMap<ProcKey, u64>>, dt: f64, mem: &Reading<Memory>) -> Reading<ProcSummary> {
    let table = table.as_ref().map_err(|e| *e)?;
    let total_mem = mem.as_ref().ok().map(|m| m.total).filter(|&t| t > 0);
    let warming = prev.is_none() || dt < MIN_DT;
    let mut rows: Vec<ProcTop> = table
        .procs
        .iter()
        .map(|p| {
            let cpu_pct = match prev.and_then(|m| m.get(&(p.pid, p.start))) {
                Some(&before) if !warming => {
                    // Saturating: a process's CPU time can't go backwards, but
                    // a PID+start collision could make it look that way.
                    p.cpu_ns.saturating_sub(before) as f64 / (dt * 1e9) * 100.0
                }
                _ => 0.0,
            };
            ProcTop {
                pid: p.pid,
                name: p.name.clone(),
                cpu_pct,
                rss_bytes: p.rss_bytes,
                mem_pct: total_mem.map(|t| p.rss_bytes as f64 / t as f64 * 100.0),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal).then(b.rss_bytes.cmp(&a.rss_bytes)).then(a.pid.cmp(&b.pid))
    });
    rows.truncate(TOP_N);
    Ok(ProcSummary { total: table.total, unreadable: table.unreadable, top: rows, warming })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(cpu: (u64, u64), rx: u64, tx: u64) -> RawSample {
        RawSample {
            hostname: Ok("enterprise".into()),
            uptime: Ok(Duration::from_secs(100)),
            load: Ok([0.5, 0.4, 0.3]),
            cpu: Ok(CpuTicks { busy: cpu.0, idle: cpu.1 }),
            cores: Ok(vec![CpuTicks { busy: cpu.0 / 2, idle: cpu.1 / 2 }; 2]),
            memory: Ok(Memory { total: 1000, available: 250 }),
            swap: Ok(Swap { total: 0, used: 0 }),
            disk: Err(Missing::Unsupported),
            net: Ok(vec![IfaceCounters { name: "eth0".into(), rx_bytes: rx, tx_bytes: tx, modulus: None }]),
            procs: None,
        }
    }

    fn secs(s: f64) -> Duration {
        Duration::from_secs_f64(s)
    }

    #[test]
    fn first_sample_has_pending_rates() {
        let mut s = Sampler::new();
        let t = s.ingest(raw((100, 900), 0, 0), secs(10.0));
        assert_eq!(t.cpu_pct, Err(Missing::Pending));
        assert_eq!(t.net, Err(Missing::Pending));
        assert!(t.memory.is_ok());
    }

    #[test]
    fn rates_use_measured_elapsed_time() {
        let mut s = Sampler::new();
        s.ingest(raw((0, 0), 1_000, 0), secs(10.0));
        // 2.5 s later, not the nominal 1 s interval.
        let t = s.ingest(raw((25, 75), 6_000, 2_500), secs(12.5)).clone();
        assert_eq!(t.cpu_pct, Ok(25.0));
        let n = t.net.unwrap();
        assert!((n.rx_bps - 2_000.0).abs() < 1e-9);
        assert!((n.tx_bps - 1_000.0).abs() < 1e-9);
    }

    #[test]
    fn zero_interval_keeps_previous_rates_and_baseline() {
        let mut s = Sampler::new();
        s.ingest(raw((0, 0), 0, 0), secs(1.0));
        let r1 = s.ingest(raw((50, 50), 1000, 1000), secs(2.0)).cpu_pct;
        // Same timestamp: no division by zero, same rate reported.
        let r2 = s.ingest(raw((51, 50), 1001, 1001), secs(2.0)).cpu_pct;
        assert_eq!(r1, r2);
        // Baseline was untouched, so the next delta is measured from t=2.0.
        let t = s.ingest(raw((60, 140), 2000, 2000), secs(3.0)).clone();
        assert_eq!(t.cpu_pct, Ok(10.0));
        assert!((t.net.unwrap().rx_bps - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn backwards_clock_is_treated_like_zero_interval() {
        let mut s = Sampler::new();
        s.ingest(raw((0, 0), 0, 0), secs(5.0));
        let t = s.ingest(raw((10, 10), 10, 10), secs(4.0));
        assert_eq!(t.cpu_pct, Err(Missing::Pending));
    }

    #[test]
    fn very_long_interval_yields_average_rate() {
        let mut s = Sampler::new();
        s.ingest(raw((0, 0), 0, 0), secs(0.0));
        let t = s.ingest(raw((500, 500), 3_600_000, 0), secs(3600.0));
        assert_eq!(t.cpu_pct, Ok(50.0));
        assert!((t.net.unwrap().rx_bps - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn counter_wrap_and_reset() {
        let m = Some(1u64 << 32);
        assert_eq!(counter_delta(10, 25, None), Some(15));
        assert_eq!(counter_delta((1 << 32) - 10, 5, m), Some(15));
        assert_eq!(counter_delta(1_000_000, 5, None), None, "64-bit decrease is a reset");
        assert_eq!(counter_delta(1 << 31, 5, m), None, "implausible wrap is a reset");
    }

    #[test]
    fn interface_reset_and_churn_do_not_spike() {
        let mut s = Sampler::new();
        let mut a = raw((0, 0), 5_000_000, 5_000_000);
        a.net = Ok(vec![
            IfaceCounters { name: "eth0".into(), rx_bytes: 5_000_000, tx_bytes: 5_000_000, modulus: None },
            IfaceCounters { name: "wg0".into(), rx_bytes: 100, tx_bytes: 100, modulus: None },
        ]);
        s.ingest(a, secs(1.0));
        let mut b = raw((1, 1), 0, 0);
        b.net = Ok(vec![
            // eth0 reset to a small value: skipped rather than a huge spike.
            IfaceCounters { name: "eth0".into(), rx_bytes: 10, tx_bytes: 10, modulus: None },
            IfaceCounters { name: "wg0".into(), rx_bytes: 1100, tx_bytes: 600, modulus: None },
            // New interface: no baseline yet.
            IfaceCounters { name: "usb0".into(), rx_bytes: 9_999_999, tx_bytes: 9, modulus: None },
        ]);
        let n = s.ingest(b, secs(2.0)).net.unwrap();
        assert_eq!(n.ifaces, 1);
        assert!((n.rx_bps - 1000.0).abs() < 1e-9);
        assert!((n.tx_bps - 500.0).abs() < 1e-9);
    }

    #[test]
    fn cpu_counter_reset_reports_pending_not_garbage() {
        assert_eq!(cpu_percent(CpuTicks { busy: 100, idle: 100 }, CpuTicks { busy: 5, idle: 200 }), None);
        assert_eq!(cpu_percent(CpuTicks { busy: 1, idle: 1 }, CpuTicks { busy: 1, idle: 1 }), None);
    }

    fn proc(pid: u32, start: u64, cpu_ns: u64, rss: u64) -> ProcRaw {
        ProcRaw { pid, start, name: format!("p{pid}"), cpu_ns, rss_bytes: rss }
    }

    #[test]
    fn process_ranking_handles_exits_and_pid_reuse() {
        let mut s = Sampler::new();
        let mut a = raw((0, 0), 0, 0);
        a.procs = Some(Ok(ProcTable { total: 3, unreadable: 0, procs: vec![proc(1, 1, 0, 10), proc(2, 1, 0, 10), proc(3, 1, 0, 10)] }));
        let first = s.ingest(a, secs(0.0)).procs.clone().unwrap().unwrap();
        assert!(first.warming);

        let mut b = raw((1, 1), 0, 0);
        b.procs = Some(Ok(ProcTable {
            total: 3,
            unreadable: 1,
            procs: vec![
                proc(1, 1, 500_000_000, 10), // 50% over 1 s
                // pid 2 exited; pid 3 was reused by a new process (new start time).
                proc(3, 99, 900_000_000, 10),
            ],
        }));
        let sum = s.ingest(b, secs(1.0)).procs.clone().unwrap().unwrap();
        assert!(!sum.warming);
        assert_eq!(sum.unreadable, 1);
        assert_eq!(sum.top[0].pid, 1);
        assert!((sum.top[0].cpu_pct - 50.0).abs() < 1e-9);
        assert_eq!(sum.top[1].cpu_pct, 0.0, "reused PID must not inherit the old counter");
        assert!((sum.top[0].mem_pct.unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn top_list_is_capped_at_five() {
        let mut s = Sampler::new();
        let mk = |k: u64| {
            let mut r = raw((0, 0), 0, 0);
            r.procs = Some(Ok(ProcTable { total: 20, unreadable: 0, procs: (0..20).map(|i| proc(i, 0, i as u64 * k, 1)).collect() }));
            r
        };
        s.ingest(mk(0), secs(0.0));
        let sum = s.ingest(mk(1_000_000), secs(1.0)).procs.clone().unwrap().unwrap();
        assert_eq!(sum.top.len(), TOP_N);
        assert_eq!(sum.top[0].pid, 19);
    }

    #[test]
    fn failed_collectors_stay_isolated() {
        let mut s = Sampler::new();
        let mut a = raw((0, 0), 0, 0);
        a.net = Err(Missing::Denied);
        a.memory = Err(Missing::Failed);
        s.ingest(a.clone(), secs(0.0));
        let t = s.ingest(raw((1, 1), 0, 0), secs(1.0));
        assert_eq!(t.net, Err(Missing::Pending));
        assert_eq!(t.cpu_pct, Ok(50.0));
        let t = s.ingest(a, secs(2.0));
        assert_eq!(t.net, Err(Missing::Denied));
        assert_eq!(t.memory, Err(Missing::Failed));
    }
}
