//! Sub-cell rasters. Shapes (frame elbows, digits, gauges) are drawn into a
//! pixel grid and then converted to Unicode quadrant (2x2 per cell) or
//! Braille (2x4 per cell) glyphs. Each pixel stores a small "slot" number so a
//! raster can hold several colors; 0 means empty.

use crate::canvas::{Canvas, Cell, Style};
use crate::color::Color;

#[derive(Clone, Debug)]
pub struct PixelGrid {
    w: i32,
    h: i32,
    px: Vec<u8>,
}

impl PixelGrid {
    pub fn new(w: i32, h: i32) -> PixelGrid {
        let (w, h) = (w.max(0), h.max(0));
        PixelGrid { w, h, px: vec![0; (w * h) as usize] }
    }
    pub fn width(&self) -> i32 {
        self.w
    }
    pub fn height(&self) -> i32 {
        self.h
    }
    pub fn get(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            0
        } else {
            self.px[(y * self.w + x) as usize]
        }
    }
    pub fn set(&mut self, x: i32, y: i32, slot: u8) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize] = slot;
        }
    }
    /// Fill the half-open pixel rectangle [x0,x1) x [y0,y1).
    pub fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, slot: u8) {
        for y in y0.max(0)..y1.min(self.h) {
            for x in x0.max(0)..x1.min(self.w) {
                self.px[(y * self.w + x) as usize] = slot;
            }
        }
    }
    pub fn count(&self, slot: u8) -> usize {
        self.px.iter().filter(|&&p| p == slot).count()
    }
}

const QUADRANTS: [char; 16] = [' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'];

/// Pick the dominant slot among a cell's pixels, and optionally a second slot
/// that fills the remaining pixels exactly (so it can become the background).
fn dominant(vals: &[u8]) -> Option<(u8, Option<u8>)> {
    let mut slots: [(u8, u8); 8] = [(0, 0); 8];
    let mut n = 0;
    let mut empties = 0;
    for &v in vals {
        if v == 0 {
            empties += 1;
            continue;
        }
        if let Some(s) = slots[..n].iter_mut().find(|s| s.0 == v) {
            s.1 += 1;
        } else if n < slots.len() {
            slots[n] = (v, 1);
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let mut best = 0;
    for i in 1..n {
        if slots[i].1 > slots[best].1 {
            best = i;
        }
    }
    let second = if n == 2 && empties == 0 { Some(slots[1 - best].0) } else { None };
    Some((slots[best].0, second))
}

/// Blit a quadrant raster with its top-left pixel at cell (cx, cy).
/// `color(slot, cell_x, cell_y)` lets callers apply gradients per cell.
pub fn blit_quadrants(canvas: &mut Canvas, grid: &PixelGrid, cx: i32, cy: i32, color: impl Fn(u8, i32, i32) -> Color) {
    let cols = (grid.w + 1) / 2;
    let rows = (grid.h + 1) / 2;
    for r in 0..rows {
        for c in 0..cols {
            let (x, y) = (cx + c, cy + r);
            if canvas.get(x, y).is_none() {
                continue;
            }
            let vals = [grid.get(2 * c, 2 * r), grid.get(2 * c + 1, 2 * r), grid.get(2 * c, 2 * r + 1), grid.get(2 * c + 1, 2 * r + 1)];
            let Some((slot, second)) = dominant(&vals) else { continue };
            let mut mask = 0usize;
            for (bit, v) in vals.iter().enumerate() {
                if *v == slot {
                    mask |= 1 << bit;
                }
            }
            let fg = color(slot, x, y);
            let existing = canvas.get(x, y).copied().unwrap_or(Cell::BLANK);
            if mask == 15 {
                // Solid cell: paint as background so labels can sit on it.
                canvas.put(x, y, ' ', Style::on(fg, fg));
            } else {
                let bg = match second {
                    Some(s) => color(s, x, y),
                    None => existing.style.bg,
                };
                canvas.put(x, y, QUADRANTS[mask], Style::on(fg, bg));
            }
        }
    }
}

/// A 2x4-dots-per-cell raster for fine traces.
#[derive(Clone, Debug)]
pub struct BrailleGrid {
    inner: PixelGrid,
}

impl BrailleGrid {
    pub fn new(cols: i32, rows: i32) -> BrailleGrid {
        BrailleGrid { inner: PixelGrid::new(cols.max(0) * 2, rows.max(0) * 4) }
    }
    pub fn width(&self) -> i32 {
        self.inner.w
    }
    pub fn height(&self) -> i32 {
        self.inner.h
    }
    pub fn set(&mut self, x: i32, y: i32, slot: u8) {
        self.inner.set(x, y, slot)
    }
    pub fn get(&self, x: i32, y: i32) -> u8 {
        self.inner.get(x, y)
    }
    /// Vertical run of dots from y0 to y1 inclusive (either order).
    pub fn vline(&mut self, x: i32, y0: i32, y1: i32, slot: u8) {
        let (a, b) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        for y in a..=b {
            self.set(x, y, slot);
        }
    }
}

const BRAILLE_BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

pub fn blit_braille(canvas: &mut Canvas, grid: &BrailleGrid, cx: i32, cy: i32, color: impl Fn(u8, i32, i32) -> Color) {
    let cols = grid.width() / 2;
    let rows = grid.height() / 4;
    for r in 0..rows {
        for c in 0..cols {
            let (x, y) = (cx + c, cy + r);
            if canvas.get(x, y).is_none() {
                continue;
            }
            let mut vals = [0u8; 8];
            for dx in 0..2 {
                for dy in 0..4 {
                    vals[(dx * 4 + dy) as usize] = grid.get(2 * c + dx, 4 * r + dy);
                }
            }
            // Dots are too small to split colors meaningfully; the lowest
            // slot number wins, so callers give their primary line slot 1.
            let Some(slot) = vals.iter().copied().filter(|&v| v != 0).min() else { continue };
            let mut bits = 0u32;
            for dx in 0..2 {
                for dy in 0..4 {
                    if vals[(dx * 4 + dy) as usize] != 0 {
                        bits |= BRAILLE_BITS[dx as usize][dy as usize];
                    }
                }
            }
            let ch = char::from_u32(0x2800 + bits).unwrap_or(' ');
            let bg = canvas.get(x, y).map(|c| c.style.bg).unwrap_or(Color::Reset);
            canvas.put(x, y, ch, Style::on(color(slot, x, y), bg));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadrant_masks_map_to_expected_glyphs() {
        let mut g = PixelGrid::new(4, 2);
        g.set(0, 0, 1); // TL of cell 0
        g.set(3, 1, 1); // BR of cell 1
        let mut c = Canvas::new(2, 1);
        blit_quadrants(&mut c, &g, 0, 0, |_, _, _| Color::rgb(255, 0, 0));
        assert_eq!(c.get(0, 0).unwrap().ch, '▘');
        assert_eq!(c.get(1, 0).unwrap().ch, '▗');
    }

    #[test]
    fn solid_cells_become_background_and_two_tone_cells_split() {
        let mut g = PixelGrid::new(4, 2);
        g.fill_rect(0, 0, 2, 2, 1);
        g.fill_rect(2, 0, 4, 1, 1);
        g.fill_rect(2, 1, 4, 2, 2);
        let mut c = Canvas::new(2, 1);
        let red = Color::rgb(255, 0, 0);
        let blue = Color::rgb(0, 0, 255);
        blit_quadrants(&mut c, &g, 0, 0, |s, _, _| if s == 1 { red } else { blue });
        assert_eq!(*c.get(0, 0).unwrap(), Cell { ch: ' ', style: Style::on(red, red) });
        let two = c.get(1, 0).unwrap();
        assert_eq!(two.ch, '▀');
        assert_eq!(two.style.fg, red);
        assert_eq!(two.style.bg, blue);
    }

    #[test]
    fn braille_dot_order() {
        let mut g = BrailleGrid::new(1, 1);
        g.set(0, 0, 1);
        g.set(1, 3, 1);
        let mut c = Canvas::new(1, 1);
        blit_braille(&mut c, &g, 0, 0, |_, _, _| Color::rgb(1, 2, 3));
        assert_eq!(c.get(0, 0).unwrap().ch, '\u{2881}');
    }

    #[test]
    fn blitting_off_canvas_is_safe() {
        let mut g = PixelGrid::new(10, 10);
        g.fill_rect(0, 0, 10, 10, 1);
        let mut c = Canvas::new(3, 3);
        blit_quadrants(&mut c, &g, -2, 1, |_, _, _| Color::rgb(9, 9, 9));
        blit_braille(&mut c, &BrailleGrid::new(5, 5), 2, 2, |_, _, _| Color::Reset);
    }
}
