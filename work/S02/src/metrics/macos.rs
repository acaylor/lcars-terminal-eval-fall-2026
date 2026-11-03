//! macOS collector. There is no /proc on macOS, so this uses Mach host
//! statistics, sysctl, the routing socket interface list, and libproc.

// libc marks its Mach bindings deprecated in favour of the `mach2` crate;
// the underlying system calls are stable ABI, so avoid the extra dependency.
#![allow(deprecated)]

use super::common;
use super::*;
use std::ffi::CStr;
use std::mem::{size_of, zeroed};
use std::ptr::{null_mut, read_unaligned};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct MacCollector {
    /// Mach absolute-time to nanoseconds ratio. libproc reports task CPU
    /// time in Mach ticks on Apple Silicon (1 tick = 125/3 ns there).
    timebase: (u64, u64),
    page_size: u64,
    pids: Vec<libc::c_int>,
    data_volume: bool,
}

impl Default for MacCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a fixed-size value via sysctlbyname.
fn sysctl_value<T: Copy>(name: &str) -> Reading<T> {
    let cname = std::ffi::CString::new(name).map_err(|_| Missing::Failed)?;
    // SAFETY: T is plain-old-data; the kernel writes at most `len` bytes.
    let mut v: T = unsafe { zeroed() };
    let mut len = size_of::<T>();
    let rc = unsafe { libc::sysctlbyname(cname.as_ptr(), &mut v as *mut T as *mut libc::c_void, &mut len, null_mut(), 0) };
    if rc != 0 || len != size_of::<T>() {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(v)
}

fn sysctl_string(name: &str) -> Option<String> {
    let cname = std::ffi::CString::new(name).ok()?;
    let mut buf = [0u8; 256];
    let mut len = buf.len();
    // SAFETY: buffer and length are valid.
    let rc = unsafe { libc::sysctlbyname(cname.as_ptr(), buf.as_mut_ptr() as *mut libc::c_void, &mut len, null_mut(), 0) };
    if rc != 0 {
        return None;
    }
    let end = buf[..len].iter().position(|&b| b == 0).unwrap_or(len);
    Some(String::from_utf8_lossy(&buf[..end]).into_owned())
}

fn cstr_field(a: &[libc::c_char]) -> String {
    // SAFETY: the kernel NUL-terminates these fixed arrays; guard anyway.
    if !a.contains(&0) {
        return String::new();
    }
    unsafe { CStr::from_ptr(a.as_ptr()) }.to_string_lossy().into_owned()
}

impl MacCollector {
    pub fn new() -> MacCollector {
        let mut tb = libc::mach_timebase_info { numer: 0, denom: 0 };
        // SAFETY: valid out-pointer.
        unsafe { libc::mach_timebase_info(&mut tb) };
        let timebase = if tb.numer == 0 || tb.denom == 0 { (1, 1) } else { (tb.numer as u64, tb.denom as u64) };
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        MacCollector {
            timebase,
            page_size: if page > 0 { page as u64 } else { 4096 },
            pids: Vec::new(),
            // On APFS the sealed system volume is mounted at "/"; user data
            // (and what users think of as disk usage) lives on the Data volume.
            data_volume: std::path::Path::new("/System/Volumes/Data").exists(),
        }
    }

    fn cpu(&self) -> (Reading<CpuTicks>, Reading<Vec<CpuTicks>>) {
        let mut ncpu: libc::natural_t = 0;
        let mut info: libc::processor_info_array_t = null_mut();
        let mut count: libc::mach_msg_type_number_t = 0;
        // SAFETY: out-pointers are valid; on success the kernel hands us a
        // vm allocation of `count` integers that we must deallocate.
        let kr = unsafe { libc::host_processor_info(libc::mach_host_self(), libc::PROCESSOR_CPU_LOAD_INFO, &mut ncpu, &mut info, &mut count) };
        if kr != libc::KERN_SUCCESS || info.is_null() {
            return (Err(Missing::Failed), Err(Missing::Failed));
        }
        let states = libc::CPU_STATE_MAX as usize;
        let mut cores = Vec::with_capacity(ncpu as usize);
        for i in 0..ncpu as usize {
            if (i + 1) * states > count as usize {
                break;
            }
            // Ticks are 32-bit counters exposed as signed ints.
            let t = |s: libc::c_int| unsafe { *info.add(i * states + s as usize) } as u32 as u64;
            cores.push(CpuTicks {
                busy: t(libc::CPU_STATE_USER) + t(libc::CPU_STATE_SYSTEM) + t(libc::CPU_STATE_NICE),
                idle: t(libc::CPU_STATE_IDLE),
            });
        }
        unsafe {
            libc::vm_deallocate(
                libc::mach_task_self_,
                info as libc::vm_address_t,
                (count as usize * size_of::<libc::integer_t>()) as libc::vm_size_t,
            );
        }
        if cores.is_empty() {
            return (Err(Missing::Failed), Err(Missing::Failed));
        }
        let total = cores.iter().fold(CpuTicks::default(), |a, c| CpuTicks { busy: a.busy + c.busy, idle: a.idle + c.idle });
        (Ok(total), Ok(cores))
    }

    fn memory(&self) -> Reading<Memory> {
        let total: u64 = sysctl_value("hw.memsize")?;
        // SAFETY: zeroed stats struct and matching count are valid in/out args.
        let mut vm: libc::vm_statistics64 = unsafe { zeroed() };
        let mut count = libc::HOST_VM_INFO64_COUNT;
        let kr =
            unsafe { libc::host_statistics64(libc::mach_host_self(), libc::HOST_VM_INFO64, &mut vm as *mut _ as libc::host_info64_t, &mut count) };
        if kr != libc::KERN_SUCCESS {
            return Err(Missing::Failed);
        }
        // Mirrors Activity Monitor's "Memory Used": app memory (anonymous
        // minus purgeable) + wired + compressed.
        let p = self.page_size;
        let internal = vm.internal_page_count as u64;
        let purgeable = vm.purgeable_count as u64;
        let used = (internal.saturating_sub(purgeable) + vm.wire_count as u64 + vm.compressor_page_count as u64) * p;
        Ok(Memory { total, available: total.saturating_sub(used) })
    }

    fn net(&self) -> Reading<Vec<IfaceCounters>> {
        let mut mib = [libc::CTL_NET, libc::PF_ROUTE, 0, 0, libc::NET_RT_IFLIST2, 0];
        let mut len: libc::size_t = 0;
        // SAFETY: size query with a null buffer.
        if unsafe { libc::sysctl(mib.as_mut_ptr(), 6, null_mut(), &mut len, null_mut(), 0) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // Interfaces can appear between the two calls; leave headroom.
        let mut buf = vec![0u8; len + 2048];
        len = buf.len();
        if unsafe { libc::sysctl(mib.as_mut_ptr(), 6, buf.as_mut_ptr() as *mut libc::c_void, &mut len, null_mut(), 0) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut out = Vec::new();
        let mut off = 0usize;
        while off + 4 <= len {
            let msglen = u16::from_ne_bytes([buf[off], buf[off + 1]]) as usize;
            let mtype = buf[off + 3] as libc::c_int;
            if msglen == 0 {
                break;
            }
            if mtype == libc::RTM_IFINFO2 && off + size_of::<libc::if_msghdr2>() <= len {
                // SAFETY: bounds checked above; the struct is packed so read unaligned.
                let m: libc::if_msghdr2 = unsafe { read_unaligned(buf.as_ptr().add(off) as *const libc::if_msghdr2) };
                let flags = m.ifm_flags;
                if flags & libc::IFF_LOOPBACK == 0 && flags & libc::IFF_UP != 0 {
                    let data = m.ifm_data;
                    let mut name = [0 as libc::c_char; libc::IF_NAMESIZE];
                    let index = m.ifm_index;
                    let p = unsafe { libc::if_indextoname(index as libc::c_uint, name.as_mut_ptr()) };
                    let name = if p.is_null() { format!("if{index}") } else { cstr_field(&name) };
                    out.push(IfaceCounters { name, rx_bytes: data.ifi_ibytes, tx_bytes: data.ifi_obytes, modulus: None });
                }
            }
            off += msglen;
        }
        Ok(out)
    }

    fn procs(&mut self) -> Reading<ProcTable> {
        // SAFETY: a null buffer asks libproc for the current count.
        let n = unsafe { libc::proc_listallpids(null_mut(), 0) };
        if n <= 0 {
            return Err(Missing::Failed);
        }
        self.pids.resize(n as usize + 64, 0);
        let bytes = (self.pids.len() * size_of::<libc::c_int>()) as libc::c_int;
        let got = unsafe { libc::proc_listallpids(self.pids.as_mut_ptr() as *mut libc::c_void, bytes) };
        if got <= 0 {
            return Err(Missing::Failed);
        }
        let mut table = ProcTable::default();
        let (numer, denom) = self.timebase;
        for &pid in &self.pids[..(got as usize).min(self.pids.len())] {
            if pid <= 0 {
                continue;
            }
            table.total += 1;
            let mut info: libc::proc_taskallinfo = unsafe { zeroed() };
            let size = size_of::<libc::proc_taskallinfo>() as libc::c_int;
            // SAFETY: info is a correctly sized out-buffer.
            let r = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTASKALLINFO, 0, &mut info as *mut _ as *mut libc::c_void, size) };
            if r != size {
                // EPERM for other users' processes without privileges, ESRCH if it exited.
                table.unreadable += 1;
                continue;
            }
            let mut name = cstr_field(&info.pbsd.pbi_name);
            if name.is_empty() {
                name = cstr_field(&info.pbsd.pbi_comm);
            }
            let ticks = info.ptinfo.pti_total_user.saturating_add(info.ptinfo.pti_total_system);
            table.procs.push(ProcRaw {
                pid: pid as u32,
                start: info.pbsd.pbi_start_tvsec.saturating_mul(1_000_000).saturating_add(info.pbsd.pbi_start_tvusec),
                name,
                cpu_ns: (ticks as u128 * numer as u128 / denom as u128) as u64,
                rss_bytes: info.ptinfo.pti_resident_size,
            });
        }
        Ok(table)
    }
}

fn uptime() -> Reading<Duration> {
    let boot: libc::timeval = sysctl_value("kern.boottime")?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| Missing::Failed)?;
    let boot = Duration::new(boot.tv_sec.max(0) as u64, (boot.tv_usec.max(0) as u32).min(999_999) * 1000);
    now.checked_sub(boot).ok_or(Missing::Failed)
}

fn swap() -> Reading<Swap> {
    let x: libc::xsw_usage = sysctl_value("vm.swapusage")?;
    Ok(Swap { total: x.xsu_total, used: x.xsu_used })
}

impl Collector for MacCollector {
    fn system_info(&mut self) -> SystemInfo {
        let (sys, rel, machine) = common::uname();
        let os = sysctl_string("kern.osproductversion").map(|v| format!("macOS {v}")).unwrap_or_else(|| "macOS".into());
        SystemInfo { os, kernel: format!("{sys} {rel}"), arch: machine }
    }

    fn sample(&mut self, want_procs: bool) -> RawSample {
        let (cpu, cores) = self.cpu();
        let disk = if self.data_volume { common::disk("/System/Volumes/Data", "/") } else { common::disk("/", "/") };
        RawSample {
            hostname: common::hostname(),
            uptime: uptime(),
            load: common::load(),
            cpu,
            cores,
            memory: self.memory(),
            swap: swap(),
            disk,
            net: self.net(),
            procs: want_procs.then(|| self.procs()),
        }
    }
}
