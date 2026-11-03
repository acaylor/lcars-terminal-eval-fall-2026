//! Data-bearing shapes: braille traces, gradient gauges, core banks, pulses.

use crate::term::{Buffer, Color, Glyphs, Style};

/// Line trace drawn with braille dots (2 samples per column, 4 levels per
/// row). `data` holds normalised 0..=1 samples, oldest first, `None` = gap.
/// Colour follows the sample value through `tint`.
#[allow(clippy::too_many_arguments)]
pub fn trace(
    buf: &mut Buffer,
    g: &Glyphs,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    data: &[Option<f32>],
    tint: impl Fn(f32) -> Color,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let levels = h * 4;
    let n = (w * 2) as usize;
    let start = data.len().saturating_sub(n);
    let samples = &data[start..];
    // Baseline so the trace reads as a trace even when flat or empty.
    let base = Style::fg(Color::SHADOW);
    for col in 0..w {
        buf.set(x + col, y + h - 1, g.track, base);
    }
    let pad = n - samples.len();
    for col in 0..w {
        // Each sub-column resolves to (row, dot bit) or nothing.
        let mut cells: [Option<(i32, u8)>; 2] = [None, None];
        let mut strength = 0.0f32;
        for (sub, slot) in cells.iter_mut().enumerate() {
            let idx = col as usize * 2 + sub;
            if idx < pad {
                continue;
            }
            let Some(Some(v)) = samples.get(idx - pad) else {
                continue;
            };
            let v = v.clamp(0.0, 1.0);
            let level = ((v * (levels - 1) as f32).round() as i32).clamp(0, levels - 1);
            *slot = Some((h - 1 - level / 4, 1u8 << (3 - level % 4)));
            strength = strength.max(v);
        }
        let st = Style::fg(tint(strength));
        match cells {
            [None, None] => {}
            [Some((r0, d0)), Some((r1, d1))] if r0 == r1 => buf.set(x + col, y + r0, g.braille(d0, d1), st),
            [l, r] => {
                if let Some((r0, d0)) = l {
                    buf.set(x + col, y + r0, g.braille(d0, 0), st);
                }
                if let Some((r1, d1)) = r {
                    buf.set(x + col, y + r1, g.braille(0, d1), st);
                }
            }
        }
    }
}

/// Horizontal gauge, one row: filled part in a gradient, empty part as a
/// thin track so the extent is visible without colour. `parts` are
/// (fraction, colour) pairs stacked left to right.
pub fn gauge(buf: &mut Buffer, g: &Glyphs, x: i32, y: i32, w: i32, parts: &[(f32, Color)], gradient: bool) {
    if w <= 0 {
        return;
    }
    for col in 0..w {
        buf.set(x + col, y, g.track, Style::fg(Color::SHADOW));
    }
    let mut cursor = 0.0f32;
    for (frac, color) in parts {
        let frac = frac.clamp(0.0, 1.0);
        let from = cursor * w as f32;
        let to = (cursor + frac).min(1.0) * w as f32;
        let full_start = from.ceil() as i32;
        let full_end = to.floor() as i32;
        for col in full_start..full_end {
            let t = col as f32 / w.max(1) as f32;
            let c = if gradient {
                color.scale(0.8).mix(*color, t)
            } else {
                *color
            };
            buf.set(x + col, y, g.full, Style::fg(c));
        }
        // Partial cell at the right edge, in eighths.
        let rem = to - to.floor();
        if full_end < w && rem > 0.0 && full_end >= full_start {
            let eighths = ((rem * 8.0).round() as usize).clamp(1, 8);
            buf.set(x + full_end, y, g.hlevels[eighths], Style::fg(*color));
        }
        cursor += frac;
    }
}

/// Column pitch for a core bank: 3 (two-column bars with a gap, labelled),
/// 2 (one-column bars with a gap) or 1 (packed) so that `n` cores fit in `w`.
pub fn core_pitch(n: usize, w: i32) -> i32 {
    let fits = |pitch: i32| (n as i32 * pitch - 1) <= w;
    if fits(3) {
        3
    } else if fits(2) {
        2
    } else {
        1
    }
}

/// Vertical bank of per-core bars, `h` rows tall, laid out at `pitch`
/// columns per core (bar width is pitch-1, or 1 when packed). Returns the
/// number of cores that fit.
#[allow(clippy::too_many_arguments)]
pub fn core_bank(
    buf: &mut Buffer,
    g: &Glyphs,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    cores: &[Option<f32>],
    pitch: i32,
    label: bool,
) -> usize {
    let pitch = pitch.clamp(1, 3);
    let bar_w = if pitch == 1 { 1 } else { pitch - 1 };
    let per = ((w + (pitch - bar_w)) / pitch).max(0) as usize;
    let n = cores.len().min(per);
    for (i, v) in cores.iter().take(n).enumerate() {
        let cx = x + i as i32 * pitch;
        match v {
            Some(v) => {
                let v = v.clamp(0.0, 1.0);
                let total = (v * (h * 8) as f32).round() as i32;
                let c = Color::heat(v);
                for row in 0..h {
                    let from_bottom = h - 1 - row;
                    let eighths = (total - from_bottom * 8).clamp(0, 8) as usize;
                    let ch = if eighths == 0 { g.track } else { g.levels[eighths] };
                    let st = if eighths == 0 {
                        Style::fg(Color::SHADOW)
                    } else {
                        Style::fg(c)
                    };
                    for b in 0..bar_w {
                        buf.set(cx + b, y + row, ch, st);
                    }
                }
            }
            None => {
                for row in 0..h {
                    for b in 0..bar_w {
                        buf.set(cx + b, y + row, g.bullet, Style::fg(Color::DIM));
                    }
                }
            }
        }
        if label && pitch == 3 {
            buf.put_str(cx, y + h, &format!("{i:02}"), Style::fg(Color::DIM));
        }
    }
    n
}

/// Four-cell pulse indicator that walks with the frame counter.
pub fn pulse(buf: &mut Buffer, g: &Glyphs, x: i32, y: i32, frame: u64, color: Color, active: bool) -> i32 {
    for i in 0..4u64 {
        let on = active && (frame / 2) % 4 == i;
        let ch = if on { g.pulse_on } else { g.pulse_off };
        let st = if on {
            Style::fg(color)
        } else {
            Style::fg(color.scale(0.5))
        };
        buf.set(x + i as i32 * 2, y, ch, st);
    }
    x + 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_places_dots_by_value() {
        let mut b = Buffer::new(4, 2);
        let data: Vec<Option<f32>> = vec![
            Some(0.0),
            Some(0.0),
            Some(1.0),
            Some(1.0),
            None,
            None,
            Some(0.5),
            Some(0.5),
        ];
        trace(&mut b, &Glyphs::unicode(), 0, 0, 4, 2, &data, |_| Color::BLUE);
        let rows = b.rows_text();
        // Column 0 at the bottom row, column 1 at the top row, column 2 gap, column 3 mid.
        assert_eq!(rows[1].chars().next().unwrap(), '\u{28C0}');
        assert_eq!(rows[0].chars().nth(1).unwrap(), '\u{2809}');
        assert_eq!(rows[1].chars().nth(2).unwrap(), '▁');
        assert_eq!(rows[0].chars().nth(3).unwrap(), '\u{28C0}');
    }

    #[test]
    fn trace_tolerates_short_and_empty_data() {
        let mut b = Buffer::new(10, 3);
        trace(&mut b, &Glyphs::unicode(), 0, 0, 10, 3, &[], |_| Color::BLUE);
        trace(&mut b, &Glyphs::unicode(), 0, 0, 10, 3, &[Some(0.3)], |_| Color::BLUE);
        trace(&mut b, &Glyphs::ascii(), 0, 0, 0, 0, &[Some(0.3)], |_| Color::BLUE);
        assert!(b.rows_text()[2].starts_with('▁'));
    }

    #[test]
    fn gauge_fills_proportionally() {
        let mut b = Buffer::new(10, 1);
        gauge(
            &mut b,
            &Glyphs::unicode(),
            0,
            0,
            10,
            &[(0.5, Color::ORANGE), (0.25, Color::BLUE)],
            false,
        );
        assert_eq!(b.rows_text()[0], "███████▌▁▁");
        let mut a = Buffer::new(10, 1);
        gauge(&mut a, &Glyphs::ascii(), 0, 0, 10, &[(1.0, Color::ORANGE)], true);
        assert_eq!(a.rows_text()[0], "##########");
    }

    #[test]
    fn core_bank_caps_to_width() {
        let mut b = Buffer::new(8, 5);
        let cores = vec![Some(1.0), Some(0.0), None, Some(0.5)];
        let n = core_bank(&mut b, &Glyphs::unicode(), 0, 0, 8, 4, &cores, 3, true);
        assert_eq!(n, 3);
        let rows = b.rows_text();
        assert!(rows[0].starts_with("██"));
        assert!(rows[4].starts_with("00 01 02"));
        assert_eq!(core_pitch(24, 71), 3);
        assert_eq!(core_pitch(24, 70), 2);
        assert_eq!(core_pitch(24, 30), 1);
        let mut p = Buffer::new(30, 2);
        assert_eq!(
            core_bank(&mut p, &Glyphs::unicode(), 0, 0, 30, 2, &vec![Some(1.0); 40], 1, true),
            30
        );
        assert_eq!(p.rows_text()[0], "██████████████████████████████");
    }
}
