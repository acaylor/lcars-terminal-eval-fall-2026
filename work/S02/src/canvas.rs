//! An in-memory grid of styled cells. All drawing happens here; nothing in
//! this module touches the terminal, which is what makes rendering testable.

use crate::color::Color;
use unicode_width::UnicodeWidthChar;

/// Marks the right half of a double-width character.
pub const WIDE_CONT: char = '\0';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
}

impl Style {
    pub const fn fg(fg: Color) -> Style {
        Style { fg, bg: Color::Reset, bold: false }
    }
    pub const fn on(fg: Color, bg: Color) -> Style {
        Style { fg, bg, bold: false }
    }
    pub const fn bold(mut self) -> Style {
        self.bold = true;
        self
    }
}

impl Default for Style {
    fn default() -> Self {
        Style { fg: Color::Reset, bg: Color::Reset, bold: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

impl Cell {
    pub const BLANK: Cell = Cell { ch: ' ', style: Style { fg: Color::Reset, bg: Color::Reset, bold: false } };

    /// True when the cell shows nothing but screen background.
    pub fn is_blank(&self) -> bool {
        self.ch == ' ' && self.style.bg == Color::Reset
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
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
    pub fn intersects(&self, o: &Rect) -> bool {
        !self.is_empty() && !o.is_empty() && self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
    pub fn contains_rect(&self, o: &Rect) -> bool {
        o.x >= self.x && o.y >= self.y && o.right() <= self.right() && o.bottom() <= self.bottom()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canvas {
    w: u16,
    h: u16,
    cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(w: u16, h: u16) -> Canvas {
        Canvas { w, h, cells: vec![Cell::BLANK; w as usize * h as usize] }
    }

    pub fn width(&self) -> i32 {
        self.w as i32
    }
    pub fn height(&self) -> i32 {
        self.h as i32
    }
    pub fn size(&self) -> (u16, u16) {
        (self.w, self.h)
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::BLANK);
    }

    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            None
        } else {
            Some(y as usize * self.w as usize + x as usize)
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<&Cell> {
        self.idx(x, y).map(|i| &self.cells[i])
    }

    pub fn row(&self, y: i32) -> &[Cell] {
        let start = y as usize * self.w as usize;
        &self.cells[start..start + self.w as usize]
    }

    /// Place one narrow character. Overwriting half of a wide character
    /// blanks the other half so the terminal never sees an orphan.
    pub fn put(&mut self, x: i32, y: i32, ch: char, style: Style) {
        let Some(i) = self.idx(x, y) else { return };
        self.break_wide_at(x, y, i);
        self.cells[i] = Cell { ch, style };
    }

    fn break_wide_at(&mut self, x: i32, y: i32, i: usize) {
        if self.cells[i].ch == WIDE_CONT {
            if let Some(j) = self.idx(x - 1, y) {
                self.cells[j].ch = ' ';
            }
        } else if let Some(j) = self.idx(x + 1, y) {
            if self.cells[j].ch == WIDE_CONT {
                self.cells[j].ch = ' ';
            }
        }
    }

    /// Change only the style of a cell (used to paint shape colors under text).
    pub fn set_bg(&mut self, x: i32, y: i32, bg: Color) {
        if let Some(i) = self.idx(x, y) {
            self.cells[i].style.bg = bg;
        }
    }

    pub fn fill(&mut self, r: Rect, ch: char, style: Style) {
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                self.put(x, y, ch, style);
            }
        }
    }

    /// Draw text with an explicit style, clipped at `max_w` columns (and the
    /// canvas edge). Returns the number of columns used.
    pub fn text_max(&mut self, x: i32, y: i32, s: &str, style: Style, max_w: i32) -> i32 {
        let mut col = 0;
        for ch in s.chars() {
            let ch = sanitize(ch);
            let cw = ch.width().unwrap_or(0) as i32;
            if cw == 0 {
                continue;
            }
            if col + cw > max_w || x + col >= self.w as i32 {
                break;
            }
            if cw == 2 {
                if x + col + 1 >= self.w as i32 {
                    // Would straddle the right edge: show a narrow stand-in.
                    self.put(x + col, y, ' ', style);
                    col += 1;
                    break;
                } else {
                    self.put(x + col, y, ch, style);
                    self.put(x + col + 1, y, WIDE_CONT, style);
                }
            } else {
                self.put(x + col, y, ch, style);
            }
            col += cw;
        }
        col
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, style: Style) -> i32 {
        self.text_max(x, y, s, style, i32::MAX / 4)
    }

    /// Draw text in `fg`, keeping each cell's existing background. This is
    /// how labels are "cut into" colored frame shapes.
    pub fn label(&mut self, x: i32, y: i32, s: &str, fg: Color, bold: bool) -> i32 {
        let mut col = 0;
        for ch in s.chars() {
            let ch = sanitize(ch);
            let cw = ch.width().unwrap_or(0) as i32;
            if cw != 1 {
                continue; // labels are always our own narrow text
            }
            let bg = self.get(x + col, y).map(|c| c.style.bg).unwrap_or(Color::Reset);
            self.put(x + col, y, ch, Style { fg, bg, bold });
            col += 1;
        }
        col
    }

    /// Plain-text dump (one line per row) used by snapshots and tests.
    pub fn to_plain_text(&self) -> String {
        let mut out = String::with_capacity(self.cells.len() + self.h as usize);
        for y in 0..self.h as i32 {
            let mut line = String::new();
            for c in self.row(y) {
                if c.ch != WIDE_CONT {
                    line.push(c.ch);
                }
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    /// Fraction of cells that are pure background (negative space).
    pub fn blank_ratio(&self) -> f64 {
        if self.cells.is_empty() {
            return 1.0;
        }
        self.cells.iter().filter(|c| c.is_blank()).count() as f64 / self.cells.len() as f64
    }
}

/// Control characters would move the real cursor and corrupt the layout.
fn sanitize(ch: char) -> char {
    if ch.is_control() {
        '?'
    } else {
        ch
    }
}

/// Display width of a string in terminal columns (after sanitizing).
pub fn str_width(s: &str) -> i32 {
    s.chars().map(|c| sanitize(c).width().unwrap_or(0) as i32).sum()
}

/// Truncate to `max` columns, appending an ellipsis when something was cut.
pub fn ellipsize(s: &str, max: i32) -> String {
    if max <= 0 {
        return String::new();
    }
    if str_width(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = sanitize(ch).width().unwrap_or(0) as i32;
        if w + cw > max - 1 {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_characters_occupy_two_cells_and_clip_cleanly() {
        let mut c = Canvas::new(5, 1);
        let used = c.text(0, 0, "日本語", Style::default());
        assert_eq!(used, 5, "two wide chars plus a blank stand-in for the clipped third");
        assert_eq!(c.get(0, 0).unwrap().ch, '日');
        assert_eq!(c.get(1, 0).unwrap().ch, WIDE_CONT);
        // Overwrite the continuation half: the lead must be blanked.
        c.put(1, 0, 'x', Style::default());
        assert_eq!(c.get(0, 0).unwrap().ch, ' ');
        // Wide char at the right edge degrades to a blank, never overflows.
        let mut e = Canvas::new(3, 1);
        e.text(2, 0, "語", Style::default());
        assert_eq!(e.get(2, 0).unwrap().ch, ' ');
    }

    #[test]
    fn out_of_bounds_drawing_is_ignored() {
        let mut c = Canvas::new(2, 2);
        c.put(-1, 0, 'x', Style::default());
        c.put(5, 5, 'x', Style::default());
        c.text(-3, 1, "abcdef", Style::default());
        assert_eq!(c.to_plain_text(), "\nde\n");
    }

    #[test]
    fn ellipsize_respects_display_width() {
        assert_eq!(ellipsize("short", 10), "short");
        assert_eq!(ellipsize("a-very-long-name", 6), "a-ver…");
        assert_eq!(str_width(&ellipsize("日本語日本語", 5)), 5);
        assert_eq!(ellipsize("x", 0), "");
    }

    #[test]
    fn control_characters_are_neutralized() {
        let mut c = Canvas::new(4, 1);
        c.text(0, 0, "a\x1bb", Style::default());
        assert_eq!(c.to_plain_text(), "a?b\n");
    }
}
