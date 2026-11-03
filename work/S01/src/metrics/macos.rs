//! macOS collector built on Mach host statistics, `sysctl`, libproc and the
//! routing socket. No `/proc` here, so every metric is an explicit API call.

use super::{
    hostname, uname_release, Collector, CpuTicks, Interface, Memory, ProcessSample, Snapshot, StaticInfo, Swap,
};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::mem;
use std::ptr;
use std::time::{Duration, Instant, SystemTime};

const PROC_ALL_PIDS: u32 = 1;

// libc marks its Mach port helpers deprecated in favour of the `mach2`
// crate; these two symbols from libSystem are all we need, so bind them
// directly rather than add a dependency.
extern "C" {
    fn mach_host_self() -> libc::mach_port_t;
    static mach_task_self_: libc::mach_port_t;
}

fn host_port() -> libc::mach_port_t {
    unsafe { mach_host_self() }
}

fn task_port() -> libc::mach_port_t {
    unsafe { mach_task_self_ }
}

pub struct MacCollector {
    info: StaticInfo,
    page_size: u64,
    warnings: BTreeMap<&'static str, String>,
}

impl MacCollector {
    pub fn new() -> MacCollector {
        let page_size = sysctl_u64("vm.pagesize").unwrap_or(4096);
        let cores = sysctl_u64("hw.ncpu").unwrap_or(0) as usize;
        let product = sysctl_string("kern.osproductversion").unwrap_or_default();
        MacCollector {
            info: StaticInfo {
                hostname: hostname(),
                os: if product.is_empty() {
                    "macOS".into()
                } else {
                    format!("macOS {product}")
                },
                kernel: uname_release(),
                arch: std::env::consts::ARCH.to_string(),
                cores,
            },
            page_size,
            warnings: BTreeMap::new(),
        }
    }

    fn note<T>(&mut self, key: &'static str, r: Result<T, String>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.warnings.insert(key, e);
                None
            }
        }
    }
}

impl Default for MacCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for MacCollector {
    fn static_info(&self) -> StaticInfo {
        self.info.clone()
    }

    fn sample(&mut self, want_processes: bool) -> Snapshot {
        let mut snap = Snapshot::empty(Instant::now());
        snap.cpu_total = self.note("cpu", cpu_total());
        snap.cpu_cores = self.note("cores", cpu_cores()).unwrap_or_default();
        snap.load = self.note("load", loadavg());
        let page = self.page_size;
        snap.memory = self.note("memory", memory(page));
        snap.swap = self.note("swap", swap()).flatten();
        snap.disk = self.note("disk", statvfs_disk("/").map_err(|e| format!("statvfs(/): {e}")));
        snap.interfaces = self.note("network", interfaces());
        snap.uptime = self.note("uptime", uptime());
        if want_processes {
            let (procs, count) = scan_processes();
            snap.process_count = Some(count);
            snap.processes = Some(procs);
        }
        snap
    }

    fn warnings(&self) -> Vec<String> {
        self.warnings.values().cloned().collect()
    }
}

fn sysctl_u64(name: &str) -> Option<u64> {
    let c = CString::new(name).ok()?;
    let mut v: u64 = 0;
    let mut len = mem::size_of::<u64>();
    let rc = unsafe {
        libc::sysctlbyname(
            c.as_ptr(),
            &mut v as *mut u64 as *mut libc::c_void,
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    // Some keys are 32-bit; the high half is left zeroed by the kernel.
    if len == 4 {
        return Some(v & 0xFFFF_FFFF);
    }
    Some(v)
}

fn sysctl_string(name: &str) -> Option<String> {
    let c = CString::new(name).ok()?;
    let mut len = 0usize;
    if unsafe { libc::sysctlbyname(c.as_ptr(), ptr::null_mut(), &mut len, ptr::null_mut(), 0) } != 0 || len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len];
    if unsafe {
        libc::sysctlbyname(
            c.as_ptr(),
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut len,
            ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..end]).into_owned())
}

fn ticks_from(t: &[u32]) -> CpuTicks {
    let g = |i: i32| t.get(i.max(0) as usize).copied().unwrap_or(0) as u64;
    CpuTicks {
        busy: g(libc::CPU_STATE_USER) + g(libc::CPU_STATE_SYSTEM) + g(libc::CPU_STATE_NICE),
        idle: g(libc::CPU_STATE_IDLE),
    }
}

fn cpu_total() -> Result<CpuTicks, String> {
    let mut info: libc::host_cpu_load_info = unsafe { mem::zeroed() };
    let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
    let rc = unsafe {
        libc::host_statistics64(
            host_port(),
            libc::HOST_CPU_LOAD_INFO,
            &mut info as *mut _ as libc::host_info64_t,
            &mut count,
        )
    };
    if rc != libc::KERN_SUCCESS {
        return Err(format!(
            "host_statistics64(HOST_CPU_LOAD_INFO) failed: kern_return {rc}"
        ));
    }
    Ok(ticks_from(&info.cpu_ticks))
}

fn cpu_cores() -> Result<Vec<CpuTicks>, String> {
    let mut ncpu: libc::natural_t = 0;
    let mut info: libc::processor_info_array_t = ptr::null_mut();
    let mut count: libc::mach_msg_type_number_t = 0;
    let rc = unsafe {
        libc::host_processor_info(
            host_port(),
            libc::PROCESSOR_CPU_LOAD_INFO,
            &mut ncpu,
            &mut info,
            &mut count,
        )
    };
    if rc != libc::KERN_SUCCESS || info.is_null() {
        return Err(format!("host_processor_info failed: kern_return {rc}"));
    }
    let per = libc::CPU_STATE_MAX as usize;
    let slice = unsafe { std::slice::from_raw_parts(info as *const u32, count as usize) };
    let cores = slice.chunks_exact(per).take(ncpu as usize).map(ticks_from).collect();
    // The array is vm_allocated by the kernel and must be handed back.
    unsafe {
        libc::vm_deallocate(
            task_port(),
            info as libc::vm_address_t,
            count as libc::vm_size_t * mem::size_of::<libc::integer_t>() as libc::vm_size_t,
        );
    }
    Ok(cores)
}

fn loadavg() -> Result<[f64; 3], String> {
    let mut v = [0f64; 3];
    let n = unsafe { libc::getloadavg(v.as_mut_ptr(), 3) };
    if n != 3 {
        return Err(format!("getloadavg returned {n}"));
    }
    Ok(v)
}

fn memory(page: u64) -> Result<Memory, String> {
    let total = sysctl_u64("hw.memsize").ok_or("sysctl hw.memsize failed")?;
    let mut vm: libc::vm_statistics64 = unsafe { mem::zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let rc = unsafe {
        libc::host_statistics64(
            host_port(),
            libc::HOST_VM_INFO64,
            &mut vm as *mut _ as libc::host_info64_t,
            &mut count,
        )
    };
    if rc != libc::KERN_SUCCESS {
        return Err(format!("host_statistics64(HOST_VM_INFO64) failed: kern_return {rc}"));
    }
    // "Used" follows Activity Monitor: app (active - purgeable) + wired + compressed.
    let active = (vm.active_count as u64).saturating_sub(vm.purgeable_count as u64);
    let used = (active + vm.wire_count as u64 + vm.compressor_page_count as u64) * page;
    let used = used.min(total);
    Ok(Memory {
        total,
        used,
        available: total - used,
    })
}

fn swap() -> Result<Option<Swap>, String> {
    let c = CString::new("vm.swapusage").unwrap();
    let mut xs: libc::xsw_usage = unsafe { mem::zeroed() };
    let mut len = mem::size_of::<libc::xsw_usage>();
    let rc = unsafe {
        libc::sysctlbyname(
            c.as_ptr(),
            &mut xs as *mut _ as *mut libc::c_void,
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(format!("sysctl vm.swapusage: {}", std::io::Error::last_os_error()));
    }
    if xs.xsu_total == 0 {
        return Ok(None);
    }
    Ok(Some(Swap {
        total: xs.xsu_total,
        used: xs.xsu_used,
    }))
}

fn uptime() -> Result<Duration, String> {
    let mut mib = [libc::CTL_KERN, libc::KERN_BOOTTIME];
    let mut tv: libc::timeval = unsafe { mem::zeroed() };
    let mut len = mem::size_of::<libc::timeval>();
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            &mut tv as *mut _ as *mut libc::c_void,
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(format!("sysctl kern.boottime: {}", std::io::Error::last_os_error()));
    }
    let boot = Duration::new(tv.tv_sec as u64, (tv.tv_usec as u32) * 1000);
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    Ok(now.saturating_sub(boot))
}

/// Interface counters via `NET_RT_IFLIST2`, which carries 64-bit byte counts
/// (unlike `getifaddrs`, whose `if_data` counters are 32-bit).
fn interfaces() -> Result<Vec<Interface>, String> {
    let mut mib = [libc::CTL_NET, libc::PF_ROUTE, 0, 0, libc::NET_RT_IFLIST2, 0];
    let mut len = 0usize;
    if unsafe { libc::sysctl(mib.as_mut_ptr(), 6, ptr::null_mut(), &mut len, ptr::null_mut(), 0) } != 0 {
        return Err(format!(
            "sysctl NET_RT_IFLIST2 size: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut buf = vec![0u8; len];
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            6,
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut len,
            ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(format!("sysctl NET_RT_IFLIST2: {}", std::io::Error::last_os_error()));
    }
    let mut out = Vec::new();
    let mut off = 0usize;
    let hdr = mem::size_of::<libc::if_msghdr>();
    while off + hdr <= len {
        let m = unsafe { ptr::read_unaligned(buf.as_ptr().add(off) as *const libc::if_msghdr) };
        let msglen = m.ifm_msglen as usize;
        if msglen == 0 {
            break;
        }
        if m.ifm_type as i32 == libc::RTM_IFINFO2 && off + mem::size_of::<libc::if_msghdr2>() <= len {
            let m2 = unsafe { ptr::read_unaligned(buf.as_ptr().add(off) as *const libc::if_msghdr2) };
            let up = m2.ifm_flags & libc::IFF_UP != 0;
            let loopback = m2.ifm_flags & libc::IFF_LOOPBACK != 0;
            if up && !loopback {
                out.push(Interface {
                    name: format!("if{}", m2.ifm_index),
                    rx_bytes: m2.ifm_data.ifi_ibytes,
                    tx_bytes: m2.ifm_data.ifi_obytes,
                });
            }
        }
        off += msglen;
    }
    Ok(out)
}

fn scan_processes() -> (Vec<ProcessSample>, usize) {
    let n = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, ptr::null_mut(), 0) };
    if n <= 0 {
        return (Vec::new(), 0);
    }
    // Leave headroom: processes may spawn between the size query and the list.
    let mut pids = vec![0 as libc::pid_t; (n as usize / mem::size_of::<libc::pid_t>()) + 64];
    let bytes = unsafe {
        libc::proc_listpids(
            PROC_ALL_PIDS,
            0,
            pids.as_mut_ptr() as *mut libc::c_void,
            (pids.len() * mem::size_of::<libc::pid_t>()) as libc::c_int,
        )
    };
    if bytes <= 0 {
        return (Vec::new(), 0);
    }
    pids.truncate(bytes as usize / mem::size_of::<libc::pid_t>());
    pids.retain(|&p| p > 0);
    let count = pids.len();
    let mut procs = Vec::with_capacity(count);
    let ti_size = mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    for &pid in &pids {
        let mut ti: libc::proc_taskinfo = unsafe { mem::zeroed() };
        // Fails with EPERM for other users' processes and ESRCH for processes
        // that exited mid-scan; both are simply skipped.
        let got = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTASKINFO,
                0,
                &mut ti as *mut _ as *mut libc::c_void,
                ti_size,
            )
        };
        if got != ti_size {
            continue;
        }
        let mut name = [0u8; 256];
        let nl = unsafe { libc::proc_name(pid, name.as_mut_ptr() as *mut libc::c_void, name.len() as u32) };
        let name = if nl > 0 {
            String::from_utf8_lossy(&name[..nl as usize]).into_owned()
        } else {
            format!("pid {pid}")
        };
        procs.push(ProcessSample {
            pid,
            name,
            cpu_time: Duration::from_nanos(ti.pti_total_user.saturating_add(ti.pti_total_system)),
            rss: ti.pti_resident_size,
        });
    }
    (procs, count)
}

pub fn statvfs_disk(mount: &str) -> Result<super::Disk, std::io::Error> {
    let path = CString::new(mount).map_err(|_| std::io::Error::other("bad path"))?;
    let mut st: libc::statvfs = unsafe { mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut st) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let frsize = st.f_frsize as u64;
    let total = st.f_blocks as u64 * frsize;
    let free = st.f_bfree as u64 * frsize;
    let available = st.f_bavail as u64 * frsize;
    Ok(super::Disk {
        mount: mount.to_string(),
        total,
        used: total.saturating_sub(free),
        available,
    })
}
