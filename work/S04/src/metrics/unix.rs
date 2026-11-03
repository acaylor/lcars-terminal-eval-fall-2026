use super::CivilTime;
use std::ffi::CStr;

pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if rc != 0 {
        return "unknown".into();
    }
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end])
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

pub fn uname_parts() -> (String, String, String) {
    unsafe {
        let mut u: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut u) != 0 {
            return ("unknown".into(), String::new(), "unknown".into());
        }
        (
            cstr(u.sysname.as_ptr()),
            cstr(u.release.as_ptr()),
            cstr(u.machine.as_ptr()),
        )
    }
}

pub fn civil_now() -> CivilTime {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return CivilTime {
                year: 1970,
                month: 1,
                day: 1,
                hour: 0,
                min: 0,
                sec: 0,
            };
        }
        CivilTime {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u8,
            day: tm.tm_mday as u8,
            hour: tm.tm_hour as u8,
            min: tm.tm_min as u8,
            sec: tm.tm_sec as u8,
        }
    }
}

pub fn disk_root() -> Option<(u64, u64)> {
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let path = std::ffi::CString::new("/").ok()?;
    let rc = unsafe { libc::statvfs(path.as_ptr(), &mut st) };
    if rc != 0 || st.f_frsize == 0 {
        return None;
    }
    let fr = st.f_frsize as u64;
    let total = (st.f_blocks as u64).saturating_mul(fr);
    let free = (st.f_bfree as u64).saturating_mul(fr);
    if total == 0 {
        return None;
    }
    Some((total.saturating_sub(free.min(total)), total))
}

pub fn page_size() -> u64 {
    let n = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if n > 0 {
        n as u64
    } else {
        4096
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn clk_tck() -> f64 {
    let n = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if n > 0 {
        n as f64
    } else {
        100.0
    }
}

unsafe fn cstr(p: *const libc::c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    CStr::from_ptr(p)
        .to_string_lossy()
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}
