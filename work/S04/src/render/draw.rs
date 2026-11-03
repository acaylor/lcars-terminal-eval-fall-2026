use super::buffer::{fit, fit_ascii, Buffer};
use super::color::{ink_for, Rgb, BLACK, SCAN};
use super::font::{self, blit_cells};

pub fn pill(buf: &mut Buffer, x: i32, y: i32, w: i32, h: i32, color: Rgb, unicode: bool) {
    if w < 2 || h < 1 {
        buf.fill(x, y, w.max(0), h.max(0), color);
        return;
    }
    buf.fill(x, y, w, h, color);
    if !unicode || h < 2 {
        return;
    }
    buf.set(x, y, '▗', color, BLACK);
    buf.set(x + w - 1, y, '▖', color, BLACK);
    buf.set(x, y + h - 1, '▝', color, BLACK);
    buf.set(x + w - 1, y + h - 1, '▘', color, BLACK);
}

pub fn label(buf: &mut Buffer, x: i32, y: i32, w: i32, text: &str, bg: Rgb, unicode: bool) {
    if w <= 2 {
        return;
    }
    let fg = ink_for(bg);
    let body = if unicode {
        fit(text, (w - 2) as usize)
    } else {
        fit_ascii(text, (w - 2) as usize)
    };
    buf.text(x + 1, y, &body, fg, bg);
}

#[allow(clippy::too_many_arguments)]
pub fn elbow_ul(
    buf: &mut Buffer,
    x: i32,
    y: i32,
    rail_w: i32,
    rail_h: i32,
    spine_w: i32,
    spine_h: i32,
    color: Rgb,
    cap: Rgb,
    unicode: bool,
) {
    buf.fill(x, y, rail_w, rail_h, color);
    buf.fill(x, y, spine_w, spine_h, color);
    if rail_w > 10 && rail_h >= 2 {
        let cap_w = 8.min(rail_w / 4);
        buf.fill(x + rail_w - cap_w, y, cap_w, rail_h, cap);
    }
    if rail_h >= 3 && rail_w > 4 {
        buf.clear_rect(x + rail_w - 2, y, 2, 1);
        buf.clear_rect(x + rail_w - 2, y + rail_h - 1, 2, 1);
    }
    if unicode {
        buf.set(x, y, '▗', color, BLACK);
        if spine_h > rail_h {
            buf.set(x, y + spine_h - 1, '▝', color, BLACK);
            if spine_w > 1 {
                buf.set(x + spine_w - 1, y + spine_h - 1, '▘', color, BLACK);
            }
        }
        let ix = x + spine_w;
        let iy = y + rail_h;
        if ix < x + rail_w {
            buf.set(ix, iy, '▘', color, BLACK);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn elbow_ur(
    buf: &mut Buffer,
    right: i32,
    y: i32,
    rail_w: i32,
    rail_h: i32,
    spine_w: i32,
    spine_h: i32,
    color: Rgb,
    cap: Rgb,
    unicode: bool,
) {
    let x = right - rail_w;
    let sx = right - spine_w;
    buf.fill(x, y, rail_w, rail_h, color);
    buf.fill(sx, y, spine_w, spine_h, color);
    if rail_w > 10 && rail_h >= 2 {
        let cap_w = 8.min(rail_w / 4);
        buf.fill(x, y, cap_w, rail_h, cap);
    }
    if rail_h >= 3 && rail_w > 4 {
        buf.clear_rect(x, y, 2, 1);
        buf.clear_rect(x, y + rail_h - 1, 2, 1);
    }
    if unicode {
        buf.set(right - 1, y, '▖', color, BLACK);
        if spine_h > rail_h {
            buf.set(right - 1, y + spine_h - 1, '▘', color, BLACK);
        }
        buf.set(sx - 1, y + rail_h, '▝', color, BLACK);
    }
}

pub fn nub(buf: &mut Buffer, x: i32, y: i32, text: &str, color: Rgb, unicode: bool) {
    pill(buf, x, y, 5, 2, color, unicode);
    label(buf, x, y, 5, text, color, unicode);
}

#[allow(clippy::too_many_arguments)]
pub fn clock(
    buf: &mut Buffer,
    x: i32,
    y: i32,
    hms: &str,
    color: Rgb,
    colon: Rgb,
    big: bool,
    unicode: bool,
) -> (i32, i32) {
    let mut cx = x;
    let mut height = 0;
    for ch in hms.chars() {
        if ch == ':' {
            let rows: &[&str] = if big { &font::COLON } else { &font::MCOLON };
            let (w, h) = blit_cells(buf, cx, y, rows, colon, unicode);
            cx += w + 1;
            height = height.max(h);
        } else if ch.is_ascii_digit() {
            let d = (ch as u8) - b'0';
            let rows = if big {
                font::big_digit(d)
            } else {
                font::med_digit(d)
            };
            let (w, h) = blit_cells(buf, cx, y, rows, color, unicode);
            cx += w + 1;
            height = height.max(h);
        }
    }
    (cx - x, height)
}

pub fn load_digits(buf: &mut Buffer, x: i32, y: i32, text: &str, color: Rgb, unicode: bool) -> i32 {
    let mut cx = x;
    for ch in text.chars() {
        if ch == '.' {
            buf.fill(cx, y + 3, 2, 2, color);
            cx += 3;
        } else if ch.is_ascii_digit() {
            let d = (ch as u8) - b'0';
            let (w, _) = blit_cells(buf, cx, y, font::med_digit(d), color, unicode);
            cx += w + 1;
        } else if ch == '-' {
            buf.fill(cx, y + 2, 4, 1, color);
            cx += 5;
        }
    }
    cx - x
}

pub fn sparkline(
    buf: &mut Buffer,
    x: i32,
    y: i32,
    w: i32,
    values: &[f64],
    color: Rgb,
    unicode: bool,
) {
    if w <= 0 {
        return;
    }
    if !unicode || values.is_empty() {
        for i in 0..w {
            let ch = if unicode { '·' } else { '.' };
            buf.set(x + i, y, ch, color, BLACK);
        }
        return;
    }
    let n = values.len();
    for i in 0..w {
        let idx = if w == 1 {
            n - 1
        } else {
            (i as usize * (n - 1)) / (w as usize - 1).max(1)
        };
        let v = values.get(idx).copied().unwrap_or(0.0).clamp(0.0, 100.0);
        let level = (v / 25.0).ceil() as u8;
        let bits = match level {
            0 => 0,
            1 => 0x40,
            2 => 0x40 | 0x04,
            3 => 0x40 | 0x04 | 0x02,
            _ => 0x40 | 0x04 | 0x02 | 0x01,
        };
        let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
        buf.set(x + i, y, ch, color, BLACK);
    }
}

pub fn scanner(buf: &mut Buffer, x: i32, y: i32, rail_w: i32, frame: u64) {
    if rail_w < 12 {
        return;
    }
    let span = (rail_w - 10).max(1) as u64;
    let pos = (frame * 2) % span;
    let sx = x + 4 + pos as i32;
    for i in 0..4 {
        let c = buf.cell(sx + i, y);
        if c.bg != BLACK {
            buf.set(sx + i, y, c.ch, c.fg, SCAN);
        }
    }
}
