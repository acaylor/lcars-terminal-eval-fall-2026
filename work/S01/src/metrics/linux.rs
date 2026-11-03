//! Linux collector: `/proc` text interfaces plus `statvfs`. Each parser is a
//! pure function over a string so it can be tested with captured fixtures.

use super::{
    hostname, uname_release, Collector, CpuTicks, Disk, Interface, Memory, ProcessSample, Snapshot, StaticInfo, Swap,
};
use std::collections::BTreeMap;
use std::fs;
use std::time::{Duration, Instant};

pub struct LinuxCollector {
    info: StaticInfo,
    clk_tck: f64,
    page_size: u64,
    warnings: BTreeMap<&'static str, String>,
}

impl LinuxCollector {
    pub fn new() -> LinuxCollector {
        let clk_tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let cores = fs::read_to_string("/proc/stat")
            .map(|s| parse_proc_stat(&s).1.len())
            .unwrap_or(0);
        LinuxCollector {
            info: StaticInfo {
                hostname: hostname(),
                os: os_release_name().unwrap_or_else(|| "Linux".into()),
                kernel: uname_release(),
                arch: std::env::consts::ARCH.to_string(),
                cores,
            },
            clk_tck: if clk_tck > 0 { clk_tck as f64 } else { 100.0 },
            page_size: if page_size > 0 { page_size as u64 } else { 4096 },
            warnings: BTreeMap::new(),
        }
    }

    fn read<T>(&mut self, key: &'static str, path: &str, parse: impl FnOnce(&str) -> Option<T>) -> Option<T> {
        match fs::read_to_string(path) {
            Ok(s) => {
                let v = parse(&s);
                if v.is_none() {
                    self.warnings.insert(key, format!("{path}: unexpected format"));
                }
                v
            }
            Err(e) => {
                self.warnings.insert(key, format!("{path}: {e}"));
                None
            }
        }
    }
}

impl Default for LinuxCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for LinuxCollector {
    fn static_info(&self) -> StaticInfo {
        self.info.clone()
    }

    fn sample(&mut self, want_processes: bool) -> Snapshot {
        let mut snap = Snapshot::empty(Instant::now());
        if let Some((total, cores)) = self.read("cpu", "/proc/stat", |s| {
            let (t, c) = parse_proc_stat(s);
            t.map(|t| (t, c))
        }) {
            snap.cpu_total = Some(total);
            snap.cpu_cores = cores;
        }
        snap.load = self.read("load", "/proc/loadavg", parse_loadavg);
        if let Some((mem, swap)) = self.read("memory", "/proc/meminfo", parse_meminfo) {
            snap.memory = Some(mem);
            snap.swap = swap;
        }
        snap.uptime = self.read("uptime", "/proc/uptime", parse_uptime);
        snap.interfaces = self.read("network", "/proc/net/dev", |s| Some(parse_net_dev(s)));
        snap.disk = match statvfs_disk("/") {
            Ok(d) => Some(d),
            Err(e) => {
                self.warnings.insert("disk", format!("statvfs(/): {e}"));
                None
            }
        };
        if want_processes {
            let (procs, count) = scan_processes(self.clk_tck, self.page_size);
            snap.process_count = Some(count);
            snap.processes = Some(procs);
        }
        snap
    }

    fn warnings(&self) -> Vec<String> {
        self.warnings.values().cloned().collect()
    }
}

/// `/proc/stat`: aggregate line first, then one line per core. Busy excludes
/// idle and iowait; guest time is already folded into user/nice by the kernel.
pub fn parse_proc_stat(s: &str) -> (Option<CpuTicks>, Vec<CpuTicks>) {
    let mut total = None;
    let mut cores = Vec::new();
    for line in s.lines() {
        let mut it = line.split_ascii_whitespace();
        let Some(name) = it.next() else { continue };
        if !name.starts_with("cpu") {
            continue;
        }
        let v: Vec<u64> = it.map(|f| f.parse().unwrap_or(0)).collect();
        if v.len() < 4 {
            continue;
        }
        let idle = v[3] + v.get(4).copied().unwrap_or(0);
        let busy: u64 = v
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 3 && *i != 4)
            .map(|(_, x)| *x)
            .sum();
        let ticks = CpuTicks { busy, idle };
        if name == "cpu" {
            total = Some(ticks);
        } else {
            cores.push(ticks);
        }
    }
    (total, cores)
}

pub fn parse_loadavg(s: &str) -> Option<[f64; 3]> {
    let mut it = s.split_ascii_whitespace();
    let a = it.next()?.parse().ok()?;
    let b = it.next()?.parse().ok()?;
    let c = it.next()?.parse().ok()?;
    Some([a, b, c])
}

pub fn parse_meminfo(s: &str) -> Option<(Memory, Option<Swap>)> {
    let mut map = BTreeMap::new();
    for line in s.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let kb: u64 = v
                .trim()
                .split_ascii_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            map.insert(k.trim(), kb * 1024);
        }
    }
    let total = *map.get("MemTotal")?;
    // MemAvailable exists since Linux 3.14; older kernels get the classic estimate.
    let available = map.get("MemAvailable").copied().unwrap_or_else(|| {
        map.get("MemFree").copied().unwrap_or(0)
            + map.get("Buffers").copied().unwrap_or(0)
            + map.get("Cached").copied().unwrap_or(0)
    });
    let mem = Memory {
        total,
        used: total.saturating_sub(available),
        available: available.min(total),
    };
    let swap = match (map.get("SwapTotal"), map.get("SwapFree")) {
        (Some(&t), Some(&f)) if t > 0 => Some(Swap {
            total: t,
            used: t.saturating_sub(f),
        }),
        _ => None,
    };
    Some((mem, swap))
}

pub fn parse_uptime(s: &str) -> Option<Duration> {
    let secs: f64 = s.split_ascii_whitespace().next()?.parse().ok()?;
    Some(Duration::from_secs_f64(secs.max(0.0)))
}

/// `/proc/net/dev`, skipping loopback. Field 0 is rx bytes, field 8 tx bytes.
pub fn parse_net_dev(s: &str) -> Vec<Interface> {
    let mut out = Vec::new();
    for line in s.lines().skip(2) {
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name == "lo" || name.is_empty() {
            continue;
        }
        let f: Vec<u64> = rest.split_ascii_whitespace().map(|x| x.parse().unwrap_or(0)).collect();
        if f.len() < 9 {
            continue;
        }
        out.push(Interface {
            name: name.to_string(),
            rx_bytes: f[0],
            tx_bytes: f[8],
        });
    }
    out
}

/// `/proc/<pid>/stat`. The command name is wrapped in parentheses and may
/// itself contain spaces, parentheses or non-ASCII bytes, so the fields are
/// split from the *last* closing parenthesis onward.
pub fn parse_pid_stat(s: &str, clk_tck: f64, page_size: u64) -> Option<ProcessSample> {
    let open = s.find('(')?;
    let close = s.rfind(')')?;
    let pid: i32 = s[..open].trim().parse().ok()?;
    let name = s[open + 1..close].to_string();
    let rest: Vec<&str> = s[close + 1..].split_ascii_whitespace().collect();
    // rest[0] = state (field 3); utime is field 14, stime 15, rss 24.
    let utime: u64 = rest.get(11)?.parse().ok()?;
    let stime: u64 = rest.get(12)?.parse().ok()?;
    let rss_pages: u64 = rest.get(21)?.parse().ok()?;
    Some(ProcessSample {
        pid,
        name,
        cpu_time: Duration::from_secs_f64((utime + stime) as f64 / clk_tck),
        rss: rss_pages.saturating_mul(page_size),
    })
}

/// Walk `/proc`. Processes that exit mid-scan just fail to read and are
/// skipped; permission problems on individual entries are likewise ignored.
fn scan_processes(clk_tck: f64, page_size: u64) -> (Vec<ProcessSample>, usize) {
    let mut procs = Vec::new();
    let mut count = 0usize;
    let Ok(dir) = fs::read_dir("/proc") else {
        return (procs, 0);
    };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        count += 1;
        let path = format!("/proc/{name}/stat");
        if let Ok(bytes) = fs::read(&path) {
            let text = String::from_utf8_lossy(&bytes);
            if let Some(p) = parse_pid_stat(&text, clk_tck, page_size) {
                procs.push(p);
            }
        }
    }
    (procs, count)
}

fn os_release_name() -> Option<String> {
    let s = fs::read_to_string("/etc/os-release").ok()?;
    s.lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
        .filter(|v| !v.is_empty())
}

pub fn statvfs_disk(mount: &str) -> Result<Disk, std::io::Error> {
    let path = std::ffi::CString::new(mount).map_err(|_| std::io::Error::other("bad path"))?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut st) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let frsize = st.f_frsize as u64;
    let total = st.f_blocks as u64 * frsize;
    let free = st.f_bfree as u64 * frsize;
    let available = st.f_bavail as u64 * frsize;
    Ok(Disk {
        mount: mount.to_string(),
        total,
        used: total.saturating_sub(free),
        available,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_stat_parsing() {
        let s = "cpu  100 5 50 1000 20 0 3 0 0 0\ncpu0 60 5 25 500 10 0 1 0 0 0\ncpu1 40 0 25 500 10 0 2 0 0 0\nintr 1 2 3\nctxt 5\n";
        let (t, cores) = parse_proc_stat(s);
        assert_eq!(t, Some(CpuTicks { busy: 158, idle: 1020 }));
        assert_eq!(cores.len(), 2);
        assert_eq!(cores[0], CpuTicks { busy: 91, idle: 510 });
        assert_eq!(parse_proc_stat("garbage").0, None);
    }

    #[test]
    fn loadavg_and_uptime() {
        assert_eq!(
            parse_loadavg("0.01 0.03 0.05 1/1866 907812\n"),
            Some([0.01, 0.03, 0.05])
        );
        assert_eq!(parse_loadavg("x"), None);
        assert_eq!(
            parse_uptime("12345.67 99999.0\n"),
            Some(Duration::from_secs_f64(12345.67))
        );
    }

    #[test]
    fn meminfo_parsing_with_and_without_memavailable() {
        let s = "MemTotal:       97979612 kB\nMemFree:        21878836 kB\nMemAvailable:   84117104 kB\nSwapTotal:       8388604 kB\nSwapFree:        8388604 kB\n";
        let (m, sw) = parse_meminfo(s).unwrap();
        assert_eq!(m.total, 97979612 * 1024);
        assert_eq!(m.available, 84117104 * 1024);
        assert_eq!(m.used, (97979612 - 84117104) * 1024);
        assert_eq!(
            sw,
            Some(Swap {
                total: 8388604 * 1024,
                used: 0
            })
        );
        let old =
            "MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 50 kB\nCached: 250 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n";
        let (m, sw) = parse_meminfo(old).unwrap();
        assert_eq!(m.available, 400 * 1024);
        assert_eq!(sw, None);
    }

    #[test]
    fn net_dev_skips_loopback() {
        let s = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 1000 10 0 0 0 0 0 0 1000 10 0 0 0 0 0 0\n  eth0: 123456 100 0 0 0 0 0 0 654321 50 0 0 0 0 0 0\n";
        let v = parse_net_dev(s);
        assert_eq!(
            v,
            vec![Interface {
                name: "eth0".into(),
                rx_bytes: 123456,
                tx_bytes: 654321
            }]
        );
    }

    #[test]
    fn pid_stat_handles_hostile_names() {
        let s = "1234 (my (weird) proc ñ日本) S 1 1234 1234 0 -1 4194560 100 0 0 0 150 50 0 0 20 0 1 0 100 1000000 256 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 3 0 0 0 0 0";
        let p = parse_pid_stat(s, 100.0, 4096).unwrap();
        assert_eq!(p.pid, 1234);
        assert_eq!(p.name, "my (weird) proc ñ日本");
        assert_eq!(p.cpu_time, Duration::from_secs(2));
        assert_eq!(p.rss, 256 * 4096);
        assert!(parse_pid_stat("1234 (truncated", 100.0, 4096).is_none());
    }

    #[test]
    fn live_root_statvfs_works() {
        let d = statvfs_disk("/").unwrap();
        assert!(d.total > 0);
        assert!(d.used <= d.total);
    }
}
