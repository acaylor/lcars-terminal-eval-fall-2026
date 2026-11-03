//! A tiny scalable stroke font for the large focal readouts.
//!
//! Glyphs are polylines in a unit box, rasterized at any cell size by
//! distance-to-stroke tests in physical space (a terminal cell is treated as
//! twice as tall as it is wide) and then drawn with quadrant glyphs. This
//! gives rounded, condensed numerals at 2x the cell resolution instead of
//! blocky full-cell "big text".

use crate::canvas::Canvas;
use crate::color::Color;
use crate::raster::{blit_quadrants, PixelGrid};
use std::collections::HashMap;

type Pt = (f32, f32);

fn arc(cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32) -> Vec<Pt> {
    let steps = (((a1 - a0).abs() / 12.0).ceil() as usize).max(4);
    (0..=steps)
        .map(|i| {
            let a = (a0 + (a1 - a0) * i as f32 / steps as f32).to_radians();
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

fn join(mut a: Vec<Pt>, b: &[Pt]) -> Vec<Pt> {
    a.extend_from_slice(b);
    a
}

/// Stroke outlines in a unit box (y grows downward; angles are clockwise).
fn strokes(ch: char) -> Vec<Vec<Pt>> {
    let k = 0.27;
    match ch {
        '0' => {
            let top = arc(0.5, k, 0.5, k, 180.0, 360.0);
            let bottom = arc(0.5, 1.0 - k, 0.5, k, 0.0, 180.0);
            vec![join(join(top, &bottom), &[(0.0, k)])]
        }
        '1' => vec![vec![(0.18, 0.2), (0.6, 0.0), (0.6, 1.0)]],
        '2' => vec![join(arc(0.5, 0.26, 0.5, 0.26, 195.0, 385.0), &[(0.0, 1.0), (1.0, 1.0)])],
        '3' => vec![arc(0.48, 0.25, 0.47, 0.25, 205.0, 450.0), arc(0.5, 0.74, 0.5, 0.26, 270.0, 520.0)],
        '4' => vec![vec![(0.75, 1.0), (0.75, 0.0), (0.0, 0.68), (1.0, 0.68)]],
        '5' => vec![join(vec![(0.95, 0.0), (0.08, 0.0), (0.04, 0.47)], &arc(0.5, 0.69, 0.5, 0.31, 218.0, 505.0))],
        '6' => vec![arc(0.5, 0.69, 0.5, 0.31, 0.0, 360.0), vec![(0.0, 0.69), (0.08, 0.32), (0.34, 0.06), (0.82, 0.0)]],
        '7' => vec![vec![(0.0, 0.0), (1.0, 0.0), (0.32, 1.0)]],
        '8' => vec![arc(0.5, 0.23, 0.43, 0.23, 0.0, 360.0), arc(0.5, 0.72, 0.5, 0.28, 0.0, 360.0)],
        '9' => vec![arc(0.5, 0.31, 0.5, 0.31, 0.0, 360.0), vec![(1.0, 0.31), (0.92, 0.68), (0.66, 0.94), (0.18, 1.0)]],
        ':' => vec![vec![(0.5, 0.3)], vec![(0.5, 0.74)]],
        '.' => vec![vec![(0.5, 0.96)]],
        '-' => vec![vec![(0.1, 0.55), (0.9, 0.55)]],
        '%' => vec![arc(0.22, 0.17, 0.2, 0.15, 0.0, 360.0), arc(0.78, 0.83, 0.2, 0.15, 0.0, 360.0), vec![(0.92, 0.0), (0.08, 1.0)]],
        _ => vec![],
    }
}

/// Relative advance of a glyph compared with a digit.
pub fn is_narrow(ch: char) -> bool {
    matches!(ch, ':' | '.')
}

fn seg_dist(p: Pt, a: Pt, b: Pt) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let (wx, wy) = (p.0 - a.0, p.1 - a.1);
    let len2 = vx * vx + vy * vy;
    let t = if len2 <= f32::EPSILON { 0.0 } else { ((wx * vx + wy * vy) / len2).clamp(0.0, 1.0) };
    let (dx, dy) = (p.0 - (a.0 + t * vx), p.1 - (a.1 + t * vy));
    (dx * dx + dy * dy).sqrt()
}

/// Rasterize one glyph into a (2*cols) x (2*rows) quadrant pixel grid.
pub fn rasterize(ch: char, cols: i32, rows: i32) -> PixelGrid {
    let mut g = PixelGrid::new(cols * 2, rows * 2);
    if cols <= 0 || rows <= 0 {
        return g;
    }
    // Physical units: 1.0 = one cell width; a cell is 2.0 tall.
    let (w, h) = (cols as f32, rows as f32 * 2.0);
    let t = (w * 0.26).clamp(0.9, 2.6);
    let map = |(u, v): Pt| (t / 2.0 + u * (w - t), t / 2.0 + v * (h - t));
    let polys: Vec<Vec<Pt>> = strokes(ch).into_iter().map(|p| p.into_iter().map(map).collect()).collect();
    for py in 0..g.height() {
        for px in 0..g.width() {
            let p = ((px as f32 + 0.5) * 0.5, py as f32 + 0.5);
            let mut d = f32::MAX;
            for poly in &polys {
                if poly.len() == 1 {
                    // A dot: slightly heavier than a stroke so it reads at small sizes.
                    d = d.min(seg_dist(p, poly[0], poly[0]) - t * 0.25);
                }
                for s in poly.windows(2) {
                    d = d.min(seg_dist(p, s[0], s[1]));
                }
            }
            if d <= t / 2.0 + 0.04 {
                g.set(px, py, 1);
            }
        }
    }
    g
}

/// Caches rasterized glyphs; bounded so resizes can't grow it forever.
#[derive(Default)]
pub struct Font {
    cache: HashMap<(char, i32, i32), PixelGrid>,
}

impl Font {
    pub fn glyph(&mut self, ch: char, cols: i32, rows: i32) -> &PixelGrid {
        if self.cache.len() > 128 {
            self.cache.clear();
        }
        self.cache.entry((ch, cols, rows)).or_insert_with(|| rasterize(ch, cols, rows))
    }

    /// Columns needed for `s` at the given digit width.
    pub fn measure(s: &str, cols: i32) -> i32 {
        let n = s.chars().count() as i32;
        let w: i32 = s.chars().map(|c| if is_narrow(c) { narrow_cols(cols) } else { cols }).sum();
        w + (n - 1).max(0) * gap(cols)
    }

    /// Draw `s` with its top-left at (x, y); colors come from a horizontal
    /// gradient `from` -> `to` across the text. Returns the width drawn.
    pub fn draw(&mut self, canvas: &mut Canvas, (x, y): (i32, i32), s: &str, (cols, rows): (i32, i32), (from, to): (Color, Color)) -> i32 {
        let total = Font::measure(s, cols).max(1);
        let mut cx = x;
        for ch in s.chars() {
            let cw = if is_narrow(ch) { narrow_cols(cols) } else { cols };
            let g = self.glyph(ch, cw, rows).clone();
            blit_quadrants(canvas, &g, cx, y, |_, px, _| from.lerp(to, (px - x) as f32 / total as f32));
            cx += cw + gap(cols);
        }
        total
    }
}

fn narrow_cols(cols: i32) -> i32 {
    (cols / 2).max(1)
}

fn gap(cols: i32) -> i32 {
    if cols >= 5 {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_digit_has_ink_and_they_are_distinct() {
        let grids: Vec<Vec<u8>> = "0123456789"
            .chars()
            .map(|c| {
                let g = rasterize(c, 5, 5);
                assert!(g.count(1) > 10, "digit {c} is nearly empty");
                (0..g.height()).flat_map(|y| (0..g.width()).map(move |x| (x, y))).map(|(x, y)| g.get(x, y)).collect()
            })
            .collect();
        for i in 0..grids.len() {
            for j in i + 1..grids.len() {
                assert_ne!(grids[i], grids[j], "digits {i} and {j} rasterize identically");
            }
        }
    }

    #[test]
    fn measure_matches_draw() {
        let mut f = Font::default();
        let mut c = Canvas::new(80, 10);
        let w = f.draw(&mut c, (0, 0), "12:34:56", (6, 7), (Color::rgb(255, 0, 0), Color::rgb(0, 0, 255)));
        assert_eq!(w, Font::measure("12:34:56", 6));
        assert!(c.get(w + 1, 3).unwrap().is_blank());
    }

    #[test]
    fn degenerate_sizes_do_not_panic() {
        for (c, r) in [(0, 0), (1, 1), (-3, 2), (1, 40)] {
            rasterize('8', c, r);
        }
    }
}
