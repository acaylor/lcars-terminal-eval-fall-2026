//! LCARS frame geometry. Elbows, rails and blocks are painted into one
//! screen-sized quadrant raster (2x2 pixels per cell) so curves and
//! half-row rails get twice the terminal's native resolution, then labels
//! are cut into the solid cells afterwards.

use crate::canvas::{Canvas, Style};
use crate::color::Color;
use crate::raster::{blit_quadrants, PixelGrid};
use crate::ui::theme::INK;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

pub struct Frame {
    grid: PixelGrid,
    colors: Vec<Color>,
}

impl Frame {
    /// A frame raster covering a `cols` x `rows` canvas.
    pub fn new(cols: i32, rows: i32) -> Frame {
        Frame { grid: PixelGrid::new(cols * 2, rows * 2), colors: Vec::new() }
    }

    fn slot(&mut self, c: Color) -> u8 {
        if let Some(i) = self.colors.iter().position(|&k| k == c) {
            return (i + 1) as u8;
        }
        if self.colors.len() >= 250 {
            return 1;
        }
        self.colors.push(c);
        self.colors.len() as u8
    }

    /// Fill pixel rectangle [x0,x1) x [y0,y1).
    pub fn rect_px(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Color) {
        let s = self.slot(c);
        self.grid.fill_rect(x0, y0, x1, y1, s);
    }

    /// Rectangle with elliptical corners (radii in pixels) on selected corners
    /// [top-left, top-right, bottom-right, bottom-left].
    pub fn rounded_px(&mut self, (x0, y0, x1, y1): (i32, i32, i32, i32), (rx, ry): (i32, i32), corners: [bool; 4], c: Color) {
        let s = self.slot(c);
        let (w, h) = (x1 - x0, y1 - y0);
        if w <= 0 || h <= 0 {
            return;
        }
        let (rx, ry) = (rx.min(w / 2).max(0), ry.min(h / 2).max(0));
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x.min(w - 1 - x), y.min(h - 1 - y));
                let left = x < w - 1 - x;
                let top = y < h - 1 - y;
                let which = match (top, left) {
                    (true, true) => 0,
                    (true, false) => 1,
                    (false, false) => 2,
                    (false, true) => 3,
                };
                if corners[which] && rx > 0 && ry > 0 && fx < rx && fy < ry && !inside_ellipse(fx, fy, rx, ry) {
                    continue;
                }
                self.grid.set(x0 + x, y0 + y, s);
            }
        }
    }

    /// An LCARS elbow: a vertical sidebar of width `side_w` joined to a
    /// horizontal bar of height `bar_h`, with a rounded outer corner and a
    /// concave inner fillet. Box and sizes are in pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn elbow_px(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        side_w: i32,
        bar_h: i32,
        corner: Corner,
        outer: (i32, i32),
        inner: (i32, i32),
        c: Color,
    ) {
        let s = self.slot(c);
        let (w, h) = (x1 - x0, y1 - y0);
        for y in 0..h.max(0) {
            for x in 0..w.max(0) {
                // Transform into the canonical top-left orientation.
                let (u, v) = match corner {
                    Corner::TopLeft => (x, y),
                    Corner::TopRight => (w - 1 - x, y),
                    Corner::BottomLeft => (x, h - 1 - y),
                    Corner::BottomRight => (w - 1 - x, h - 1 - y),
                };
                let mut on = u < side_w || v < bar_h;
                if on && u < outer.0 && v < outer.1 && !inside_ellipse(u, v, outer.0, outer.1) {
                    on = false;
                }
                if !on && inner.0 > 0 && inner.1 > 0 && u >= side_w && v >= bar_h && u < side_w + inner.0 && v < bar_h + inner.1 {
                    // Fillet: the region outside a quarter ellipse tucked into the joint.
                    let (cu, cv) = (u - side_w, v - bar_h);
                    let dx = (cu as f32 + 0.5 - inner.0 as f32) / inner.0 as f32;
                    let dy = (cv as f32 + 0.5 - inner.1 as f32) / inner.1 as f32;
                    on = dx * dx + dy * dy > 1.0;
                }
                if on {
                    self.grid.set(x0 + x, y0 + y, s);
                }
            }
        }
    }

    pub fn blit(&self, canvas: &mut Canvas) {
        self.blit_at(canvas, 0, 0);
    }

    /// Blit with the raster's top-left at cell (x, y).
    pub fn blit_at(&self, canvas: &mut Canvas, x: i32, y: i32) {
        let colors = &self.colors;
        blit_quadrants(canvas, &self.grid, x, y, |slot, _, _| colors.get(slot as usize - 1).copied().unwrap_or(Color::Reset));
    }
}

/// Is pixel (x, y) of a corner box inside the quarter ellipse whose center is
/// at (rx, ry)? Used for convex rounding of the outer corner.
fn inside_ellipse(x: i32, y: i32, rx: i32, ry: i32) -> bool {
    let dx = (x as f32 + 0.5 - rx as f32) / rx as f32;
    let dy = (y as f32 + 0.5 - ry as f32) / ry as f32;
    dx * dx + dy * dy <= 1.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Center,
}

/// A one-row pill with round caps and a label cut into it.
pub fn pill(canvas: &mut Canvas, x: i32, y: i32, w: i32, color: Color, label: &str, align: Align) {
    if w <= 0 {
        return;
    }
    let body = Style::on(INK, color);
    if w < 4 {
        canvas.fill(crate::canvas::Rect::new(x, y, w, 1), ' ', body);
    } else {
        canvas.put(x, y, '◖', Style::fg(color));
        canvas.fill(crate::canvas::Rect::new(x + 1, y, w - 2, 1), ' ', body);
        canvas.put(x + w - 1, y, '◗', Style::fg(color));
    }
    let inner = (w - 4).max(0);
    let text = crate::canvas::ellipsize(label, inner);
    let tw = crate::canvas::str_width(&text);
    let tx = match align {
        Align::Left => x + 2,
        Align::Right => x + w - 2 - tw,
        Align::Center => x + (w - tw) / 2,
    };
    canvas.label(tx, y, &text, INK, true);
}

/// Cut a label into existing colored cells, right-aligned so it ends at
/// column `right` (exclusive).
pub fn cut_right(canvas: &mut Canvas, right: i32, y: i32, label: &str, bold: bool) {
    let w = crate::canvas::str_width(label);
    canvas.label(right - w, y, label, INK, bold);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elbow_has_rounded_outer_corner_and_filled_inner_fillet() {
        let mut f = Frame::new(20, 10);
        f.elbow_px(0, 0, 40, 20, 12, 4, Corner::TopLeft, (8, 4), (4, 2), Color::rgb(255, 153, 0));
        // Outer corner pixel is cut away.
        assert_eq!(f.grid.get(0, 0), 0);
        // Deep inside the sidebar and the bar are filled.
        assert_ne!(f.grid.get(2, 15), 0);
        assert_ne!(f.grid.get(30, 1), 0);
        // The fillet adds ink just outside the joint.
        assert_ne!(f.grid.get(12, 4), 0);
        // Content area stays empty.
        assert_eq!(f.grid.get(25, 12), 0);
    }

    #[test]
    fn orientations_mirror_each_other() {
        let mut a = Frame::new(10, 5);
        a.elbow_px(0, 0, 20, 10, 6, 3, Corner::TopLeft, (6, 3), (2, 1), Color::rgb(1, 1, 1));
        let mut b = Frame::new(10, 5);
        b.elbow_px(0, 0, 20, 10, 6, 3, Corner::BottomRight, (6, 3), (2, 1), Color::rgb(1, 1, 1));
        for y in 0..10 {
            for x in 0..20 {
                assert_eq!(a.grid.get(x, y), b.grid.get(19 - x, 9 - y));
            }
        }
    }

    #[test]
    fn pill_clips_long_labels() {
        let mut c = Canvas::new(12, 1);
        pill(&mut c, 0, 0, 12, Color::rgb(255, 153, 0), "A VERY LONG LABEL", Align::Left);
        let t = c.to_plain_text();
        assert!(t.starts_with("◖ A VERY …"), "{t}");
        assert!(t.trim_end().ends_with('◗'));
    }
}
