use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clock {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub weekday: u8,
    pub unix: i64,
}

const WEEKDAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

impl Clock {
    pub fn now_local() -> Option<Self> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
        let secs = now.as_secs().min(i64::MAX as u64) as i64;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let time = secs as libc::time_t;
        let result = unsafe { libc::localtime_r(&time, &mut tm) };
        if result.is_null() {
            return None;
        }
        Some(Self {
            hour: tm.tm_hour.clamp(0, 23) as u8,
            minute: tm.tm_min.clamp(0, 59) as u8,
            second: tm.tm_sec.clamp(0, 60) as u8,
            year: tm.tm_year + 1900,
            month: (tm.tm_mon.clamp(0, 11) + 1) as u8,
            day: tm.tm_mday.clamp(1, 31) as u8,
            weekday: tm.tm_wday.clamp(0, 6) as u8,
            unix: secs,
        })
    }

    pub fn hms(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }

    pub fn hm(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    pub fn date_upper(&self) -> String {
        format!(
            "{} {:02} {} {}",
            WEEKDAYS[self.weekday as usize],
            self.day,
            MONTHS[(self.month.max(1) - 1) as usize],
            self.year
        )
    }

    pub fn date_short(&self) -> String {
        format!(
            "{:02} {} {}",
            self.day,
            MONTHS[(self.month.max(1) - 1) as usize],
            self.year
        )
    }

    pub fn seconds_progress(&self) -> f32 {
        self.second as f32 / 60.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_clock_labels() {
        let c = Clock {
            hour: 7,
            minute: 4,
            second: 9,
            year: 2026,
            month: 10,
            day: 27,
            weekday: 2,
            unix: 0,
        };
        assert_eq!(c.hms(), "07:04:09");
        assert_eq!(c.date_upper(), "TUE 27 OCT 2026");
        assert_eq!(c.date_short(), "27 OCT 2026");
    }
}
