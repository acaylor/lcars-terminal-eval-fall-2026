use super::rate::IfaceSnap;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CpuStat {
    pub busy: u64,
    pub total: u64,
    pub cores: Vec<(u64, u64)>,
}

#[derive(Clone, Debug, Default)]
pub struct MemInfo {
    pub total: Option<u64>,
    pub available: Option<u64>,
    pub free: Option<u64>,
    pub buffers: Option<u64>,
    pub cached: Option<u64>,
    pub sreclaimable: Option<u64>,
    pub swap_total: Option<u64>,
    pub swap_free: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcStat {
    pub pid: u32,
    pub name: String,
    pub utime: u64,
    pub stime: u64,
    pub starttime: u64,
    pub rss_pages: u64,
}

pub fn parse_proc_stat(text: &str) -> Option<CpuStat> {
    let mut agg = None;
    let mut cores = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let label = parts.next()?;
        if label == "cpu" {
            agg = Some(cpu_fields(&mut parts)?);
        } else if label.starts_with("cpu")
            && label.as_bytes().get(3).is_some_and(|b| b.is_ascii_digit())
        {
            if let Some(v) = cpu_fields(&mut parts) {
                cores.push(v);
            }
        }
    }
    let (busy, total) = agg?;
    Some(CpuStat { busy, total, cores })
}

fn cpu_fields<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Option<(u64, u64)> {
    let user: u64 = parts.next()?.parse().ok()?;
    let nice: u64 = parts.next()?.parse().ok()?;
    let system: u64 = parts.next()?.parse().ok()?;
    let idle: u64 = parts.next()?.parse().ok()?;
    let iowait: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let irq: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let softirq: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let steal: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let idle_all = idle.saturating_add(iowait);
    let total = user
        .saturating_add(nice)
        .saturating_add(system)
        .saturating_add(idle_all)
        .saturating_add(irq)
        .saturating_add(softirq)
        .saturating_add(steal);
    let busy = total.saturating_sub(idle_all);
    Some((busy, total))
}

pub fn parse_meminfo(text: &str) -> MemInfo {
    let mut info = MemInfo::default();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else { continue };
        let Some(val) = parts.next().and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        let bytes = val.saturating_mul(1024);
        match key {
            "MemTotal:" => info.total = Some(bytes),
            "MemAvailable:" => info.available = Some(bytes),
            "MemFree:" => info.free = Some(bytes),
            "Buffers:" => info.buffers = Some(bytes),
            "Cached:" => info.cached = Some(bytes),
            "SReclaimable:" => info.sreclaimable = Some(bytes),
            "SwapTotal:" => info.swap_total = Some(bytes),
            "SwapFree:" => info.swap_free = Some(bytes),
            _ => {}
        }
    }
    info
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn memory_fields(info: &MemInfo) -> (Option<u64>, Option<u64>, Option<u64>) {
    let total = info.total;
    let available = info.available.or_else(|| {
        let free = info.free?;
        Some(
            free.saturating_add(info.buffers.unwrap_or(0))
                .saturating_add(info.cached.unwrap_or(0))
                .saturating_add(info.sreclaimable.unwrap_or(0)),
        )
    });
    let used = match (total, available) {
        (Some(t), Some(a)) => Some(t.saturating_sub(a.min(t))),
        _ => None,
    };
    (used, available, total)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn swap_fields(info: &MemInfo) -> (Option<u64>, Option<u64>) {
    match (info.swap_total, info.swap_free) {
        (Some(t), Some(f)) => (Some(t.saturating_sub(f.min(t))), Some(t)),
        (Some(t), None) => (None, Some(t)),
        _ => (None, None),
    }
}

pub fn parse_loadavg(text: &str) -> Option<[f64; 3]> {
    let mut parts = text.split_whitespace();
    let a: f64 = parts.next()?.parse().ok()?;
    let b: f64 = parts.next()?.parse().ok()?;
    let c: f64 = parts.next()?.parse().ok()?;
    if [a, b, c].iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    Some([a, b, c])
}

pub fn parse_uptime(text: &str) -> Option<Duration> {
    let secs: f64 = text.split_whitespace().next()?.parse().ok()?;
    if !secs.is_finite() || secs < 0.0 {
        return None;
    }
    Some(Duration::from_secs_f64(secs))
}

pub fn parse_net_dev(text: &str) -> Vec<IfaceSnap> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name == "lo" || name.starts_with("lo") {
            continue;
        }
        let nums: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if nums.len() < 9 {
            continue;
        }
        out.push(IfaceSnap {
            name: name.to_string(),
            rx: nums[0],
            tx: nums[8],
        });
    }
    out
}

pub fn parse_os_release(text: &str) -> Option<String> {
    let mut name = None;
    let mut pretty = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
            pretty = Some(unquote(v));
        } else if let Some(v) = line.strip_prefix("NAME=") {
            name = Some(unquote(v));
        }
    }
    pretty.or(name).filter(|s| !s.is_empty())
}

fn unquote(v: &str) -> String {
    v.trim().trim_matches('"').trim().to_string()
}

pub fn parse_proc_pid_stat(bytes: &[u8]) -> Option<ProcStat> {
    let open = bytes.iter().position(|b| *b == b'(')?;
    let close = bytes.iter().rposition(|b| *b == b')')?;
    if close <= open || open == 0 {
        return None;
    }
    let pid: u32 = std::str::from_utf8(&bytes[..open])
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let name = String::from_utf8_lossy(&bytes[open + 1..close])
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    let rest = std::str::from_utf8(&bytes[close + 1..]).ok()?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    if fields.len() < 22 {
        return None;
    }
    let utime: u64 = fields[11].parse().ok()?;
    let stime: u64 = fields[12].parse().ok()?;
    let starttime: u64 = fields[19].parse().ok()?;
    let rss_pages: u64 = fields[21].parse().ok()?;
    Some(ProcStat {
        pid,
        name,
        utime,
        stime,
        starttime,
        rss_pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_ignores_guest_double_count_and_parses_cores() {
        let text = "\
cpu  100 10 50 800 40 5 5 0 20 0
cpu0 40 0 10 100 0 0 0 0 0 0
cpu1 60 10 40 700 40 5 5 0 0 0
intr 1
";
        let s = parse_proc_stat(text).unwrap();
        assert_eq!(s.total, 100 + 10 + 50 + 800 + 40 + 5 + 5);
        assert_eq!(s.busy, s.total - 840);
        assert_eq!(s.cores.len(), 2);
    }

    #[test]
    fn meminfo_prefers_available_and_falls_back() {
        let modern = parse_meminfo("MemTotal: 1000 kB\nMemAvailable: 400 kB\nMemFree: 100 kB\nSwapTotal: 200 kB\nSwapFree: 50 kB\n");
        let (used, avail, total) = memory_fields(&modern);
        assert_eq!(total, Some(1000 * 1024));
        assert_eq!(avail, Some(400 * 1024));
        assert_eq!(used, Some(600 * 1024));
        let (su, st) = swap_fields(&modern);
        assert_eq!(su, Some(150 * 1024));
        assert_eq!(st, Some(200 * 1024));

        let old =
            parse_meminfo("MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 50 kB\nCached: 150 kB\n");
        let (used, avail, _) = memory_fields(&old);
        assert_eq!(avail, Some(300 * 1024));
        assert_eq!(used, Some(700 * 1024));
    }

    #[test]
    fn missing_mem_is_not_zero() {
        let info = parse_meminfo("Buffers: 10 kB\n");
        assert_eq!(memory_fields(&info), (None, None, None));
        assert_eq!(swap_fields(&info), (None, None));
    }

    #[test]
    fn net_dev_skips_loopback_and_short_lines() {
        let text = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 10 0 0 0 0 0 0 0 20 0 0 0 0 0 0 0
  eth0: 1000 1 0 0 0 0 0 0 250 1 0 0 0 0 0 0
  new0: 5
";
        let ifaces = parse_net_dev(text);
        assert_eq!(ifaces.len(), 1);
        assert_eq!(ifaces[0].name, "eth0");
        assert_eq!(ifaces[0].rx, 1000);
        assert_eq!(ifaces[0].tx, 250);
    }

    #[test]
    fn proc_stat_handles_spaces_parens_and_non_utf8() {
        let mut raw =
            b"42 (we (ird) \xff) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 4096 "
                .to_vec();
        // fields after comm: state ppid ... utime is 12th token? index 11
        // Let me rebuild explicitly.
        let _ = &mut raw;
        let text = b"7 (caf\xffe (x)) R 1 1 1 0 -1 0 0 0 0 0 15 25 0 0 20 0 1 0 999 0 12 0";
        let p = parse_proc_pid_stat(text).unwrap();
        assert_eq!(p.pid, 7);
        assert!(p.name.contains('x'));
        assert!(!p.name.chars().any(|c| c.is_control()));
        assert_eq!(p.utime, 15);
        assert_eq!(p.stime, 25);
        assert_eq!(p.starttime, 999);
        assert_eq!(p.rss_pages, 12);
    }

    #[test]
    fn load_and_uptime_reject_garbage() {
        assert_eq!(parse_loadavg("0.10 0.20 0.30 1/2 3"), Some([0.1, 0.2, 0.3]));
        assert!(parse_loadavg("0.1 0.2").is_none());
        assert!(parse_loadavg("-1 0 0").is_none());
        assert_eq!(
            parse_uptime("12.5 99").unwrap(),
            Duration::from_secs_f64(12.5)
        );
        assert!(parse_uptime("nope").is_none());
    }

    #[test]
    fn os_release_prefers_pretty_name() {
        let text = "NAME=\"Debian\"\nPRETTY_NAME=\"Debian GNU/Linux\"\n";
        assert_eq!(parse_os_release(text).as_deref(), Some("Debian GNU/Linux"));
    }
}
