//! macOS collectors call Mach and libproc through libc.
//! Those symbols are deprecated in favor of the mach2 crate; libc is kept as the
//! only dependency so the binary stays small and needs no network after install.
#![allow(deprecated)]

use super::rate::{cpu_percent, iface_rates, process_cpu_ns, IfaceSnap};
use super::unix::{self, page_size};
use super::{top_processes, ProcessRow, Sample};
use std::collections::HashMap;
use std::time::Instant;

pub struct MacCollector {
    prev_cpu: Option<(u64, u64)>,
    prev_cores: Vec<(u64, u64)>,
    prev_net: Vec<IfaceSnap>,
    prev_procs: HashMap<(u32, u64), u64>,
    prev_at: Option<Instant>,
    os_name: String,
    arch: String,
    page: u64,
}

impl MacCollector {
    pub fn new() -> Self {
        let (sys, rel, arch) = unix::uname_parts();
        let ver = sysctl_string("kern.osproductversion");
        let os_name = if let Some(v) = ver {
            format!("macOS {v}")
        } else if rel.is_empty() {
            sys
        } else {
            format!("Darwin {rel}")
        };
        Self {
            prev_cpu: None,
            prev_cores: Vec::new(),
            prev_net: Vec::new(),
            prev_procs: HashMap::new(),
            prev_at: None,
            os_name,
            arch,
            page: page_size(),
        }
    }

    pub fn sample(&mut self, processes: bool) -> Sample {
        let now = Instant::now();
        let elapsed = self
            .prev_at
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        let mut sample =
            Sample::unavailable(unix::hostname(), self.os_name.clone(), self.arch.clone());
        sample.time = unix::civil_now();
        sample.uptime = boot_uptime();
        sample.load = loadavg();
        if let Some((used, total)) = unix::disk_root() {
            sample.disk_used = Some(used);
            sample.disk_total = Some(total);
        }
        self.fill_cpu(&mut sample, elapsed);
        self.fill_mem(&mut sample);
        self.fill_swap(&mut sample);
        self.fill_net(&mut sample, elapsed);
        if processes {
            self.fill_procs(&mut sample, elapsed);
        } else {
            self.prev_procs.clear();
        }
        self.prev_at = Some(now);
        sample
    }

    fn fill_cpu(&mut self, sample: &mut Sample, elapsed: f64) {
        let Some((busy, total)) = host_cpu() else {
            return;
        };
        if elapsed > 0.0 {
            if let Some((pb, pt)) = self.prev_cpu {
                sample.cpu_pct = cpu_percent(pb, pt, busy, total);
            }
        }
        self.prev_cpu = Some((busy, total));
        if let Some(cores) = host_cores() {
            let mut pcts = Vec::with_capacity(cores.len());
            for (i, (b, t)) in cores.iter().copied().enumerate() {
                let pct = if elapsed > 0.0 {
                    self.prev_cores
                        .get(i)
                        .and_then(|(pb, pt)| cpu_percent(*pb, *pt, b, t))
                } else {
                    None
                };
                pcts.push(pct);
            }
            sample.per_core = pcts;
            self.prev_cores = cores;
        }
    }

    fn fill_mem(&self, sample: &mut Sample) {
        let Some(total) = sysctl_u64("hw.memsize") else {
            return;
        };
        let Some(vm) = vm_stat() else {
            sample.mem_total = Some(total);
            return;
        };
        let page = self.page;
        let available = vm
            .free
            .saturating_add(vm.inactive)
            .saturating_add(vm.speculative)
            .saturating_mul(page);
        let available = available.min(total);
        sample.mem_total = Some(total);
        sample.mem_available = Some(available);
        sample.mem_used = Some(total.saturating_sub(available));
    }

    fn fill_swap(&self, sample: &mut Sample) {
        if let Some((used, total)) = swap_usage() {
            sample.swap_used = Some(used);
            sample.swap_total = Some(total);
        }
    }

    fn fill_net(&mut self, sample: &mut Sample, elapsed: f64) {
        let Some(now) = if_counters() else { return };
        if elapsed > 0.0 {
            if let Some((rx, tx)) = iface_rates(&self.prev_net, &now, elapsed) {
                sample.net_rx_bps = Some(rx);
                sample.net_tx_bps = Some(tx);
            }
        }
        self.prev_net = now;
    }

    fn fill_procs(&mut self, sample: &mut Sample, elapsed: f64) {
        let Some(pids) = list_pids() else {
            sample.process_count = None;
            return;
        };
        sample.process_count = Some(pids.len());
        let mut rows = Vec::new();
        let mut next_map = HashMap::new();
        for pid in pids {
            if pid <= 0 {
                continue;
            }
            let Some(info) = task_info(pid) else { continue };
            let Some(bsd) = bsd_info(pid) else { continue };
            let ticks = info.user.saturating_add(info.system);
            let key = (pid as u32, bsd.start);
            let cpu = if elapsed > 0.0 {
                self.prev_procs
                    .get(&key)
                    .and_then(|prev| process_cpu_ns(*prev, ticks, elapsed))
            } else {
                None
            };
            next_map.insert(key, ticks);
            rows.push(ProcessRow {
                pid: pid as u32,
                name: if bsd.name.is_empty() {
                    format!("pid-{pid}")
                } else {
                    bsd.name
                },
                cpu_pct: cpu,
                mem_bytes: Some(info.rss),
            });
        }
        sample.top_cpu = top_processes(rows);
        self.prev_procs = next_map;
    }
}

struct VmCounts {
    free: u64,
    inactive: u64,
    speculative: u64,
}

struct TaskTimes {
    user: u64,
    system: u64,
    rss: u64,
}

struct BsdProc {
    name: String,
    start: u64,
}

fn host_cpu() -> Option<(u64, u64)> {
    unsafe {
        let mut ticks = [0u32; 4];
        let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
        let rc = libc::host_statistics64(
            libc::mach_host_self(),
            libc::HOST_CPU_LOAD_INFO,
            ticks.as_mut_ptr().cast(),
            &mut count,
        );
        if rc != 0 {
            return None;
        }
        let user = ticks[libc::CPU_STATE_USER as usize] as u64;
        let system = ticks[libc::CPU_STATE_SYSTEM as usize] as u64;
        let idle = ticks[libc::CPU_STATE_IDLE as usize] as u64;
        let nice = ticks[libc::CPU_STATE_NICE as usize] as u64;
        let busy = user.saturating_add(system).saturating_add(nice);
        Some((busy, busy.saturating_add(idle)))
    }
}

fn host_cores() -> Option<Vec<(u64, u64)>> {
    unsafe {
        let mut ncpu: libc::natural_t = 0;
        let mut info: libc::processor_info_array_t = std::ptr::null_mut();
        let mut info_count: libc::mach_msg_type_number_t = 0;
        let rc = libc::host_processor_info(
            libc::mach_host_self(),
            libc::PROCESSOR_CPU_LOAD_INFO,
            &mut ncpu,
            &mut info,
            &mut info_count,
        );
        if rc != 0 || info.is_null() || ncpu == 0 {
            return None;
        }
        let stride = libc::CPU_STATE_MAX as usize;
        let mut out = Vec::with_capacity(ncpu as usize);
        for i in 0..ncpu as usize {
            let base = i * stride;
            if base + 3 >= info_count as usize {
                break;
            }
            let user = *info.add(base + libc::CPU_STATE_USER as usize) as u64;
            let system = *info.add(base + libc::CPU_STATE_SYSTEM as usize) as u64;
            let idle = *info.add(base + libc::CPU_STATE_IDLE as usize) as u64;
            let nice = *info.add(base + libc::CPU_STATE_NICE as usize) as u64;
            let busy = user.saturating_add(system).saturating_add(nice);
            out.push((busy, busy.saturating_add(idle)));
        }
        let _ = libc::vm_deallocate(
            libc::mach_task_self(),
            info as libc::vm_address_t,
            (info_count as usize * std::mem::size_of::<libc::integer_t>()) as libc::vm_size_t,
        );
        Some(out)
    }
}

fn vm_stat() -> Option<VmCounts> {
    unsafe {
        let mut vm: libc::vm_statistics64 = std::mem::zeroed();
        let mut count = libc::HOST_VM_INFO64_COUNT;
        let rc = libc::host_statistics64(
            libc::mach_host_self(),
            libc::HOST_VM_INFO64,
            (&mut vm as *mut libc::vm_statistics64).cast(),
            &mut count,
        );
        if rc != 0 {
            return None;
        }
        Some(VmCounts {
            free: vm.free_count as u64,
            inactive: vm.inactive_count as u64,
            speculative: vm.speculative_count as u64,
        })
    }
}

fn swap_usage() -> Option<(u64, u64)> {
    let mut raw = [0u8; 256];
    let mut len: libc::size_t = raw.len();
    let key = std::ffi::CString::new("vm.swapusage").ok()?;
    let rc = unsafe {
        libc::sysctlbyname(
            key.as_ptr(),
            raw.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len < 24 {
        return None;
    }
    let total = u64::from_ne_bytes(raw[0..8].try_into().ok()?);
    let used = u64::from_ne_bytes(raw[16..24].try_into().ok()?);
    Some((used, total))
}

fn if_counters() -> Option<Vec<IfaceSnap>> {
    unsafe {
        let mut mib = [libc::CTL_NET, libc::PF_ROUTE, 0, 0, libc::NET_RT_IFLIST2, 0];
        let mut len: libc::size_t = 0;
        if libc::sysctl(
            mib.as_mut_ptr(),
            6,
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        ) != 0
            || len == 0
        {
            return None;
        }
        let mut buf = vec![0u8; len];
        if libc::sysctl(
            mib.as_mut_ptr(),
            6,
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        buf.truncate(len);
        Some(parse_iflist2(&buf))
    }
}

fn parse_iflist2(buf: &[u8]) -> Vec<IfaceSnap> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    let hdr_size = std::mem::size_of::<libc::if_msghdr2>();
    while offset + 4 <= buf.len() {
        let msglen = u16::from_ne_bytes([buf[offset], buf[offset + 1]]) as usize;
        if msglen < 4 || offset + msglen > buf.len() {
            break;
        }
        let typ = buf[offset + 3];
        if typ == libc::RTM_IFINFO2 as u8 && msglen >= hdr_size {
            let hdr = unsafe {
                std::ptr::read_unaligned(buf[offset..].as_ptr().cast::<libc::if_msghdr2>())
            };
            let name_off = offset + hdr_size;
            if name_off < buf.len() {
                let sdl_len = buf[name_off] as usize;
                if sdl_len >= 8 && name_off + sdl_len <= buf.len() {
                    let nlen = buf[name_off + 5] as usize;
                    let start = name_off + 8;
                    let end = (start + nlen).min(buf.len());
                    let name = String::from_utf8_lossy(&buf[start..end]).trim().to_string();
                    if !name.is_empty() && name != "lo0" && !name.starts_with("lo") {
                        out.push(IfaceSnap {
                            name,
                            rx: hdr.ifm_data.ifi_ibytes,
                            tx: hdr.ifm_data.ifi_obytes,
                        });
                    }
                }
            }
        }
        offset += msglen;
    }
    out
}

fn list_pids() -> Option<Vec<i32>> {
    unsafe {
        let bytes = libc::proc_listallpids(std::ptr::null_mut(), 0);
        if bytes <= 0 {
            return None;
        }
        let count = (bytes as usize / std::mem::size_of::<i32>()).saturating_add(64);
        let mut pids = vec![0i32; count];
        let wrote = libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * std::mem::size_of::<i32>()) as i32,
        );
        if wrote <= 0 {
            return None;
        }
        let n = wrote as usize / std::mem::size_of::<i32>();
        pids.truncate(n.min(pids.len()));
        pids.retain(|p| *p > 0);
        Some(pids)
    }
}

fn task_info(pid: i32) -> Option<TaskTimes> {
    unsafe {
        let mut info: libc::proc_taskinfo = std::mem::zeroed();
        let sz = libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTASKINFO,
            0,
            (&mut info as *mut libc::proc_taskinfo).cast(),
            std::mem::size_of::<libc::proc_taskinfo>() as i32,
        );
        if sz <= 0 {
            return None;
        }
        Some(TaskTimes {
            user: info.pti_total_user,
            system: info.pti_total_system,
            rss: info.pti_resident_size,
        })
    }
}

fn bsd_info(pid: i32) -> Option<BsdProc> {
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let sz = libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            std::mem::size_of::<libc::proc_bsdinfo>() as i32,
        );
        if sz <= 0 {
            return None;
        }
        let name = c_chars(&info.pbi_name);
        Some(BsdProc {
            name,
            start: info.pbi_start_tvsec,
        })
    }
}

fn c_chars(bytes: &[libc::c_char]) -> String {
    let slice = unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<u8>(), bytes.len()) };
    let end = slice.iter().position(|b| *b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..end])
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect()
}

fn boot_uptime() -> Option<std::time::Duration> {
    unsafe {
        let mut tv: libc::timeval = std::mem::zeroed();
        let mut len = std::mem::size_of::<libc::timeval>() as libc::size_t;
        let key = std::ffi::CString::new("kern.boottime").ok()?;
        if libc::sysctlbyname(
            key.as_ptr(),
            (&mut tv as *mut libc::timeval).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        let now = libc::time(std::ptr::null_mut());
        if now < tv.tv_sec {
            return None;
        }
        Some(std::time::Duration::from_secs((now - tv.tv_sec) as u64))
    }
}

fn loadavg() -> Option<[f64; 3]> {
    let mut loads = [0f64; 3];
    let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    if n < 3 {
        return None;
    }
    if loads.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    Some(loads)
}

fn sysctl_string(key: &str) -> Option<String> {
    let key = std::ffi::CString::new(key).ok()?;
    let mut buf = [0u8; 128];
    let mut len = buf.len() as libc::size_t;
    let rc = unsafe {
        libc::sysctlbyname(
            key.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len == 0 {
        return None;
    }
    let n = len.min(buf.len());
    let end = buf[..n].iter().position(|b| *b == 0).unwrap_or(n);
    let s = String::from_utf8_lossy(&buf[..end]).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn sysctl_u64(key: &str) -> Option<u64> {
    let key = std::ffi::CString::new(key).ok()?;
    let mut val: u64 = 0;
    let mut len = std::mem::size_of::<u64>() as libc::size_t;
    let rc = unsafe {
        libc::sysctlbyname(
            key.as_ptr(),
            (&mut val as *mut u64).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        None
    } else {
        Some(val)
    }
}
