use crate::glyphs::{big_text_width, BIG_HEIGHT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };

    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    pub fn inside(&self, w: i32, h: i32) -> bool {
        self.x >= 0 && self.y >= 0 && self.right() <= w && self.bottom() <= h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Density {
    Tiny,
    Compact,
    Full,
}

#[derive(Clone, Debug)]
pub struct PrimaryLayout {
    pub w: i32,
    pub h: i32,
    pub density: Density,
    pub header: Rect,
    pub spine: Rect,
    pub clock: Rect,
    pub clock_scale: i32,
    pub clock_label_y: i32,
    pub status_y: i32,
    pub sweep: Rect,
    pub date_y: i32,
    pub host_y: i32,
    pub trace: Rect,
    pub cpu: Rect,
    pub memory: Rect,
    pub disk: Rect,
    pub net: Rect,
    pub footer: Rect,
}

impl PrimaryLayout {
    pub fn new(w: i32, h: i32) -> Self {
        if w < 80 || h < 24 {
            return Self::tiny(w, h);
        }
        if w < 112 || h < 35 {
            return Self::compact(w, h);
        }
        Self::full(w, h)
    }

    fn tiny(w: i32, h: i32) -> Self {
        Self {
            w,
            h,
            density: Density::Tiny,
            header: Rect::new(0, 0, w, 1),
            spine: Rect::ZERO,
            clock: Rect::ZERO,
            clock_scale: 0,
            clock_label_y: 1,
            status_y: 1,
            sweep: Rect::ZERO,
            date_y: 2,
            host_y: 3,
            trace: Rect::ZERO,
            cpu: Rect::ZERO,
            memory: Rect::ZERO,
            disk: Rect::ZERO,
            net: Rect::ZERO,
            footer: Rect::new(0, (h - 1).max(0), w, 1),
        }
    }

    fn compact(w: i32, h: i32) -> Self {
        let header_h = 2;
        let right_w = 28;
        let right_x = w - right_w - 2;
        let clock_x = 4;
        let clock_scale = if big_text_width("21:06:42", 2) <= right_x - clock_x - 2 {
            2
        } else {
            1
        };
        let clock_w = big_text_width("21:06:42", clock_scale);
        let clock_y = header_h + 1;
        let cpu_h = (h - clock_y - 11).clamp(8, 11);
        let lower_y = (clock_y + BIG_HEIGHT + 3).max(14);
        Self {
            w,
            h,
            density: Density::Compact,
            header: Rect::new(0, 0, w, header_h),
            spine: Rect::new(0, header_h, 2, h - header_h - 2),
            clock: Rect::new(clock_x, clock_y, clock_w, BIG_HEIGHT),
            clock_scale,
            clock_label_y: header_h,
            status_y: header_h,
            sweep: Rect::new(clock_x, clock_y + BIG_HEIGHT, clock_w, 1),
            date_y: clock_y + BIG_HEIGHT + 1,
            host_y: clock_y + BIG_HEIGHT + 2,
            trace: Rect::new(clock_x, h - 3, w - clock_x - 2, 1),
            cpu: Rect::new(right_x, clock_y, right_w, cpu_h),
            memory: Rect::new(4, lower_y, right_x - 6, 3),
            disk: Rect::new(4, lower_y + 3, right_x - 6, 3),
            net: Rect::new(right_x, lower_y, right_w, 6),
            footer: Rect::new(0, h - 1, w, 1),
        }
    }

    fn full(w: i32, h: i32) -> Self {
        let header_h = 3;
        let right_w = if w >= 112 { 26 } else { 24 };
        let right_x = w - right_w - 2;
        let clock_x = 10;
        let clock_y = header_h + 2;
        let available = right_x - clock_x - 3;
        let clock_scale = if big_text_width("21:06:42", 3) <= available {
            3
        } else if big_text_width("21:06:42", 2) <= available {
            2
        } else {
            1
        };
        let clock_w = big_text_width("21:06:42", clock_scale);
        let memory_y = (clock_y + BIG_HEIGHT + 13)
            .min(h - 9)
            .max(clock_y + BIG_HEIGHT + 4);
        let trace_y = (clock_y + BIG_HEIGHT + 8).min(memory_y - 2);
        let footer_y = h - 1;
        Self {
            w,
            h,
            density: Density::Full,
            header: Rect::new(0, 0, w, header_h),
            spine: Rect::new(0, header_h, 5, h - header_h - 2),
            clock: Rect::new(clock_x, clock_y, clock_w, BIG_HEIGHT),
            clock_scale,
            clock_label_y: header_h + 1,
            status_y: header_h,
            sweep: Rect::new(clock_x, clock_y + BIG_HEIGHT, clock_w, 1),
            date_y: clock_y + BIG_HEIGHT + 1,
            host_y: clock_y + BIG_HEIGHT + 2,
            trace: Rect::new(clock_x, trace_y, clock_w, 1),
            cpu: Rect::new(right_x, header_h - 1, right_w, 23),
            memory: Rect::new(clock_x, memory_y, right_x - clock_x - 4, 4),
            disk: Rect::ZERO,
            net: Rect::new(
                right_x,
                (footer_y - 9).max(header_h - 1 + 23 + 1),
                right_w,
                ((footer_y) - (footer_y - 9).max(header_h + 22 + 1)).clamp(0, 8),
            ),
            footer: Rect::new(0, footer_y, w, 1),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiagnosticsLayout {
    pub w: i32,
    pub h: i32,
    pub density: Density,
    pub header: Rect,
    pub spine: Rect,
    pub left: Rect,
    pub right: Rect,
    pub footer: Rect,
    pub title_y: i32,
}

impl DiagnosticsLayout {
    pub fn new(w: i32, h: i32) -> Self {
        let density = if w < 80 || h < 20 {
            Density::Tiny
        } else if w < 120 || h < 28 {
            Density::Compact
        } else {
            Density::Full
        };
        match density {
            Density::Full => {
                let header_h = 3;
                Self {
                    w,
                    h,
                    density,
                    header: Rect::new(0, 0, w, header_h),
                    spine: Rect::new(0, header_h, 5, h - header_h - 2),
                    left: Rect::new(10, 6, 46, h - 12),
                    right: Rect::new(w - 60, 4, 58, h - 8),
                    footer: Rect::new(0, h - 1, w, 1),
                    title_y: header_h,
                }
            }
            Density::Compact => {
                let header_h = 2;
                Self {
                    w,
                    h,
                    density,
                    header: Rect::new(0, 0, w, header_h),
                    spine: Rect::new(0, header_h, 2, h - header_h - 2),
                    left: Rect::new(4, 3, (w / 2 - 6).max(10), h - 8),
                    right: Rect::new(w / 2, 3, w / 2 - 2, h - 8),
                    footer: Rect::new(0, h - 1, w, 1),
                    title_y: header_h,
                }
            }
            Density::Tiny => Self {
                w,
                h,
                density,
                header: Rect::new(0, 0, w, 1),
                spine: Rect::ZERO,
                left: Rect::new(0, 1, w, h - 2),
                right: Rect::ZERO,
                footer: Rect::new(0, (h - 1).max(0), w, 1),
                title_y: 1,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_rects_stay_inside_and_do_not_collide() {
        for (w, h) in [
            (120, 36),
            (80, 24),
            (100, 30),
            (110, 34),
            (110, 35),
            (112, 28),
            (112, 35),
            (200, 60),
            (60, 16),
        ] {
            let l = PrimaryLayout::new(w, h);
            for r in [
                l.header, l.spine, l.clock, l.sweep, l.trace, l.cpu, l.memory, l.disk, l.net,
                l.footer,
            ] {
                if !r.is_empty() {
                    assert!(r.inside(w, h), "{r:?} outside {w}x{h}");
                }
            }
            if !l.cpu.is_empty() && !l.clock.is_empty() {
                assert!(!l.clock.overlaps(&l.cpu), "clock/cpu collide at {w}x{h}");
            }
            if !l.memory.is_empty() && !l.cpu.is_empty() {
                assert!(!l.memory.overlaps(&l.cpu));
            }
            if !l.net.is_empty() && !l.memory.is_empty() {
                assert!(!l.net.overlaps(&l.memory));
            }
            assert!(!l.clock.overlaps(&l.memory));
            if !l.trace.is_empty() && !l.memory.is_empty() {
                assert!(!l.trace.overlaps(&l.memory));
            }
            if !l.trace.is_empty() && !l.sweep.is_empty() {
                assert!(!l.trace.overlaps(&l.sweep));
            }
        }
    }

    #[test]
    fn full_layout_uses_a_large_clock() {
        let l = PrimaryLayout::new(120, 36);
        assert_eq!(l.density, Density::Full);
        assert_eq!(l.clock_scale, 2);
        assert_eq!(l.clock.w, 71);
        assert_eq!(l.cpu.x, 92);
        assert!(l.cpu.x - l.clock.right() >= 8);
    }

    #[test]
    fn compact_and_tiny_are_safe() {
        assert_eq!(PrimaryLayout::new(80, 24).density, Density::Compact);
        assert_eq!(PrimaryLayout::new(40, 12).density, Density::Tiny);
        assert_eq!(PrimaryLayout::new(8, 3).density, Density::Tiny);
        let d = DiagnosticsLayout::new(8, 3);
        assert_eq!(d.density, Density::Tiny);
    }
}
