use std::collections::BTreeMap;
use std::time::Duration;

use unicode_width::UnicodeWidthChar;

pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LoadAvg {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub hostname: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub kernel: Option<String>,
    pub arch: String,
    pub uptime_secs: Option<u64>,
    pub load: Option<LoadAvg>,
    pub cpu: Option<f32>,
    pub per_core: Vec<f32>,
    pub mem_total: Option<u64>,
    pub mem_used: Option<u64>,
    pub mem_available: Option<u64>,
    pub swap_total: Option<u64>,
    pub swap_used: Option<u64>,
    pub disk_mount: Option<String>,
    pub disk_total: Option<u64>,
    pub disk_available: Option<u64>,
    pub net_rx_rate: Option<f64>,
    pub net_tx_rate: Option<f64>,
    pub net_ifaces: usize,
    pub process_count: Option<usize>,
    pub top_processes: Vec<ProcInfo>,
    pub collect_error: Option<String>,
}

impl Metrics {
    pub fn cpu_pct(&self) -> Option<f32> {
        self.cpu
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 100.0))
    }

    pub fn mem_pct(&self) -> Option<f32> {
        match (self.mem_total, self.mem_used) {
            (Some(total), Some(used)) if total > 0 => {
                Some((used as f64 / total as f64 * 100.0) as f32)
            }
            _ => None,
        }
    }

    pub fn disk_pct(&self) -> Option<f32> {
        match (self.disk_total, self.disk_available) {
            (Some(total), Some(avail)) if total > 0 => {
                let used = total.saturating_sub(avail);
                Some((used as f64 / total as f64 * 100.0) as f32)
            }
            _ => None,
        }
    }

    pub fn swap_pct(&self) -> Option<f32> {
        match (self.swap_total, self.swap_used) {
            (Some(total), Some(used)) if total > 0 => {
                Some((used as f64 / total as f64 * 100.0) as f32)
            }
            _ => None,
        }
    }
}

/// Tracks cumulative network counters and turns them into rates.
///
/// Counters are kept per interface so an interface appearing or disappearing cannot
/// manufacture a burst of traffic. A counter moving backwards is treated as a reset
/// and skipped for that interval.
#[derive(Debug, Default)]
pub struct RateTracker {
    prev: BTreeMap<String, (u64, u64)>,
    pub last_valid: Option<(f64, f64)>,
}

impl RateTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds the baseline without producing a rate.
    pub fn prime<I>(&mut self, totals: I)
    where
        I: IntoIterator<Item = (String, u64, u64)>,
    {
        self.prev.clear();
        for (name, rx, tx) in totals {
            self.prev.insert(name, (rx, tx));
        }
        self.last_valid = None;
    }

    /// `totals` is the current cumulative counter for each interface.
    /// Returns aggregate bytes/second when a meaningful interval is available.
    pub fn update<I>(&mut self, totals: I, elapsed: Duration) -> Option<(f64, f64)>
    where
        I: IntoIterator<Item = (String, u64, u64)>,
    {
        let secs = elapsed.as_secs_f64();
        let mut rx_delta = 0u64;
        let mut tx_delta = 0u64;
        let mut compared = false;
        let mut next = BTreeMap::new();

        for (name, rx, tx) in totals {
            if let Some(&(prev_rx, prev_tx)) = self.prev.get(&name) {
                // Both counters must move forward. A reset on either counter makes
                // this interface's interval unknowable, so it contributes nothing.
                if rx >= prev_rx && tx >= prev_tx {
                    rx_delta = rx_delta.saturating_add(rx - prev_rx);
                    tx_delta = tx_delta.saturating_add(tx - prev_tx);
                    compared = true;
                }
            }
            next.insert(name, (rx, tx));
        }
        self.prev = next;

        if !compared || secs < 0.05 || secs > 30.0 {
            self.last_valid = None;
            return None;
        }

        let rate = (rx_delta as f64 / secs, tx_delta as f64 / secs);
        self.last_valid = Some(rate);
        Some(rate)
    }
}

pub fn select_top_processes(mut procs: Vec<ProcInfo>, limit: usize) -> Vec<ProcInfo> {
    procs.retain(|p| p.cpu.is_finite() && p.cpu >= 0.0);
    procs.sort_by(|a, b| {
        b.cpu
            .partial_cmp(&a.cpu)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.memory.cmp(&a.memory))
            .then_with(|| a.pid.cmp(&b.pid))
    });
    procs.truncate(limit);
    procs
}

/// Makes an arbitrary OS string safe and single-width for the fixed LCARS grid.
pub fn sanitize_ui_text(input: &str, max_cols: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    let mut last_was_space = false;

    for ch in input.chars() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            if !last_was_space && used < max_cols {
                out.push(' ');
                used += 1;
                last_was_space = true;
            }
            continue;
        }
        let ch = if ch.is_control() { '?' } else { ch };
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w == 0 {
            continue;
        }
        if w != 1 || used + 1 > max_cols {
            if used < max_cols {
                out.push('·');
                used += 1;
                last_was_space = false;
            }
            continue;
        }
        out.push(ch);
        used += 1;
        last_was_space = ch == ' ';
    }
    out.trim_end().to_string()
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = "K";
    for (name, threshold) in [("M", 1024.0), ("G", 1024.0), ("T", 1024.0)] {
        if value >= threshold {
            value /= 1024.0;
            unit = name;
        } else {
            break;
        }
    }
    if value >= 100.0 {
        format!("{value:.0}{unit}")
    } else if value >= 10.0 {
        format!("{value:.1}{unit}")
    } else {
        format!("{value:.2}{unit}")
    }
}

pub fn format_bytes_compact(bytes: u64) -> String {
    format_bytes(bytes).replace(' ', "")
}

pub fn format_rate(bytes_per_sec: f64) -> String {
    if !bytes_per_sec.is_finite() || bytes_per_sec < 0.0 {
        return "N/A".to_string();
    }
    if bytes_per_sec < 1024.0 {
        return format!("{:.0} B/s", bytes_per_sec);
    }
    let kb = bytes_per_sec / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.1} KB/s");
    }
    let mb = kb / 1024.0;
    if mb < 1024.0 {
        return format!("{mb:.1} MB/s");
    }
    format!("{:.2} GB/s", mb / 1024.0)
}

pub fn format_rate_compact(bytes_per_sec: f64) -> String {
    format_rate(bytes_per_sec).replace(' ', "")
}

pub fn format_uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;
    let seconds = secs % 60;
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }
}

pub fn format_uptime_compact(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;
    if days > 0 {
        format!("{days}d{hours:02}h{minutes:02}m")
    } else {
        format!("{hours:02}h{minutes:02}m")
    }
}

pub fn format_pct(value: f32) -> String {
    if value.is_finite() {
        format!("{:.0}%", value.clamp(0.0, 100.0))
    } else {
        "N/A".to_string()
    }
}

pub fn format_load(value: f64) -> String {
    if value.is_finite() && value >= 0.0 {
        format!("{value:.2}")
    } else {
        "N/A".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, name: &str, cpu: f32, memory: u64) -> ProcInfo {
        ProcInfo {
            pid,
            name: name.to_string(),
            cpu,
            memory,
        }
    }

    #[test]
    fn rates_need_two_samples() {
        let mut t = RateTracker::new();
        t.prime([("eth0".to_string(), 100, 200)]);
        assert_eq!(
            t.update([("eth0".to_string(), 1100, 2200)], Duration::from_secs(1)),
            Some((1000.0, 2000.0))
        );
        let r = t
            .update([("eth0".to_string(), 2100, 3200)], Duration::from_secs(1))
            .unwrap();
        assert_eq!(r, (1000.0, 1000.0));
    }

    #[test]
    fn rates_survive_interface_changes() {
        let mut t = RateTracker::new();
        t.prime([("eth0".to_string(), 0, 0), ("wlan0".to_string(), 0, 0)]);
        let _ = t.update(
            [
                ("eth0".to_string(), 500, 500),
                ("wlan0".to_string(), 100, 100),
            ],
            Duration::from_secs(1),
        );
        let r = t
            .update([("wlan0".to_string(), 300, 300)], Duration::from_secs(1))
            .unwrap();
        assert_eq!(r, (200.0, 200.0));
        assert_eq!(
            t.update([("eth0".to_string(), 1, 1)], Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn rates_ignore_resets_and_bad_intervals() {
        let mut t = RateTracker::new();
        t.prime([("eth0".to_string(), 100, 100)]);
        assert_eq!(
            t.update([("eth0".to_string(), 50, 50)], Duration::from_secs(1)),
            None
        );
        let _ = t.update([("eth0".to_string(), 150, 150)], Duration::from_secs(1));
        assert_eq!(
            t.update([("eth0".to_string(), 250, 250)], Duration::from_millis(10)),
            None
        );
        let _ = t.update([("eth0".to_string(), 350, 350)], Duration::from_secs(1));
        assert_eq!(
            t.update([("eth0".to_string(), 450, 450)], Duration::from_secs(31)),
            None
        );
    }

    #[test]
    fn top_processes_are_stable_and_limited() {
        let procs = vec![
            proc(1, "a", 1.0, 10),
            proc(2, "b", 9.0, 20),
            proc(3, "c", 9.0, 30),
            proc(4, "d", f32::NAN, 99),
            proc(5, "e", -1.0, 99),
        ];
        let top = select_top_processes(procs, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].pid, 3);
        assert_eq!(top[1].pid, 2);
    }

    #[test]
    fn sanitizes_long_and_wide_names() {
        assert_eq!(sanitize_ui_text("a\u{7}b", 10), "a?b");
        assert_eq!(sanitize_ui_text("日本語", 10), "···");
        assert_eq!(sanitize_ui_text("hello world", 7), "hello w");
        assert_eq!(sanitize_ui_text("café", 10), "café");
    }

    #[test]
    fn formats_bytes_and_rates() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.50K");
        assert_eq!(format_rate(2048.0), "2.0 KB/s");
        assert_eq!(format_uptime(90_000), "1d 01:00:00");
        assert_eq!(format_uptime(3661), "01:01:01");
    }

    #[test]
    fn percentages_are_clamped() {
        assert_eq!(format_pct(-4.0), "0%");
        assert_eq!(format_pct(104.0), "100%");
        assert_eq!(format_pct(f32::INFINITY), "N/A");
    }
}
