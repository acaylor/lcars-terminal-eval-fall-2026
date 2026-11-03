//! Pure rate arithmetic. Everything here takes explicit timestamps so tests
//! can drive it deterministically.

use super::CpuTicks;
use std::time::Instant;

/// Width of the underlying hardware/kernel counter, which decides how a
/// decreasing value is interpreted: a 32-bit counter wrapped, a 64-bit one
/// was reset (interface re-created, kernel restart of the stat).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CounterWidth {
    /// Not produced by the bundled collectors; kept so a future source with
    /// 32-bit counters (e.g. `getifaddrs` on macOS) gets wrap handling.
    #[allow(dead_code)]
    Bits32,
    Bits64,
}

/// Tracks one monotonically increasing counter and yields units per second.
#[derive(Clone, Debug)]
pub struct RateTracker {
    width: CounterWidth,
    last: Option<(u64, Instant)>,
}

impl RateTracker {
    pub fn new(width: CounterWidth) -> RateTracker {
        RateTracker { width, last: None }
    }

    /// Feed a new reading. Returns `None` when no rate can be computed yet:
    /// first sample, zero elapsed time, or a counter reset.
    pub fn update(&mut self, value: u64, now: Instant) -> Option<f64> {
        let Some((prev, t0)) = self.last else {
            self.last = Some((value, now));
            return None;
        };
        let elapsed = now.saturating_duration_since(t0).as_secs_f64();
        if elapsed <= 0.0 {
            // Same instant (or clock did not advance): keep the old baseline so
            // the next real interval is measured from it.
            return None;
        }
        self.last = Some((value, now));
        let delta = if value >= prev {
            value - prev
        } else {
            match self.width {
                CounterWidth::Bits32 => (value as u128 + (1u128 << 32) - prev as u128) as u64,
                CounterWidth::Bits64 => return None,
            }
        };
        Some(delta as f64 / elapsed)
    }
}

/// Busy fraction between two cumulative tick readings, 0.0..=1.0.
pub fn cpu_fraction(prev: &CpuTicks, cur: &CpuTicks) -> Option<f32> {
    let total = cur.total().checked_sub(prev.total())?;
    if total == 0 {
        return None;
    }
    let busy = cur.busy.checked_sub(prev.busy)?;
    Some((busy as f64 / total as f64).clamp(0.0, 1.0) as f32)
}

/// Fixed-capacity history of samples for traces. `None` marks a gap
/// (metric unavailable at that time) so the trace can show it honestly.
#[derive(Clone, Debug)]
pub struct History {
    cap: usize,
    data: std::collections::VecDeque<Option<f32>>,
}

impl History {
    pub fn new(cap: usize) -> History {
        History {
            cap,
            data: std::collections::VecDeque::with_capacity(cap),
        }
    }
    pub fn push(&mut self, v: Option<f32>) {
        if self.data.len() == self.cap {
            self.data.pop_front();
        }
        self.data.push_back(v);
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    /// The most recent `n` samples, oldest first, padded on the left with
    /// `None` so the caller always gets exactly `n` slots.
    pub fn last_n(&self, n: usize) -> Vec<Option<f32>> {
        let mut out = vec![None; n.saturating_sub(self.data.len())];
        let skip = self.data.len().saturating_sub(n);
        out.extend(self.data.iter().skip(skip).copied());
        out
    }
    pub fn max(&self) -> f32 {
        self.data.iter().flatten().copied().fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn t(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn rate_uses_measured_elapsed_time() {
        let base = Instant::now();
        let mut r = RateTracker::new(CounterWidth::Bits64);
        assert_eq!(r.update(1000, t(base, 0)), None);
        // 1500 bytes over 1.5 s, not over a nominal 1 s interval.
        let v = r.update(2500, t(base, 1500)).unwrap();
        assert!((v - 1000.0).abs() < 1e-9, "{v}");
    }

    #[test]
    fn zero_elapsed_is_skipped_without_losing_baseline() {
        let base = Instant::now();
        let mut r = RateTracker::new(CounterWidth::Bits64);
        r.update(0, t(base, 0));
        assert_eq!(r.update(500, t(base, 0)), None);
        let v = r.update(1000, t(base, 1000)).unwrap();
        assert!((v - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn very_long_gap_still_produces_a_rate() {
        let base = Instant::now();
        let mut r = RateTracker::new(CounterWidth::Bits64);
        r.update(0, t(base, 0));
        let v = r.update(3_600_000, t(base, 3_600_000)).unwrap();
        assert!((v - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn counter_reset_on_64bit_yields_none_then_recovers() {
        let base = Instant::now();
        let mut r = RateTracker::new(CounterWidth::Bits64);
        r.update(5000, t(base, 0));
        assert_eq!(r.update(10, t(base, 1000)), None);
        let v = r.update(1010, t(base, 2000)).unwrap();
        assert!((v - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn counter_wrap_on_32bit_is_unwrapped() {
        let base = Instant::now();
        let mut r = RateTracker::new(CounterWidth::Bits32);
        r.update(u32::MAX as u64 - 99, t(base, 0));
        let v = r.update(100, t(base, 1000)).unwrap();
        assert!((v - 200.0).abs() < 1e-9, "{v}");
    }

    #[test]
    fn cpu_fraction_handles_resets_and_idle_boxes() {
        let a = CpuTicks { busy: 100, idle: 900 };
        let b = CpuTicks { busy: 150, idle: 950 };
        assert!((cpu_fraction(&a, &b).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(cpu_fraction(&a, &a), None);
        assert_eq!(cpu_fraction(&b, &a), None);
    }

    #[test]
    fn history_is_bounded_and_left_padded() {
        let mut h = History::new(3);
        for i in 0..5 {
            h.push(Some(i as f32));
        }
        assert_eq!(h.len(), 3);
        assert_eq!(h.last_n(5), vec![None, None, Some(2.0), Some(3.0), Some(4.0)]);
        assert_eq!(h.last_n(2), vec![Some(3.0), Some(4.0)]);
        assert_eq!(h.max(), 4.0);
    }
}
