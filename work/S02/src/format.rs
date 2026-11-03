//! Compact human-readable formatting for the display.

use std::time::Duration;

/// Binary-prefixed size with ~3 significant digits: "812M", "15.6G", "1.00T".
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1000.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{}B", n)
    } else if v < 10.0 {
        format!("{:.2}{}", v, UNITS[u])
    } else if v < 100.0 {
        format!("{:.1}{}", v, UNITS[u])
    } else {
        format!("{:.0}{}", v, UNITS[u])
    }
}

/// Transfer rate, e.g. "1.20M/s", "0B/s".
pub fn rate(bps: f64) -> String {
    let b = if bps.is_finite() && bps > 0.0 { bps.round() as u64 } else { 0 };
    format!("{}/s", bytes(b))
}

/// "3D 04:12" / "04:12:55" style uptime.
pub fn uptime(d: Duration) -> String {
    let s = d.as_secs();
    let (days, h, m, sec) = (s / 86_400, (s / 3600) % 24, (s / 60) % 60, s % 60);
    if days > 0 {
        format!("{}D {:02}:{:02}:{:02}", days, h, m, sec)
    } else {
        format!("{:02}:{:02}:{:02}", h, m, sec)
    }
}

pub fn pct(p: f64) -> String {
    if !p.is_finite() {
        return "--%".into();
    }
    format!("{:.0}%", p.clamp(0.0, 999.0))
}

/// Load average in at most four characters: "0.42", "12.3", "128".
pub fn load(l: f64) -> String {
    if !l.is_finite() || l < 0.0 {
        "--".into()
    } else if l < 10.0 {
        format!("{:.2}", l)
    } else if l < 100.0 {
        format!("{:.1}", l)
    } else {
        format!("{:.0}", l.min(9999.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_formatting() {
        assert_eq!(bytes(0), "0B");
        assert_eq!(bytes(999), "999B");
        assert_eq!(bytes(1536), "1.50K");
        assert_eq!(bytes(16 * 1024 * 1024 * 1024), "16.0G");
        assert_eq!(bytes(812 * 1024 * 1024), "812M");
        assert_eq!(bytes(u64::MAX), "16384P");
    }

    #[test]
    fn rate_formatting_is_robust() {
        assert_eq!(rate(f64::NAN), "0B/s");
        assert_eq!(rate(-5.0), "0B/s");
        assert_eq!(rate(2048.0), "2.00K/s");
    }

    #[test]
    fn uptime_formatting() {
        assert_eq!(uptime(Duration::from_secs(59)), "00:00:59");
        assert_eq!(uptime(Duration::from_secs(3 * 86_400 + 4 * 3600 + 12 * 60 + 5)), "3D 04:12:05");
    }

    #[test]
    fn load_formatting_fits_four_columns() {
        for l in [0.0, 0.42, 9.99, 12.34, 99.9, 128.0, 1e9] {
            assert!(load(l).len() <= 4, "{} -> {}", l, load(l));
        }
    }
}
