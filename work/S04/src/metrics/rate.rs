#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfaceSnap {
    pub name: String,
    pub rx: u64,
    pub tx: u64,
}

pub fn finite_elapsed(elapsed: f64) -> Option<f64> {
    if elapsed.is_finite() && elapsed > 0.0 {
        Some(elapsed)
    } else {
        None
    }
}

pub fn counter_delta(prev: u64, now: u64) -> Option<u64> {
    if now < prev {
        None
    } else {
        Some(now - prev)
    }
}

pub fn cpu_percent(prev_busy: u64, prev_total: u64, busy: u64, total: u64) -> Option<f64> {
    let d_total = counter_delta(prev_total, total)?;
    if d_total == 0 {
        return None;
    }
    let d_busy = counter_delta(prev_busy, busy)?;
    Some((d_busy as f64) * 100.0 / (d_total as f64))
}

pub fn process_cpu_ticks(prev: u64, now: u64, elapsed: f64, clk_tck: f64) -> Option<f64> {
    if !clk_tck.is_finite() || clk_tck <= 0.0 {
        return None;
    }
    let d = counter_delta(prev, now)?;
    let e = finite_elapsed(elapsed)?;
    Some((d as f64) / clk_tck / e * 100.0)
}

pub fn process_cpu_ns(prev: u64, now: u64, elapsed: f64) -> Option<f64> {
    let d = counter_delta(prev, now)?;
    let e = finite_elapsed(elapsed)?;
    Some((d as f64) / 1.0e9 / e * 100.0)
}

pub fn iface_rates(prev: &[IfaceSnap], now: &[IfaceSnap], elapsed: f64) -> Option<(f64, f64)> {
    let e = finite_elapsed(elapsed)?;
    if prev.is_empty() {
        return None;
    }
    let mut rx = 0u64;
    let mut tx = 0u64;
    let mut matched = 0u32;
    let mut reset = 0u32;
    for n in now {
        if let Some(p) = prev.iter().find(|p| p.name == n.name) {
            match (counter_delta(p.rx, n.rx), counter_delta(p.tx, n.tx)) {
                (Some(dr), Some(dt)) => {
                    rx = rx.saturating_add(dr);
                    tx = tx.saturating_add(dt);
                    matched += 1;
                }
                _ => reset += 1,
            }
        }
    }
    if matched == 0 {
        if reset > 0 {
            return None;
        }
        if now.is_empty() {
            return Some((0.0, 0.0));
        }
        return None;
    }
    Some((rx as f64 / e, tx as f64 / e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_uses_elapsed_not_assumed_interval() {
        let pct = cpu_percent(0, 0, 25, 100).unwrap();
        assert!((pct - 25.0).abs() < 1e-9);
        let slow = process_cpu_ticks(0, 100, 10.0, 100.0).unwrap();
        assert!((slow - 10.0).abs() < 1e-9);
        let fast = process_cpu_ticks(0, 100, 0.5, 100.0).unwrap();
        assert!((fast - 200.0).abs() < 1e-6);
    }

    #[test]
    fn zero_and_negative_elapsed_are_unavailable() {
        assert!(cpu_percent(0, 0, 1, 2).is_some());
        assert!(process_cpu_ticks(0, 10, 0.0, 100.0).is_none());
        assert!(process_cpu_ticks(0, 10, -1.0, 100.0).is_none());
        assert!(process_cpu_ns(0, 1_000_000_000, f64::NAN).is_none());
        assert!(iface_rates(&[], &[], 1.0).is_none());
    }

    #[test]
    fn rollover_or_reset_does_not_spike() {
        assert!(cpu_percent(50, 100, 10, 40).is_none());
        assert!(process_cpu_ticks(500, 20, 1.0, 100.0).is_none());
        assert!(process_cpu_ns(9_000, 10, 1.0).is_none());
        let prev = vec![IfaceSnap {
            name: "eth0".into(),
            rx: 1_000,
            tx: 2_000,
        }];
        let now = vec![IfaceSnap {
            name: "eth0".into(),
            rx: 10,
            tx: 2_100,
        }];
        assert!(iface_rates(&prev, &now, 1.0).is_none());
    }

    #[test]
    fn long_interval_is_an_average_not_a_guess() {
        let prev = vec![IfaceSnap {
            name: "en0".into(),
            rx: 0,
            tx: 0,
        }];
        let now = vec![IfaceSnap {
            name: "en0".into(),
            rx: 10_000,
            tx: 5_000,
        }];
        let (rx, tx) = iface_rates(&prev, &now, 10_000.0).unwrap();
        assert!((rx - 1.0).abs() < 1e-9);
        assert!((tx - 0.5).abs() < 1e-9);
    }

    #[test]
    fn new_and_gone_interfaces_do_not_invent_traffic() {
        let prev = vec![
            IfaceSnap {
                name: "eth0".into(),
                rx: 100,
                tx: 50,
            },
            IfaceSnap {
                name: "wlan0".into(),
                rx: 9_000,
                tx: 9_000,
            },
        ];
        let now = vec![
            IfaceSnap {
                name: "eth0".into(),
                rx: 300,
                tx: 80,
            },
            IfaceSnap {
                name: "new0".into(),
                rx: 5_000_000,
                tx: 5_000_000,
            },
        ];
        let (rx, tx) = iface_rates(&prev, &now, 2.0).unwrap();
        assert!((rx - 100.0).abs() < 1e-9);
        assert!((tx - 15.0).abs() < 1e-9);
    }

    #[test]
    fn all_interfaces_gone_is_zero_not_a_crash() {
        let prev = vec![IfaceSnap {
            name: "eth0".into(),
            rx: 10,
            tx: 10,
        }];
        let (rx, tx) = iface_rates(&prev, &[], 1.0).unwrap();
        assert_eq!((rx, tx), (0.0, 0.0));
    }
}
