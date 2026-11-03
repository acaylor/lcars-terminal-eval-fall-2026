//! Data-bearing ornaments: Braille traces and the concentric sweep gauge.

use crate::canvas::{Canvas, Rect, Style};
use crate::color::Color;
use crate::raster::{blit_braille, blit_quadrants, BrailleGrid, PixelGrid};

/// Map a value to a dot row (0 = top) in a trace of `h` dots.
fn level(v: f32, max: f32, h: i32) -> i32 {
    let f = if max > 0.0 && v.is_finite() { (v / max).clamp(0.0, 1.0) } else { 0.0 };
    (h - 1) - (f * (h - 1) as f32).round() as i32
}

/// A scrolling line trace with a stippled fill below it. `values` are oldest
/// first; the newest sample sits at the right edge. `color(frac)` receives
/// the height fraction of each cell for vertical gradients.
pub fn trace(canvas: &mut Canvas, r: Rect, values: &[f32], max: f32, color: impl Fn(f32) -> Color) {
    if r.is_empty() {
        return;
    }
    let mut g = BrailleGrid::new(r.w, r.h);
    let (w, h) = (g.width(), g.height());
    // Baseline: a sparse dotted floor that reads as "zero" without shouting.
    for x in (0..w).step_by(2) {
        g.set(x, h - 1, 3);
    }
    // Faint quarter gridlines give the empty trace a sense of scale.
    for q in 1..4 {
        let y = (h - 1) - (h - 1) * q / 4;
        for x in (0..w).step_by(6) {
            g.set(x, y, 4);
        }
    }
    let n = values.len().min(w as usize);
    let start = values.len() - n;
    let x0 = w - n as i32;
    let mut prev: Option<i32> = None;
    for (i, &v) in values[start..].iter().enumerate() {
        let x = x0 + i as i32;
        let y = level(v, max, h);
        for fy in (y + 1)..h {
            if (fy + x) % 3 == 0 {
                g.set(x, fy, 2);
            }
        }
        match prev {
            Some(py) => g.vline(x, py, y, 1),
            None => g.set(x, y, 1),
        }
        prev = Some(y);
    }
    let rows = r.h;
    blit_braille(canvas, &g, r.x, r.y, |slot, _, cy| {
        let frac = 1.0 - (cy - r.y) as f32 / rows.max(1) as f32;
        let c = color(frac);
        match slot {
            1 => c,
            2 => c.scale(0.45),
            3 => c.scale(0.3),
            _ => c.scale(0.22),
        }
    });
}

/// Mirrored trace: `up` grows upward from the center line, `down` downward.
pub fn mirror_trace(canvas: &mut Canvas, r: Rect, up: &[f32], down: &[f32], max: f32, up_c: Color, down_c: Color) {
    if r.is_empty() || r.h < 2 {
        return;
    }
    let mut g = BrailleGrid::new(r.w, r.h);
    let (w, h) = (g.width(), g.height());
    let mid = h / 2;
    for x in (0..w).step_by(2) {
        g.set(x, mid, 3);
    }
    let half = mid.max(1);
    for (vals, slot, dir) in [(up, 1u8, -1i32), (down, 2u8, 1i32)] {
        let n = vals.len().min(w as usize);
        let x0 = w - n as i32;
        for (i, &v) in vals[vals.len() - n..].iter().enumerate() {
            let f = if max > 0.0 && v.is_finite() { (v / max).clamp(0.0, 1.0) } else { 0.0 };
            let len = (f * (half - 1) as f32).round() as i32;
            if len > 0 {
                g.vline(x0 + i as i32, mid + dir, mid + dir * len, slot);
            }
        }
    }
    blit_braille(canvas, &g, r.x, r.y, |slot, _, _| match slot {
        1 => up_c,
        2 => down_c,
        _ => up_c.scale(0.3),
    });
}

pub struct Ring<'a> {
    pub label: &'a str,
    /// Filled fraction 0..=1, or None when the metric is unavailable.
    pub value: Option<f32>,
    pub color: Color,
}

/// Geometry of one ring in physical units (1.0 = one cell width).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingSpan {
    pub r_in: f32,
    pub r_out: f32,
}

/// Concentric half-rings, outermost first, fitting in `r` minus its label row.
pub fn ring_spans(r: Rect, n: usize) -> (f32, f32, Vec<RingSpan>) {
    let cx = r.w as f32 / 2.0;
    let base = (r.h - 1).max(0) as f32 * 2.0; // physical height of the arc area
    let radius = (cx - 0.5).min(base - 0.3).max(0.0);
    let th = (radius * 0.17).max(1.6);
    let gap = (radius * 0.07).max(1.0);
    let spans = (0..n)
        .map(|i| {
            let r_out = radius - i as f32 * (th + gap);
            RingSpan { r_in: (r_out - th).max(0.0), r_out: r_out.max(0.0) }
        })
        .collect();
    (cx, base, spans)
}

/// The sweep gauge: concentric half-rings that fill clockwise from the left
/// end over the top. Ring names sit under the left ends, readings under the
/// right ends, so the gauge carries its own legend.
pub fn sweep_gauge(canvas: &mut Canvas, r: Rect, rings: &[Ring], readings: &[String]) {
    if r.w < 8 || r.h < 4 || rings.is_empty() {
        return;
    }
    let (cx, base, spans) = ring_spans(r, rings.len());
    let mut g = PixelGrid::new(r.w * 2, (r.h - 1) * 2);
    for py in 0..g.height() {
        for px in 0..g.width() {
            let x = (px as f32 + 0.5) * 0.5 - cx;
            let y = base - (py as f32 + 0.5);
            if y < 0.0 {
                continue;
            }
            let d = (x * x + y * y).sqrt();
            let theta = y.atan2(-x) / std::f32::consts::PI; // 0 at left, 1 at right
            for (i, (ring, span)) in rings.iter().zip(&spans).enumerate() {
                if d >= span.r_in && d <= span.r_out && span.r_out - span.r_in > 0.5 {
                    let filled = ring.value.is_some_and(|v| theta <= v.clamp(0.0, 1.0));
                    g.set(px, py, (i * 2 + if filled { 1 } else { 2 }) as u8);
                    break;
                }
            }
        }
    }
    blit_quadrants(canvas, &g, r.x, r.y, |slot, _, _| {
        let i = ((slot - 1) / 2) as usize;
        let c = rings.get(i).map(|k| k.color).unwrap_or(Color::Reset);
        if slot % 2 == 1 {
            c
        } else {
            c.scale(0.28)
        }
    });
    // Legend row directly under the ring ends.
    let y = r.bottom() - 1;
    for (i, (ring, span)) in rings.iter().zip(&spans).enumerate() {
        if span.r_out - span.r_in < 2.5 {
            continue; // too thin to carry a three-letter legend
        }
        let lx = r.x + (cx - span.r_out).round() as i32;
        let rx = r.x + (cx + span.r_in).round() as i32;
        let wdt = (span.r_out - span.r_in).round() as i32 + 1;
        canvas.text_max(lx, y, ring.label, Style::fg(ring.color).bold(), wdt.max(3));
        if let Some(t) = readings.get(i) {
            let col = if ring.value.is_some() { ring.color } else { crate::ui::theme::DIM };
            canvas.text_max(rx, y, t, Style::fg(col).bold(), wdt.max(3) + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_newest_sample_is_rightmost() {
        let mut c = Canvas::new(10, 4);
        let vals: Vec<f32> = (0..100).map(|i| if i == 99 { 100.0 } else { 0.0 }).collect();
        trace(&mut c, Rect::new(0, 0, 10, 4), &vals, 100.0, |_| Color::rgb(255, 0, 0));
        // The spike reaches the top row only in the last column.
        let top: Vec<char> = (0..10).map(|x| c.get(x, 0).unwrap().ch).collect();
        assert!(top[9] != ' ' && top[9] != '\u{2800}', "{top:?}");
        assert!(top[..9].iter().all(|&ch| ch == ' '));
    }

    #[test]
    fn gauge_fill_grows_with_value() {
        let ink = |v: f32| {
            let mut c = Canvas::new(30, 10);
            let rings = [Ring { label: "CPU", value: Some(v), color: Color::rgb(255, 153, 0) }];
            sweep_gauge(&mut c, Rect::new(0, 0, 30, 10), &rings, &[]);
            (0..10)
                .flat_map(|y| (0..30).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    c.get(x, y).unwrap().style.fg == Color::rgb(255, 153, 0) || c.get(x, y).unwrap().style.bg == Color::rgb(255, 153, 0)
                })
                .count()
        };
        assert!(ink(0.1) < ink(0.5));
        assert!(ink(0.5) < ink(1.0));
    }

    #[test]
    fn widgets_tolerate_tiny_and_empty_input() {
        let mut c = Canvas::new(5, 5);
        trace(&mut c, Rect::new(0, 0, 0, 0), &[], 0.0, |_| Color::Reset);
        trace(&mut c, Rect::new(3, 3, 10, 10), &[f32::NAN, f32::INFINITY], 0.0, |_| Color::Reset);
        mirror_trace(&mut c, Rect::new(0, 0, 5, 1), &[1.0], &[1.0], 1.0, Color::Reset, Color::Reset);
        sweep_gauge(&mut c, Rect::new(0, 0, 5, 5), &[], &[]);
    }
}
