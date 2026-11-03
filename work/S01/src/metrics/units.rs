//! Number formatting shared by all views. Kept with the metrics so tests can
//! pin the exact strings the composition relies on for alignment.

use std::time::Duration;

/// Bytes with a one-letter binary-ish suffix, always 4-5 characters wide.
pub fn bytes(b: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1000.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b}B")
    } else if v < 10.0 {
        format!("{v:.1}{}", UNITS[i])
    } else {
        format!("{v:.0}{}", UNITS[i])
    }
}

/// Bytes per second as a fixed-width "123 KB/S" style string.
pub fn rate(bps: f64) -> String {
    const UNITS: [&str; 5] = ["B/S", "KB/S", "MB/S", "GB/S", "TB/S"];
    let mut v = bps.max(0.0);
    let mut i = 0;
    while v >= 1000.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:>3.0} {}", UNITS[i])
    } else if v < 10.0 {
        format!("{v:>3.1} {}", UNITS[i])
    } else {
        format!("{v:>3.0} {}", UNITS[i])
    }
}

pub fn percent(f: Option<f32>) -> String {
    match f {
        Some(v) => format!("{:>3.0}%", (v * 100.0).clamp(0.0, 100.0)),
        None => " ──%".into(),
    }
}

/// Uptime as "3D 04:12:55" (days only when non-zero).
pub fn uptime(d: Duration) -> String {
    let s = d.as_secs();
    let (days, rem) = (s / 86_400, s % 86_400);
    let (h, m, sec) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    if days > 0 {
        format!("{days}D {h:02}:{m:02}:{sec:02}")
    } else {
        format!("{h:02}:{m:02}:{sec:02}")
    }
}

/// Compact uptime for narrow rail labels: "3D 04H" or "04:12".
pub fn uptime_short(d: Duration) -> String {
    let s = d.as_secs();
    let (days, rem) = (s / 86_400, s % 86_400);
    let (h, m) = (rem / 3600, (rem % 3600) / 60);
    if days > 0 {
        format!("{days}D {h:02}H")
    } else {
        format!("{h:02}:{m:02}")
    }
}

pub fn load(v: f64) -> String {
    if v >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_formatting() {
        assert_eq!(bytes(0), "0B");
        assert_eq!(bytes(999), "999B");
        assert_eq!(bytes(1536), "1.5K");
        assert_eq!(bytes(12_345_678), "12M");
        assert_eq!(bytes(97_979_612 * 1024), "93G");
        assert_eq!(bytes(2 * 1024u64.pow(4)), "2.0T");
    }

    #[test]
    fn rate_formatting_is_fixed_width() {
        assert_eq!(rate(0.0), "  0 B/S");
        assert_eq!(rate(1234.0), "1.2 KB/S");
        assert_eq!(rate(123_456_789.0), "118 MB/S");
        assert_eq!(rate(-5.0), "  0 B/S");
    }

    #[test]
    fn uptime_formatting() {
        assert_eq!(
            uptime(Duration::from_secs(3 * 86400 + 4 * 3600 + 12 * 60 + 55)),
            "3D 04:12:55"
        );
        assert_eq!(uptime(Duration::from_secs(59)), "00:00:59");
        assert_eq!(uptime_short(Duration::from_secs(3 * 86400 + 4 * 3600)), "3D 04H");
        assert_eq!(uptime_short(Duration::from_secs(4 * 3600 + 120)), "04:02");
    }

    #[test]
    fn percent_and_load() {
        assert_eq!(percent(Some(0.374)), " 37%");
        assert_eq!(percent(Some(2.0)), "100%");
        assert_eq!(percent(None), " ──%");
        assert_eq!(load(0.42), "0.42");
        assert_eq!(load(123.4), "123");
    }
}
