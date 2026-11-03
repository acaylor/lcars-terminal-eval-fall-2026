use crate::model::CpuTimes;

#[cfg(target_os = "macos")]
use std::collections::BTreeMap;
#[cfg(target_os = "macos")]
use std::ffi::CStr;
#[cfg(target_os = "macos")]
use std::mem::{size_of, MaybeUninit};

#[cfg(target_os = "macos")]
use crate::collector::{
    collect_hostname, collect_loadavg, collect_local_clock, collect_root_disk, collect_uname,
    MetricCollector,
};
#[cfg(target_os = "macos")]
use crate::model::{RawNetBytes, RawProcessSample, RawSystemSample};

/// Converts Mach `PROCESSOR_CPU_LOAD_INFO` integer slice (`CPU_STATE_MAX = 4` ints per core:
/// `[USER, SYSTEM, IDLE, NICE]`) into overall `CpuTimes` and per-core `Vec<CpuTimes>`.
pub fn parse_mach_cpu_load_info(raw_ticks: &[i32]) -> (Option<CpuTimes>, Vec<CpuTimes>) {
    const STATES: usize = 4; // USER=0, SYSTEM=1, IDLE=2, NICE=3
    let num_cores = raw_ticks.len() / STATES;
    if num_cores == 0 {
        return (None, Vec::new());
    }

    let mut cores = Vec::with_capacity(num_cores);
    let mut sum_active = 0u64;
    let mut sum_total = 0u64;

    for c in 0..num_cores {
        let base = c * STATES;
        let user = raw_ticks[base].max(0) as u64;
        let system = raw_ticks[base + 1].max(0) as u64;
        let idle = raw_ticks[base + 2].max(0) as u64;
        let nice = raw_ticks[base + 3].max(0) as u64;

        let active = user.saturating_add(system).saturating_add(nice);
        let total = active.saturating_add(idle);
        sum_active = sum_active.saturating_add(active);
        sum_total = sum_total.saturating_add(total);
        cores.push(CpuTimes { active, total });
    }

    (
        Some(CpuTimes {
            active: sum_active,
            total: sum_total,
        }),
        cores,
    )
}

/// Computes available and used memory bytes on macOS from `HW_MEMSIZE` total bytes,
/// VM page size, and Mach `vm_statistics64` page counters.
pub fn compute_macos_vm_memory(
    total_bytes: u64,
    page_size: u64,
    free_pages: u64,
    inactive_pages: u64,
    purgeable_pages: u64,
) -> (u64, u64) {
    let reclaimable_pages = free_pages
        .saturating_add(inactive_pages)
        .saturating_add(purgeable_pages);
    let avail_bytes = reclaimable_pages.saturating_mul(page_size).min(total_bytes);
    let used_bytes = total_bytes.saturating_sub(avail_bytes);
    (total_bytes, avail_bytes.max(total_bytes.saturating_sub(used_bytes)))
}

/// Safely extracts a UTF-8 string from a fixed-size C char buffer (`pbi_name` or `pbi_comm`).
pub fn extract_c_char_slice(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).trim().to_string()
}

#[cfg(target_os = "macos")]
pub struct MacOsCollector {
    hostname: Option<String>,
    os_name: Option<String>,
    arch: Option<String>,
    page_size: u64,
    timebase_numer: u64,
    timebase_denom: u64,
}

#[cfg(target_os = "macos")]
impl MacOsCollector {
    pub fn new() -> Self {
        let (os_name, arch) = collect_uname();
        let page_size = unsafe {
            let v = libc::sysconf(libc::_SC_PAGESIZE);
            if v > 0 {
                v as u64
            } else {
                4096
            }
        };

        let (timebase_numer, timebase_denom) = unsafe {
            let mut tb = MaybeUninit::<libc::mach_timebase_info>::zeroed();
            if libc::mach_timebase_info(tb.as_mut_ptr()) == 0 {
                let tb = tb.assume_init();
                (tb.numer.max(1) as u64, tb.denom.max(1) as u64)
            } else {
                (1, 1)
            }
        };

        Self {
            hostname: collect_hostname(),
            os_name,
            arch,
            page_size,
            timebase_numer,
            timebase_denom,
        }
    }

    fn collect_uptime(&self) -> Option<u64> {
        unsafe {
            let mut mib = [libc::CTL_KERN, libc::KERN_BOOTTIME];
            let mut boottime = MaybeUninit::<libc::timeval>::zeroed();
            let mut size = size_of::<libc::timeval>();
            if libc::sysctl(
                mib.as_mut_ptr(),
                2,
                boottime.as_mut_ptr() as *mut libc::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            ) != 0
            {
                return None;
            }
            let boottime = boottime.assume_init();
            let now = libc::time(std::ptr::null_mut());
            if now > boottime.tv_sec {
                Some((now - boottime.tv_sec) as u64)
            } else {
                None
            }
        }
    }

    fn collect_cpu(&self) -> (Option<CpuTimes>, Vec<CpuTimes>) {
        unsafe {
            let mut num_cpus: libc::natural_t = 0;
            let mut cpu_info: libc::processor_info_array_t = std::ptr::null_mut();
            let mut num_cpu_info: libc::mach_msg_type_number_t = 0;

            let kr = libc::host_processor_info(
                libc::mach_host_self(),
                libc::PROCESSOR_CPU_LOAD_INFO,
                &mut num_cpus,
                &mut cpu_info,
                &mut num_cpu_info,
            );
            if kr != libc::KERN_SUCCESS || cpu_info.is_null() || num_cpu_info == 0 {
                return (None, Vec::new());
            }

            let slice = std::slice::from_raw_parts(cpu_info, num_cpu_info as usize);
            let result = parse_mach_cpu_load_info(slice);

            #[allow(deprecated)]
            let _ = libc::vm_deallocate(
                libc::mach_task_self_,
                cpu_info as libc::vm_address_t,
                (num_cpu_info as usize) * size_of::<libc::integer_t>(),
            );

            result
        }
    }

    fn collect_memory_and_swap(&self) -> (Option<u64>, Option<u64>, Option<u64>, Option<u64>) {
        let mem_total = unsafe {
            let mut mib = [libc::CTL_HW, libc::HW_MEMSIZE];
            let mut total: u64 = 0;
            let mut size = size_of::<u64>();
            if libc::sysctl(
                mib.as_mut_ptr(),
                2,
                &mut total as *mut u64 as *mut libc::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            ) == 0
                && total > 0
            {
                Some(total)
            } else {
                None
            }
        };

        let mem_avail = mem_total.and_then(|total| unsafe {
            let mut vm_stat = MaybeUninit::<libc::vm_statistics64>::zeroed();
            let mut count = libc::HOST_VM_INFO64_COUNT;
            let kr = libc::host_statistics64(
                libc::mach_host_self(),
                libc::HOST_VM_INFO64,
                vm_stat.as_mut_ptr() as libc::host_info64_t,
                &mut count,
            );
            if kr == libc::KERN_SUCCESS {
                let s = vm_stat.assume_init();
                let (_, avail) = compute_macos_vm_memory(
                    total,
                    self.page_size,
                    s.free_count as u64,
                    s.inactive_count as u64,
                    s.purgeable_count as u64,
                );
                Some(avail)
            } else {
                None
            }
        });

        let (swap_total, swap_used) = unsafe {
            let mut mib = [libc::CTL_VM, libc::VM_SWAPUSAGE];
            let mut xsw = MaybeUninit::<libc::xsw_usage>::zeroed();
            let mut size = size_of::<libc::xsw_usage>();
            if libc::sysctl(
                mib.as_mut_ptr(),
                2,
                xsw.as_mut_ptr() as *mut libc::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            ) == 0
            {
                let xsw = xsw.assume_init();
                (Some(xsw.xsu_total), Some(xsw.xsu_used))
            } else {
                (None, None)
            }
        };

        (mem_total, mem_avail, swap_total, swap_used)
    }

    fn collect_network(&self) -> BTreeMap<String, RawNetBytes> {
        let mut map = BTreeMap::new();
        unsafe {
            let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
            if libc::getifaddrs(&mut ifap) != 0 || ifap.is_null() {
                return map;
            }
            let mut cur = ifap;
            while !cur.is_null() {
                let ifa = &*cur;
                if !ifa.ifa_addr.is_null()
                    && !ifa.ifa_data.is_null()
                    && !ifa.ifa_name.is_null()
                    && ((*ifa.ifa_addr).sa_family as i32) == libc::AF_LINK
                    && ((ifa.ifa_flags as i32) & libc::IFF_LOOPBACK) == 0
                {
                    let name = CStr::from_ptr(ifa.ifa_name).to_string_lossy().to_string();
                    let data = &*(ifa.ifa_data as *const libc::if_data);
                    let entry = map.entry(name).or_insert(RawNetBytes {
                        rx_bytes: 0,
                        tx_bytes: 0,
                    });
                    entry.rx_bytes = entry.rx_bytes.saturating_add(data.ifi_ibytes as u64);
                    entry.tx_bytes = entry.tx_bytes.saturating_add(data.ifi_obytes as u64);
                }
                cur = ifa.ifa_next;
            }
            libc::freeifaddrs(ifap);
        }
        map
    }

    fn collect_processes(&self) -> (Option<usize>, Vec<RawProcessSample>) {
        unsafe {
            let byte_estimate = libc::proc_listallpids(std::ptr::null_mut(), 0);
            if byte_estimate <= 0 {
                return (None, Vec::new());
            }
            let cap = (byte_estimate as usize / size_of::<libc::pid_t>()).saturating_add(64);
            let mut pids = vec![0 as libc::pid_t; cap];
            let actual = libc::proc_listallpids(
                pids.as_mut_ptr() as *mut libc::c_void,
                (pids.len() * size_of::<libc::pid_t>()) as libc::c_int,
            );
            if actual <= 0 {
                return (None, Vec::new());
            }
            let count = (actual as usize).min(pids.len());
            pids.truncate(count);

            let mut procs = Vec::with_capacity(count);
            for &pid in &pids {
                if pid <= 0 {
                    continue;
                }
                let mut info = MaybeUninit::<libc::proc_taskallinfo>::zeroed();
                let expected_size = size_of::<libc::proc_taskallinfo>() as libc::c_int;
                let got = libc::proc_pidinfo(
                    pid,
                    libc::PROC_PIDTASKALLINFO,
                    0,
                    info.as_mut_ptr() as *mut libc::c_void,
                    expected_size,
                );
                if got != expected_size {
                    continue;
                }
                let info = info.assume_init();
                let name_bytes: &[u8] = std::slice::from_raw_parts(
                    info.pbsd.pbi_name.as_ptr() as *const u8,
                    info.pbsd.pbi_name.len(),
                );
                let mut name = extract_c_char_slice(name_bytes);
                if name.is_empty() {
                    let comm_bytes: &[u8] = std::slice::from_raw_parts(
                        info.pbsd.pbi_comm.as_ptr() as *const u8,
                        info.pbsd.pbi_comm.len(),
                    );
                    name = extract_c_char_slice(comm_bytes);
                }
                if name.is_empty() {
                    name = format!("pid-{}", pid);
                }

                let raw_mach_time = info
                    .ptinfo
                    .pti_total_user
                    .saturating_add(info.ptinfo.pti_total_system);
                let cpu_time_ns = (raw_mach_time as u128)
                    .saturating_mul(self.timebase_numer as u128)
                    .checked_div(self.timebase_denom as u128)
                    .unwrap_or(0) as u64;

                procs.push(RawProcessSample {
                    pid: pid as u32,
                    name,
                    cpu_time_ns,
                    memory_bytes: info.ptinfo.pti_resident_size,
                    start_uptime_secs: None,
                });
            }

            (Some(count), procs)
        }
    }
}

#[cfg(target_os = "macos")]
impl Default for MacOsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl MetricCollector for MacOsCollector {
    fn collect(&mut self, include_processes: bool) -> RawSystemSample {
        let clock = collect_local_clock();
        if self.hostname.is_none() {
            self.hostname = collect_hostname();
        }
        let uptime_secs = self.collect_uptime();
        let (cpu_total, cpu_cores) = self.collect_cpu();
        let (mem_total, mem_avail, swap_total, swap_used) = self.collect_memory_and_swap();
        let (disk_total, disk_used, disk_avail) = collect_root_disk();
        let net_interfaces = self.collect_network();
        let (process_count, processes) = if include_processes {
            self.collect_processes()
        } else {
            (None, Vec::new())
        };

        RawSystemSample {
            clock,
            hostname: self.hostname.clone(),
            os_name: self.os_name.clone(),
            arch: self.arch.clone(),
            uptime_secs,
            cpu_total,
            cpu_cores,
            load_avg: collect_loadavg(),
            mem_total_bytes: mem_total,
            mem_avail_bytes: mem_avail,
            swap_total_bytes: swap_total,
            swap_used_bytes: swap_used,
            disk_root_total_bytes: disk_total,
            disk_root_used_bytes: disk_used,
            disk_root_avail_bytes: disk_avail,
            net_interfaces,
            process_count,
            processes,
        }
    }
}
