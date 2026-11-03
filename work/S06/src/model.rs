use std::collections::VecDeque;
use std::time::Duration;

pub const HISTORY_CAPACITY: usize = 180;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub time: String,
    pub date: String,
    pub hostname: String,
    pub os: String,
    pub architecture: String,
    pub uptime: Option<Duration>,
    pub cpu: Option<f32>,
    pub cores: Vec<f32>,
    pub load: Option<[f64; 3]>,
    pub memory: Option<Memory>,
    pub swap: Option<Swap>,
    pub disk: Option<Disk>,
    pub network: Option<NetworkRates>,
    pub process_count: Option<usize>,
    pub top_processes: Vec<Process>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    pub used: u64,
    pub available: u64,
    pub total: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swap {
    pub used: u64,
    pub total: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Disk {
    pub used: u64,
    pub available: u64,
    pub total: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetworkRates {
    pub receive: f64,
    pub transmit: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkCounters {
    pub received: u64,
    pub transmitted: u64,
}

#[derive(Debug, Default)]
pub struct RateTracker {
    previous: Option<NetworkCounters>,
}

impl RateTracker {
    /// A new baseline deliberately produces no rate. Reset counters and a zero
    /// sampling interval also produce no rate rather than a negative or infinite value.
    pub fn update(&mut self, current: NetworkCounters, elapsed: Duration) -> Option<NetworkRates> {
        let previous = self.previous.replace(current)?;
        if elapsed.is_zero()
            || current.received < previous.received
            || current.transmitted < previous.transmitted
        {
            return None;
        }
        let seconds = elapsed.as_secs_f64();
        Some(NetworkRates {
            receive: (current.received - previous.received) as f64 / seconds,
            transmit: (current.transmitted - previous.transmitted) as f64 / seconds,
        })
    }

    pub fn reset(&mut self) {
        self.previous = None;
    }
}

#[derive(Debug)]
pub struct History {
    samples: VecDeque<f32>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            samples: VecDeque::with_capacity(HISTORY_CAPACITY),
        }
    }
}

impl History {
    pub fn push(&mut self, value: f32) {
        if self.samples.len() == HISTORY_CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(value.clamp(0.0, 100.0));
    }

    pub fn samples(&self) -> impl Iterator<Item = f32> + '_ {
        self.samples.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

pub fn percent(part: u64, total: u64) -> Option<f32> {
    (total > 0).then(|| (part as f64 / total as f64 * 100.0).clamp(0.0, 100.0) as f32)
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 100.0 {
        format!("{value:.0}{}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1}{}", UNITS[unit])
    } else {
        format!("{value:.2}{}", UNITS[unit])
    }
}

pub fn format_rate(bytes_per_second: f64) -> String {
    if !bytes_per_second.is_finite() || bytes_per_second < 0.0 {
        return "—".into();
    }
    format!("{}/s", format_bytes(bytes_per_second as u64))
}

pub fn format_uptime(duration: Option<Duration>) -> String {
    let Some(seconds) = duration.map(|d| d.as_secs()) else {
        return "UNAVAILABLE".into();
    };
    let days = seconds / 86_400;
    let hours = seconds / 3_600 % 24;
    let minutes = seconds / 60 % 60;
    if days > 0 {
        format!("{days}d {hours:02}h {minutes:02}m")
    } else {
        format!("{hours:02}h {minutes:02}m {:02}s", seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_use_real_elapsed_time() {
        let mut tracker = RateTracker::default();
        let first = NetworkCounters {
            received: 1_000,
            transmitted: 500,
        };
        assert_eq!(tracker.update(first, Duration::ZERO), None);
        let rates = tracker
            .update(
                NetworkCounters {
                    received: 3_000,
                    transmitted: 1_500,
                },
                Duration::from_millis(500),
            )
            .unwrap();
        assert_eq!(rates.receive, 4_000.0);
        assert_eq!(rates.transmit, 2_000.0);
    }

    #[test]
    fn rates_handle_reset_and_zero_or_long_intervals() {
        let mut tracker = RateTracker::default();
        tracker.update(
            NetworkCounters {
                received: 900,
                transmitted: 900,
            },
            Duration::from_secs(1),
        );
        assert_eq!(
            tracker.update(
                NetworkCounters {
                    received: 1,
                    transmitted: 1
                },
                Duration::from_secs(1)
            ),
            None
        );
        assert_eq!(
            tracker.update(
                NetworkCounters {
                    received: 1,
                    transmitted: 1
                },
                Duration::ZERO
            ),
            None
        );
        let rate = tracker
            .update(
                NetworkCounters {
                    received: 2,
                    transmitted: 3,
                },
                Duration::from_secs(3_600),
            )
            .unwrap();
        assert!(rate.receive < 0.001);
        assert!(rate.transmit < 0.001);
    }

    #[test]
    fn history_is_bounded() {
        let mut history = History::default();
        for n in 0..1_000 {
            history.push(n as f32);
        }
        assert_eq!(history.len(), HISTORY_CAPACITY);
        assert_eq!(history.samples().next(), Some(100.0));
        assert_eq!(history.samples().last(), Some(100.0));
    }

    #[test]
    fn byte_formatting_is_compact() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(1024), "1.00K");
        assert_eq!(format_bytes(15 * 1024 * 1024), "15.0M");
        assert_eq!(format_bytes(200 * 1024 * 1024 * 1024), "200G");
    }
}
