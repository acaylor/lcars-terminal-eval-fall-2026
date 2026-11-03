use crate::buffer::Buffer;
use crate::theme::{Ink, BOLD, DIM};

pub const BIG_HEIGHT: i32 = 9;

const DIGITS: [[u8; 9]; 10] = [
    [
        0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
    ],
    [
        0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ],
    [
        0b01110, 0b10001, 0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
    ],
    [
        0b11110, 0b00001, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b00001, 0b11110,
    ],
    [
        0b00010, 0b00110, 0b01010, 0b10010, 0b10010, 0b11111, 0b00010, 0b00010, 0b00010,
    ],
    [
        0b11111, 0b10000, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b00001, 0b11110,
    ],
    [
        0b00110, 0b01000, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b01110,
    ],
    [
        0b11111, 0b00001, 0b00010, 0b00010, 0b00100, 0b00100, 0b01000, 0b01000, 0b10000,
    ],
    [
        0b01110, 0b10001, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110,
    ],
    [
        0b01110, 0b10001, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b00010, 0b01100,
    ],
];

const COLON: [u8; 9] = [0, 0, 0, 1, 1, 0, 1, 1, 0];
const DOT: [u8; 9] = [0, 0, 0, 0, 0, 0, 0, 0, 1];

fn glyph(ch: char) -> Option<(&'static [u8; 9], i32)> {
    match ch {
        '0'..='9' => Some((&DIGITS[(ch as u8 - b'0') as usize], 5)),
        ':' => Some((&COLON, 1)),
        '.' => Some((&DOT, 1)),
        _ => None,
    }
}

pub fn big_text_width(text: &str, scale: i32) -> i32 {
    let scale = scale.max(1);
    let mut total = 0;
    let mut count = 0;
    for ch in text.chars() {
        if let Some((_, w)) = glyph(ch) {
            total += w * scale;
            count += 1;
        }
    }
    if count > 1 {
        total + (count - 1)
    } else {
        total
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_big_text(
    b: &mut Buffer,
    x: i32,
    y: i32,
    text: &str,
    scale: i32,
    fg: Ink,
    bg: Ink,
    attrs: u8,
) -> i32 {
    let scale = scale.max(1);
    let block = if b.ascii { '#' } else { '█' };
    let mut cx = x;
    let mut last = x;
    for ch in text.chars() {
        let Some((rows, width)) = glyph(ch) else {
            continue;
        };
        last = cx + width * scale - 1;
        for (py, row) in rows.iter().enumerate() {
            for px in 0..width {
                if row & (1 << (4 - px)) != 0 {
                    for sy in 0..1 {
                        for sx in 0..scale {
                            let px_cell = cx + px * scale + sx;
                            let py_cell = y + py as i32 + sy;
                            let cell_bg = b.cell(px_cell, py_cell).map(|c| c.bg).unwrap_or(bg);
                            b.put(px_cell, py_cell, block, fg, cell_bg, attrs);
                        }
                    }
                }
            }
        }
        cx += width * scale + 1;
    }
    last
}

pub fn pill(b: &mut Buffer, x: i32, y: i32, w: i32, h: i32, color: Ink, uni: bool) {
    if w <= 0 || h <= 0 {
        return;
    }
    b.fill_bg(x, y, w, h, color);
    if w < 2 || h < 2 {
        return;
    }
    if uni {
        b.put(x, y, '▗', color, Ink::Default, 0);
        b.put(x + w - 1, y, '▖', color, Ink::Default, 0);
        b.put(x, y + h - 1, '▘', color, Ink::Default, 0);
        b.put(x + w - 1, y + h - 1, '▝', color, Ink::Default, 0);
        for yy in (y + 1)..(y + h - 1) {
            b.put(x, yy, '▐', color, Ink::Default, 0);
            b.put(x + w - 1, yy, '▌', color, Ink::Default, 0);
        }
    } else {
        b.fill(x, y, 1, 1, ' ', Ink::Default, Ink::Default, 0);
        b.fill(x + w - 1, y, 1, 1, ' ', Ink::Default, Ink::Default, 0);
        b.fill(x, y + h - 1, 1, 1, ' ', Ink::Default, Ink::Default, 0);
        b.fill(
            x + w - 1,
            y + h - 1,
            1,
            1,
            ' ',
            Ink::Default,
            Ink::Default,
            0,
        );
    }
}

/// A vertical LCARS field with a square top (so it can grow out of a header rail)
/// and a generously rounded bottom.
pub fn elbow_capsule(b: &mut Buffer, x: i32, y: i32, w: i32, h: i32, color: Ink, uni: bool) {
    if w <= 0 || h <= 0 {
        return;
    }
    b.fill_bg(x, y, w, h, color);
    if !uni {
        b.fill(x, y + h - 1, 1, 1, ' ', Ink::Default, Ink::Default, 0);
        b.fill(
            x + w - 1,
            y + h - 1,
            1,
            1,
            ' ',
            Ink::Default,
            Ink::Default,
            0,
        );
        return;
    }
    let radius = (h / 5).clamp(2, 4).min(w / 4);
    for i in 0..radius {
        let inset = radius - i;
        let yy = y + h - 1 - i;
        if inset <= 0 {
            continue;
        }
        b.fill(x, yy, inset, 1, ' ', Ink::Default, Ink::Default, 0);
        b.fill(
            x + w - inset,
            yy,
            inset,
            1,
            ' ',
            Ink::Default,
            Ink::Default,
            0,
        );
        let left = if i == 0 { '▘' } else { '▐' };
        let right = if i == 0 { '▝' } else { '▌' };
        b.put(x + inset - 1, yy, left, color, Ink::Default, 0);
        b.put(x + w - inset, yy, right, color, Ink::Default, 0);
    }
}

pub fn pill_label(b: &mut Buffer, x: i32, y: i32, text: &str, fg: Ink, bg: Ink, uni: bool) -> i32 {
    let len = text.chars().count() as i32;
    let w = len + 4;
    pill(b, x, y, w, 3, bg, uni);
    b.text_center_keep_bg(x, y + 1, w, text, fg, 0);
    w
}

pub fn tag(b: &mut Buffer, x: i32, y: i32, text: &str, fg: Ink, bg: Ink, uni: bool) -> i32 {
    let len = text.chars().count() as i32;
    let w = len + 2;
    if uni && len > 0 {
        b.fill_bg(x + 1, y, len, 1, bg);
        b.put(x, y, '▐', bg, Ink::Default, 0);
        b.put(x + w - 1, y, '▌', bg, Ink::Default, 0);
        b.text_keep_bg(x + 1, y, text, fg, 0);
    } else {
        b.fill_bg(x, y, w, 1, bg);
        b.text(x + 1, y, text, fg, bg, 0);
    }
    w
}

pub fn meter(b: &mut Buffer, x: i32, y: i32, w: i32, fraction: Option<f32>, color: Ink) {
    if w <= 0 {
        return;
    }
    match fraction {
        Some(value) if value.is_finite() => {
            let value = value.clamp(0.0, 1.0);
            let filled = (value * w as f32).round() as i32;
            if filled > 0 {
                b.fill_bg(x, y, filled, 1, color);
            }
            if filled < w {
                b.fill_bg(x + filled, y, w - filled, 1, Ink::Track);
            }
        }
        _ => {
            let ch = if b.ascii { '-' } else { '╌' };
            b.hline(x, y, w, ch, Ink::Dim, Ink::Default);
        }
    }
}

/// A single-row rail: a colored stroke for the filled portion and dim dashes
/// for the remainder. Unlike a filled track this does not read as a box.
pub fn thin_meter(b: &mut Buffer, x: i32, y: i32, w: i32, fraction: Option<f32>, color: Ink) {
    if w <= 0 {
        return;
    }
    let filled = match fraction {
        Some(value) if value.is_finite() => (value.clamp(0.0, 1.0) * w as f32).round() as i32,
        _ => {
            let ch = if b.ascii { '-' } else { '╌' };
            b.hline(x, y, w, ch, Ink::Dim, Ink::Default);
            return;
        }
    };
    for i in 0..w {
        if i < filled {
            b.put(
                x + i,
                y,
                if b.ascii { '=' } else { '━' },
                color,
                Ink::Default,
                BOLD,
            );
        } else {
            b.put(
                x + i,
                y,
                if b.ascii { '-' } else { '╌' },
                Ink::Dim,
                Ink::Default,
                0,
            );
        }
    }
}

pub fn segmented_meter(b: &mut Buffer, x: i32, y: i32, w: i32, fraction: Option<f32>, color: Ink) {
    if w <= 0 {
        return;
    }
    let segments = (w / 2).max(1);
    let fraction = fraction
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.0, 1.0));
    for i in 0..segments {
        let sx = x + i * 2;
        let filled = fraction.map(|f| (i as f32 + 0.5) / segments as f32 <= f);
        match filled {
            Some(true) => b.fill_bg(sx, y, 1, 1, color),
            Some(false) => b.fill_bg(sx, y, 1, 1, Ink::Track),
            None => b.put(
                sx,
                y,
                if b.ascii { '-' } else { '╌' },
                Ink::Dim,
                Ink::Default,
                0,
            ),
        }
    }
}

pub fn sparkline(b: &mut Buffer, x: i32, y: i32, w: i32, values: &[f32], color: Ink) {
    if w <= 0 {
        return;
    }
    if values.is_empty() || values.iter().all(|v| !v.is_finite()) {
        let ch = if b.ascii { '.' } else { '·' };
        b.hline(x, y, w, ch, Ink::Dim, Ink::Default);
        return;
    }
    let unicode = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let ascii = [' ', '.', ':', '-', '=', '+', '*', '#'];
    let start = values.len().saturating_sub(w as usize);
    let span = values.len() - start;
    for i in 0..w {
        let idx = start + ((i as usize * span) / w.max(1) as usize).min(span.saturating_sub(1));
        let v = values[idx];
        if !v.is_finite() {
            continue;
        }
        let level = ((v.clamp(0.0, 1.0) * 7.0).round() as usize).min(7);
        let ch = if b.ascii {
            ascii[level]
        } else {
            unicode[level]
        };
        b.put(x + i, y, ch, color, Ink::Default, 0);
    }
}

pub fn pulse(b: &mut Buffer, x: i32, y: i32, phase: u64, color: Ink) {
    let chars = if b.ascii {
        ['*', '+', 'o', '+']
    } else {
        ['●', '◉', '○', '◉']
    };
    let ch = chars[(phase as usize / 3) % chars.len()];
    b.put(x, y, ch, color, Ink::Default, 0);
}

pub fn braille_trace(b: &mut Buffer, x: i32, y: i32, w: i32, phase: u64, color: Ink) {
    let uni = ['⠁', '⠉', '⠙', '⠚', '⠒', '⠲', '⠴', '⠤', '⠄', '⠂'];
    let asc = ['.', ':', '-', '=', '+', '*', '#', '+', '=', '-'];
    let chars = if b.ascii { &asc } else { &uni };
    for i in 0..w {
        let ch = chars[((i as u64 + phase / 2) % chars.len() as u64) as usize];
        b.put(
            x + i,
            y,
            ch,
            color,
            Ink::Default,
            if (i as u64 + phase / 3).is_multiple_of(7) {
                DIM
            } else {
                0
            },
        );
    }
}

pub fn arrow(b: &Buffer, up: bool) -> char {
    if b.ascii {
        if up {
            '^'
        } else {
            'v'
        }
    } else if up {
        '▲'
    } else {
        '▼'
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_clock_width_matches_drawing() {
        assert_eq!(big_text_width("21:06:42", 2), 71);
        let mut b = Buffer::new(80, 10, false);
        let end = draw_big_text(&mut b, 0, 0, "21:06:42", 2, Ink::Amber, Ink::Default, 0);
        assert_eq!(end, 70);
        assert!(b.plain_text().contains('█'));
    }

    #[test]
    fn ascii_big_clock_is_ascii() {
        let mut b = Buffer::new(80, 10, true);
        draw_big_text(&mut b, 0, 0, "12:34:56", 2, Ink::Amber, Ink::Default, 0);
        assert!(b.plain_text().is_ascii());
        assert!(b.plain_text().contains('#'));
    }

    #[test]
    fn pill_caps_are_drawn() {
        let mut b = Buffer::new(20, 5, false);
        pill(&mut b, 0, 0, 10, 3, Ink::Blue, true);
        assert_eq!(b.cell(0, 0).unwrap().ch, '▗');
        assert_eq!(b.cell(0, 1).unwrap().ch, '▐');
        assert_eq!(b.cell(9, 2).unwrap().ch, '▝');
    }
}
