//! Local wall-clock time via localtime_r (honours TZ without shelling out).

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub hour: u8,
    pub min: u8,
    pub sec: u8,
    pub day: u8,
    /// 0 = January.
    pub month: u8,
    pub year: i32,
    /// 0 = Sunday.
    pub weekday: u8,
    pub yday: u16,
    pub zone: String,
}

pub const WEEKDAYS: [&str; 7] = ["SUNDAY", "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY"];
pub const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

impl LocalTime {
    pub fn now() -> LocalTime {
        // SAFETY: time(NULL) and localtime_r with valid pointers.
        let t = unsafe { libc::time(std::ptr::null_mut()) };
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return LocalTime::fixed(0, 0, 0);
        }
        let zone = if tm.tm_zone.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr(tm.tm_zone) }.to_string_lossy().into_owned() };
        LocalTime {
            hour: tm.tm_hour.clamp(0, 23) as u8,
            min: tm.tm_min.clamp(0, 59) as u8,
            sec: tm.tm_sec.clamp(0, 60) as u8,
            day: tm.tm_mday.clamp(1, 31) as u8,
            month: tm.tm_mon.clamp(0, 11) as u8,
            year: tm.tm_year + 1900,
            weekday: tm.tm_wday.clamp(0, 6) as u8,
            yday: tm.tm_yday.clamp(0, 365) as u16,
            zone,
        }
    }

    /// A deterministic time for tests and reproducible snapshots.
    pub fn fixed(hour: u8, min: u8, sec: u8) -> LocalTime {
        LocalTime { hour, min, sec, day: 22, month: 8, year: 2026, weekday: 2, yday: 264, zone: "UTC".into() }
    }

    pub fn hms(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.min, self.sec.min(59))
    }

    /// "TUESDAY 22 SEP 2026"
    pub fn long_date(&self) -> String {
        format!("{} {:02} {} {}", WEEKDAYS[self.weekday as usize], self.day, MONTHS[self.month as usize], self.year)
    }

    /// "2026-09-22"
    pub fn iso_date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month + 1, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        let t = LocalTime::fixed(7, 5, 3);
        assert_eq!(t.hms(), "07:05:03");
        assert_eq!(t.long_date(), "TUESDAY 22 SEP 2026");
        assert_eq!(t.iso_date(), "2026-09-22");
        let now = LocalTime::now();
        assert!(now.hour < 24 && (1..=31).contains(&now.day));
    }
}
