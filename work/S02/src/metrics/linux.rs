//! Linux collector: /proc and /sys virtual files plus a few syscalls.
//! Parsers are pure functions over file contents so they can be tested with
//! captured fixtures.

use super::common;
use super::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

pub struct LinuxCollector {
    buf: String,
    clk_tck: u64,
    page_size: u64,
    /// interface name -> has a backing device (physical NIC / Wi-Fi).
    physical: HashMap<String, bool>,
}

impl Default for LinuxCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxCollector {
    pub fn new() -> LinuxCollector {
        // SAFETY: sysconf has no preconditions.
        let clk = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        LinuxCollector {
            buf: String::with_capacity(16 * 1024),
            clk_tck: if clk > 0 { clk as u64 } else { 100 },
            page_size: if page > 0 { page as u64 } else { 4096 },
            physical: HashMap::new(),
        }
    }

    /// Read a small virtual file into the reusable buffer.
    fn read(&mut self, path: &str) -> std::io::Result<&str> {
        self.buf.clear();
        File::open(path)?.read_to_string(&mut self.buf)?;
        Ok(&self.buf)
    }

    fn cpu(&mut self) -> (Reading<CpuTicks>, Reading<Vec<CpuTicks>>) {
        match self.read("/proc/stat") {
            Ok(s) => {
                let (total, cores) = parse_proc_stat(s);
                (total.ok_or(Missing::Failed), if cores.is_empty() { Err(Missing::Failed) } else { Ok(cores) })
            }
            Err(e) => {
                let m: Missing = e.into();
                (Err(m), Err(m))
            }
        }
    }

    fn net(&mut self) -> Reading<Vec<IfaceCounters>> {
        let all = parse_net_dev(self.read("/proc/net/dev")?);
        // Summing every interface double-counts bridged/virtual traffic
        // (docker0 + veth*, etc.). Prefer interfaces backed by a device.
        self.physical.retain(|name, _| all.iter().any(|i| &i.name == name));
        for i in &all {
            self.physical.entry(i.name.clone()).or_insert_with(|| Path::new(&format!("/sys/class/net/{}/device", i.name)).exists());
        }
        let phys: Vec<IfaceCounters> = all.iter().filter(|i| self.physical.get(&i.name) == Some(&true)).cloned().collect();
        Ok(if phys.is_empty() { all } else { phys })
    }

    fn procs(&mut self) -> Reading<ProcTable> {
        let dir = std::fs::read_dir("/proc")?;
        let mut table = ProcTable::default();
        let mut path = String::with_capacity(32);
        for entry in dir.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
            table.total += 1;
            path.clear();
            path.push_str("/proc/");
            path.push_str(name.to_str().unwrap_or_default());
            path.push_str("/stat");
            let parsed = match File::open(&path) {
                Ok(mut f) => {
                    self.buf.clear();
                    match f.read_to_string(&mut self.buf) {
                        Ok(_) => parse_pid_stat(&self.buf),
                        Err(_) => None,
                    }
                }
                // Vanished between readdir and open, or hidden by hidepid.
                Err(_) => None,
            };
            match parsed {
                Some(p) => table.procs.push(ProcRaw {
                    pid,
                    start: p.start_ticks,
                    name: p.comm,
                    cpu_ns: (p.utime + p.stime).saturating_mul(1_000_000_000) / self.clk_tck,
                    rss_bytes: p.rss_pages.max(0) as u64 * self.page_size,
                }),
                None => table.unreadable += 1,
            }
        }
        Ok(table)
    }
}

impl Collector for LinuxCollector {
    fn system_info(&mut self) -> SystemInfo {
        let (sys, rel, machine) = common::uname();
        let os = self
            .read("/etc/os-release")
            .ok()
            .and_then(parse_os_release)
            .or_else(|| std::fs::read_to_string("/usr/lib/os-release").ok().as_deref().and_then(parse_os_release))
            .unwrap_or_else(|| "Linux".to_string());
        SystemInfo { os, kernel: format!("{sys} {rel}"), arch: machine }
    }

    fn sample(&mut self, want_procs: bool) -> RawSample {
        let (cpu, cores) = self.cpu();
        let mem = self.read("/proc/meminfo").map_err(Missing::from).and_then(parse_meminfo);
        RawSample {
            hostname: common::hostname(),
            uptime: boottime(),
            load: common::load(),
            cpu,
            cores,
            memory: mem.map(|m| m.0),
            swap: mem.map(|m| m.1),
            disk: common::disk("/", "/"),
            net: self.net(),
            procs: want_procs.then(|| self.procs()),
        }
    }

    fn full_name(&mut self, pid: u32) -> Option<String> {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        let comm = comm.trim_end_matches('\n');
        let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let argv0 = raw.split(|&b| b == 0).next()?;
        let argv0 = String::from_utf8_lossy(argv0);
        let base = argv0.rsplit('/').next()?.split(' ').next()?;
        // Only trust argv[0] when it extends the (15-byte truncated) comm;
        // many daemons rewrite argv[0] into status strings.
        (base.len() > comm.len() && base.starts_with(comm)).then(|| base.to_string())
    }
}

fn boottime() -> Reading<Duration> {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: valid out-pointer. CLOCK_BOOTTIME includes time spent suspended.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) } != 0 {
        return Err(Missing::Failed);
    }
    Ok(Duration::new(ts.tv_sec.max(0) as u64, ts.tv_nsec.clamp(0, 999_999_999) as u32))
}

/// Aggregate and per-core ticks from /proc/stat.
pub fn parse_proc_stat(s: &str) -> (Option<CpuTicks>, Vec<CpuTicks>) {
    let mut total = None;
    let mut cores = Vec::new();
    for line in s.lines() {
        let Some(rest) = line.strip_prefix("cpu") else {
            if !cores.is_empty() {
                break; // cpu lines are contiguous at the top
            }
            continue;
        };
        let mut it = rest.split_ascii_whitespace();
        let label = if rest.starts_with(' ') { "" } else { it.next().unwrap_or("") };
        let v: Vec<u64> = it.map(|x| x.parse().unwrap_or(0)).collect();
        if v.len() < 4 {
            continue;
        }
        let g = |i: usize| v.get(i).copied().unwrap_or(0);
        // user nice system idle iowait irq softirq steal (guest time is
        // already folded into user/nice).
        let ticks = CpuTicks { busy: g(0) + g(1) + g(2) + g(5) + g(6) + g(7), idle: g(3) + g(4) };
        if label.is_empty() {
            total = Some(ticks);
        } else {
            cores.push(ticks);
        }
    }
    (total, cores)
}

pub fn parse_meminfo(s: &str) -> Reading<(Memory, Swap)> {
    let mut f: HashMap<&str, u64> = HashMap::with_capacity(8);
    for line in s.lines() {
        let mut it = line.split_ascii_whitespace();
        let (Some(k), Some(v)) = (it.next(), it.next()) else { continue };
        let k = k.trim_end_matches(':');
        if matches!(k, "MemTotal" | "MemAvailable" | "MemFree" | "Buffers" | "Cached" | "SReclaimable" | "SwapTotal" | "SwapFree") {
            f.insert(k, v.parse::<u64>().unwrap_or(0) * 1024);
        }
    }
    let total = *f.get("MemTotal").ok_or(Missing::Failed)?;
    // MemAvailable exists since Linux 3.14; approximate it on older kernels.
    let available =
        f.get("MemAvailable").copied().unwrap_or_else(|| ["MemFree", "Buffers", "Cached", "SReclaimable"].iter().filter_map(|k| f.get(k)).sum());
    let swap_total = f.get("SwapTotal").copied().unwrap_or(0);
    let swap_free = f.get("SwapFree").copied().unwrap_or(0);
    Ok((Memory { total, available: available.min(total) }, Swap { total: swap_total, used: swap_total.saturating_sub(swap_free) }))
}

/// Non-loopback interfaces from /proc/net/dev.
pub fn parse_net_dev(s: &str) -> Vec<IfaceCounters> {
    s.lines()
        .skip(2)
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            let name = name.trim();
            if name == "lo" {
                return None;
            }
            let v: Vec<u64> = rest.split_ascii_whitespace().map(|x| x.parse().unwrap_or(0)).collect();
            if v.len() < 9 {
                return None;
            }
            Some(IfaceCounters { name: name.to_string(), rx_bytes: v[0], tx_bytes: v[8], modulus: None })
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub struct PidStat {
    pub comm: String,
    pub utime: u64,
    pub stime: u64,
    pub start_ticks: u64,
    pub rss_pages: i64,
}

/// Parse /proc/<pid>/stat. The command name may contain spaces and
/// parentheses, so it is delimited by the first '(' and the *last* ')'.
pub fn parse_pid_stat(s: &str) -> Option<PidStat> {
    let open = s.find('(')?;
    let close = s.rfind(')')?;
    if close < open {
        return None;
    }
    let comm = s[open + 1..close].to_string();
    let fields: Vec<&str> = s[close + 1..].split_ascii_whitespace().collect();
    // fields[0] is field 3 (state); field N is fields[N - 3].
    let f = |n: usize| fields.get(n - 3).and_then(|x| x.parse::<i64>().ok());
    Some(PidStat { comm, utime: f(14)?.max(0) as u64, stime: f(15)?.max(0) as u64, start_ticks: f(22)?.max(0) as u64, rss_pages: f(24)? })
}

pub fn parse_os_release(s: &str) -> Option<String> {
    let get = |key: &str| {
        s.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).map(|v| v.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty())
    };
    get("PRETTY_NAME").or_else(|| get("NAME"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_stat_parsing() {
        let s = "cpu  100 5 50 800 20 3 2 1 0 0\ncpu0 50 2 25 400 10 1 1 0 0 0\ncpu1 50 3 25 400 10 2 1 1 0 0\nintr 1 2 3\n";
        let (t, c) = parse_proc_stat(s);
        assert_eq!(t, Some(CpuTicks { busy: 100 + 5 + 50 + 3 + 2 + 1, idle: 820 }));
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].busy, 50 + 3 + 25 + 2 + 1 + 1);
    }

    #[test]
    fn meminfo_parsing_with_and_without_memavailable() {
        let s = "MemTotal:       16000000 kB\nMemFree:  1000 kB\nMemAvailable:   8000000 kB\nSwapTotal: 2000 kB\nSwapFree: 500 kB\n";
        let (m, sw) = parse_meminfo(s).unwrap();
        assert_eq!(m.total, 16_000_000 * 1024);
        assert_eq!(m.available, 8_000_000 * 1024);
        assert_eq!(sw.used, 1500 * 1024);
        let old = "MemTotal: 100 kB\nMemFree: 10 kB\nBuffers: 5 kB\nCached: 20 kB\n";
        assert_eq!(parse_meminfo(old).unwrap().0.available, 35 * 1024);
        assert_eq!(parse_meminfo("garbage"), Err(Missing::Failed));
    }

    #[test]
    fn net_dev_parsing_skips_loopback() {
        let s = "Inter-|   Receive\n face |bytes packets\n    lo: 999 1 0 0 0 0 0 0 999 1 0 0 0 0 0 0\n  eth0: 1234 10 0 0 0 0 0 0 5678 20 0 0 0 0 0 0\nwlp2s0:1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16\n";
        let v = parse_net_dev(s);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], IfaceCounters { name: "eth0".into(), rx_bytes: 1234, tx_bytes: 5678, modulus: None });
        assert_eq!(v[1].rx_bytes, 1);
        assert_eq!(v[1].tx_bytes, 9);
    }

    #[test]
    fn pid_stat_with_hostile_command_names() {
        let s = "4242 (evil) name (x)) S 1 1 1 0 -1 4194304 1 0 0 0 70 30 0 0 20 0 1 0 12345 1000 256 18446744073709551615";
        let p = parse_pid_stat(s).unwrap();
        assert_eq!(p.comm, "evil) name (x)");
        assert_eq!((p.utime, p.stime, p.start_ticks, p.rss_pages), (70, 30, 12345, 256));
        let u = "7 (Ünïcødé 名前) R 1 1 1 0 -1 0 0 0 0 0 1 2 0 0 20 0 1 0 99 0 3";
        assert_eq!(parse_pid_stat(u).unwrap().comm, "Ünïcødé 名前");
        assert_eq!(parse_pid_stat("12 (truncated"), None);
    }

    #[test]
    fn os_release_parsing() {
        assert_eq!(parse_os_release("NAME=\"Foo\"\nPRETTY_NAME=\"Foo Linux 1.0\"\n").as_deref(), Some("Foo Linux 1.0"));
        assert_eq!(parse_os_release("NAME=Bar\n").as_deref(), Some("Bar"));
        assert_eq!(parse_os_release(""), None);
    }

    #[test]
    fn live_sample_does_not_panic() {
        let mut c = LinuxCollector::new();
        let s = c.sample(true);
        assert!(s.cpu.is_ok());
        let t = s.procs.unwrap().unwrap();
        assert!(t.total > 0);
        assert!(!c.system_info().arch.is_empty());
    }
}
