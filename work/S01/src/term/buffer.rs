//! A terminal-independent grid of styled cells. Views draw into this; tests
//! inspect it; the renderer diffs two of them.

use super::color::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
}

impl Style {
    pub const fn fg(c: Color) -> Style {
        Style {
            fg: c,
            bg: Color::Default,
            bold: false,
        }
    }
    /// Black label text on a coloured rail segment.
    pub const fn label(bg: Color) -> Style {
        Style {
            fg: Color::BLACK,
            bg,
            bold: true,
        }
    }
    pub const fn bold(mut self) -> Style {
        self.bold = true;
        self
    }
}

/// `ch == WIDE_TAIL` marks the second column of a double-width character.
pub const WIDE_TAIL: char = '\0';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            style: Style::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Buffer {
    w: usize,
    h: usize,
    cells: Vec<Cell>,
}

impl Buffer {
    pub fn new(w: usize, h: usize) -> Buffer {
        Buffer {
            w,
            h,
            cells: vec![Cell::default(); w * h],
        }
    }

    pub fn width(&self) -> usize {
        self.w
    }
    pub fn height(&self) -> usize {
        self.h
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&Cell> {
        if x < self.w && y < self.h {
            self.cells.get(y * self.w + x)
        } else {
            None
        }
    }

    /// Set one cell. Out-of-range coordinates are silently clipped, which is
    /// what lets every view survive absurd terminal sizes without checks.
    pub fn set(&mut self, x: i32, y: i32, ch: char, style: Style) {
        if x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        if x >= self.w || y >= self.h {
            return;
        }
        let idx = y * self.w + x;
        // Overwriting half of a wide character must not leave a dangling tail.
        if self.cells[idx].ch == WIDE_TAIL && x > 0 {
            self.cells[idx - 1].ch = ' ';
        }
        if x + 1 < self.w && self.cells[idx + 1].ch == WIDE_TAIL {
            self.cells[idx + 1].ch = ' ';
        }
        self.cells[idx] = Cell { ch, style };
    }

    /// Write a string, clipping at the buffer edge. Returns the x after the
    /// last written column. Wide characters occupy two columns.
    pub fn put_str(&mut self, x: i32, y: i32, text: &str, style: Style) -> i32 {
        let mut cx = x;
        for ch in text.chars() {
            let w = char_width(ch);
            if w == 0 {
                continue;
            }
            if w == 2 {
                if cx + 1 >= self.w as i32 {
                    break;
                }
                self.set(cx, y, ch, style);
                self.set(cx + 1, y, WIDE_TAIL, style);
            } else {
                self.set(cx, y, ch, style);
            }
            cx += w as i32;
        }
        cx
    }

    /// Right-aligned write ending at column `x_end` (exclusive).
    pub fn put_str_right(&mut self, x_end: i32, y: i32, text: &str, style: Style) -> i32 {
        let w = str_width(text) as i32;
        self.put_str(x_end - w, y, text, style)
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, ch: char, style: Style) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, ch, style);
            }
        }
    }

    /// Plain-text rows (no styling). Wide tails are dropped so the text lines
    /// up with what a terminal would show.
    pub fn rows_text(&self) -> Vec<String> {
        (0..self.h)
            .map(|y| {
                let row: String = self.cells[y * self.w..(y + 1) * self.w]
                    .iter()
                    .filter(|c| c.ch != WIDE_TAIL)
                    .map(|c| c.ch)
                    .collect();
                row.trim_end().to_string()
            })
            .collect()
    }

    pub fn contains_text(&self, needle: &str) -> bool {
        self.rows_text().iter().any(|r| r.contains(needle))
    }
}

/// Display width of a character in terminal columns. Covers the East Asian
/// wide ranges that matter for process names; everything else is 1 column.
pub fn char_width(c: char) -> usize {
    let cp = c as u32;
    if cp < 0x20 || (0x7F..0xA0).contains(&cp) {
        return 0;
    }
    // Combining marks.
    if (0x0300..=0x036F).contains(&cp) || (0x200B..=0x200F).contains(&cp) || cp == 0xFE0F {
        return 0;
    }
    let wide = (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0x303E).contains(&cp)
        || (0x3041..=0x33FF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x4E00..=0x9FFF).contains(&cp)
        || (0xA000..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7A3).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE4F).contains(&cp)
        || (0xFF00..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp)
        || (0x1F300..=0x1F64F).contains(&cp)
        || (0x1F900..=0x1F9FF).contains(&cp)
        || (0x20000..=0x3FFFD).contains(&cp);
    if wide {
        2
    } else {
        1
    }
}

pub fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// Truncate to at most `max` columns, appending `ellipsis` when cut.
pub fn fit(s: &str, max: usize, ellipsis: &str) -> String {
    if str_width(s) <= max {
        return s.to_string();
    }
    let ew = str_width(ellipsis);
    if max <= ew {
        return s.chars().take(max).collect();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = char_width(ch);
        if w + cw > max - ew {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push_str(ellipsis);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_never_panics() {
        let mut b = Buffer::new(5, 2);
        b.set(-1, 0, 'x', Style::default());
        b.set(99, 99, 'x', Style::default());
        b.put_str(3, 1, "hello", Style::default());
        b.fill(-5, -5, 100, 100, '#', Style::default());
        assert_eq!(b.rows_text(), vec!["#####", "#####"]);
        let zero = Buffer::new(0, 0);
        assert!(zero.rows_text().is_empty());
    }

    #[test]
    fn wide_chars_take_two_columns() {
        let mut b = Buffer::new(6, 1);
        let next = b.put_str(0, 0, "日本x", Style::default());
        assert_eq!(next, 5);
        assert_eq!(b.get(1, 0).unwrap().ch, WIDE_TAIL);
        assert_eq!(b.rows_text(), vec!["日本x"]);
        // Overwriting the tail clears the head.
        b.set(1, 0, 'y', Style::default());
        assert_eq!(b.get(0, 0).unwrap().ch, ' ');
    }

    #[test]
    fn fit_truncates_with_ellipsis() {
        assert_eq!(fit("firefox-bin", 8, "…"), "firefox…");
        assert_eq!(fit("short", 8, "…"), "short");
        assert_eq!(fit("日本語テキスト", 7, "…"), "日本語…");
        assert_eq!(fit("abc", 1, "…"), "a");
    }

    #[test]
    fn right_align() {
        let mut b = Buffer::new(10, 1);
        b.put_str_right(10, 0, "end", Style::default());
        assert_eq!(b.rows_text(), vec!["       end"]);
    }
}
