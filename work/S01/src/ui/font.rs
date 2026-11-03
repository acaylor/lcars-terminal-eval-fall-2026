//! Bitmap fonts rendered with half-block glyphs. Each glyph is defined on a
//! grid with twice the vertical resolution of a terminal row; pairs of rows
//! collapse into `▀`, `▄`, `█` or space. The ASCII fallback renders any set
//! pixel in a row pair as `#`.

use crate::term::{Buffer, Glyphs, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    /// 5 columns x 5 rows, used by the clock.
    Big,
    /// 3 columns x 3 rows, used by the CPU readout.
    Medium,
}

impl Font {
    pub fn rows(self) -> i32 {
        match self {
            Font::Big => 5,
            Font::Medium => 3,
        }
    }
    fn glyph(self, c: char) -> Option<(&'static [&'static str], i32)> {
        match self {
            Font::Big => big(c),
            Font::Medium => medium(c),
        }
    }
    /// Advance width of a glyph including the one-column gap after it.
    fn advance(self, c: char) -> i32 {
        self.glyph(c)
            .map(|(_, w)| w + 1)
            .unwrap_or(if self == Font::Big { 3 } else { 2 })
    }
    pub fn width(self, text: &str) -> i32 {
        let w: i32 = text.chars().map(|c| self.advance(c)).sum();
        (w - 1).max(0)
    }
}

/// Draw `text` with its top-left at (x, y). Returns the x after the text.
pub fn draw(buf: &mut Buffer, g: &Glyphs, x: i32, y: i32, text: &str, font: Font, style: Style) -> i32 {
    let mut cx = x;
    for c in text.chars() {
        let Some((rows, w)) = font.glyph(c) else {
            cx += font.advance(c);
            continue;
        };
        for r in 0..font.rows() {
            let top = rows[(r * 2) as usize].as_bytes();
            let bot = rows[(r * 2 + 1) as usize].as_bytes();
            for col in 0..w {
                let t = top.get(col as usize).map(|&b| b == b'X').unwrap_or(false);
                let b = bot.get(col as usize).map(|&b| b == b'X').unwrap_or(false);
                let ch = if g.unicode {
                    match (t, b) {
                        (true, true) => g.full,
                        (true, false) => g.upper,
                        (false, true) => g.lower,
                        (false, false) => continue,
                    }
                } else if t || b {
                    g.full
                } else {
                    continue;
                };
                buf.set(cx + col, y + r, ch, style);
            }
        }
        cx += w + 1;
    }
    cx - 1
}

fn big(c: char) -> Option<(&'static [&'static str], i32)> {
    let rows: &'static [&'static str] = match c {
        '0' => &[
            ".XXX.", "XX.XX", "X...X", "X...X", "X...X", "X...X", "X...X", "X...X", "XX.XX", ".XXX.",
        ],
        '1' => &[
            "..X..", ".XX..", "X.X..", "..X..", "..X..", "..X..", "..X..", "..X..", "..X..", "XXXXX",
        ],
        '2' => &[
            ".XXX.", "X...X", "....X", "....X", "...X.", "..X..", ".X...", "X....", "X....", "XXXXX",
        ],
        '3' => &[
            ".XXX.", "X...X", "....X", "....X", "..XX.", "....X", "....X", "....X", "X...X", ".XXX.",
        ],
        '4' => &[
            "...X.", "..XX.", ".X.X.", ".X.X.", "X..X.", "X..X.", "XXXXX", "...X.", "...X.", "...X.",
        ],
        '5' => &[
            "XXXXX", "X....", "X....", "X....", "XXXX.", "....X", "....X", "....X", "X...X", ".XXX.",
        ],
        '6' => &[
            ".XXX.", "X...X", "X....", "X....", "XXXX.", "X...X", "X...X", "X...X", "X...X", ".XXX.",
        ],
        '7' => &[
            "XXXXX", "....X", "....X", "...X.", "...X.", "..X..", "..X..", ".X...", ".X...", ".X...",
        ],
        '8' => &[
            ".XXX.", "X...X", "X...X", "X...X", ".XXX.", "X...X", "X...X", "X...X", "X...X", ".XXX.",
        ],
        '9' => &[
            ".XXX.", "X...X", "X...X", "X...X", "X...X", ".XXXX", "....X", "....X", "X...X", ".XXX.",
        ],
        ':' => return Some((&[".", ".", "X", "X", ".", ".", "X", "X", ".", "."], 1)),
        ' ' => return Some((&["", "", "", "", "", "", "", "", "", ""], 2)),
        _ => return None,
    };
    Some((rows, 5))
}

fn medium(c: char) -> Option<(&'static [&'static str], i32)> {
    let rows: &'static [&'static str] = match c {
        '0' => &["XXX", "X.X", "X.X", "X.X", "X.X", "XXX"],
        '1' => &[".X.", "XX.", ".X.", ".X.", ".X.", "XXX"],
        '2' => &["XXX", "..X", "..X", "XXX", "X..", "XXX"],
        '3' => &["XXX", "..X", "XXX", "..X", "..X", "XXX"],
        '4' => &["X.X", "X.X", "X.X", "XXX", "..X", "..X"],
        '5' => &["XXX", "X..", "XXX", "..X", "..X", "XXX"],
        '6' => &["XXX", "X..", "XXX", "X.X", "X.X", "XXX"],
        '7' => &["XXX", "..X", "..X", "..X", "..X", "..X"],
        '8' => &["XXX", "X.X", "XXX", "X.X", "X.X", "XXX"],
        '9' => &["XXX", "X.X", "XXX", "..X", "..X", "XXX"],
        '%' => &["X.X", "..X", ".X.", ".X.", "X..", "X.X"],
        '-' => &["...", "...", "XXX", "XXX", "...", "..."],
        '.' => return Some((&[".", ".", ".", ".", "X", "X"], 1)),
        ' ' => return Some((&["", "", "", "", "", ""], 1)),
        _ => return None,
    };
    Some((rows, 3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_has_the_right_dimensions() {
        for c in "0123456789: ".chars() {
            let (rows, w) = big(c).unwrap();
            assert_eq!(rows.len(), 10, "{c}");
            assert!(rows.iter().all(|r| r.len() as i32 <= w), "{c}");
        }
        for c in "0123456789%-. ".chars() {
            let (rows, w) = medium(c).unwrap();
            assert_eq!(rows.len(), 6, "{c}");
            assert!(rows.iter().all(|r| r.len() as i32 <= w), "{c}");
        }
    }

    #[test]
    fn clock_width_is_stable() {
        assert_eq!(Font::Big.width("23:59:07"), 6 * 6 + 2 * 2 - 1);
        assert_eq!(Font::Medium.width("100%"), 15);
        assert_eq!(Font::Medium.width("7%"), 7);
    }

    #[test]
    fn renders_with_half_blocks_and_ascii() {
        let mut b = Buffer::new(8, 5);
        draw(&mut b, &Glyphs::unicode(), 0, 0, "1", Font::Big, Style::default());
        let rows = b.rows_text();
        assert_eq!(rows[0], " ▄█");
        assert_eq!(rows[4], "▄▄█▄▄");
        let mut a = Buffer::new(8, 5);
        draw(&mut a, &Glyphs::ascii(), 0, 0, "1", Font::Big, Style::default());
        assert!(a.rows_text().iter().all(|r| r.is_ascii()));
        assert_eq!(a.rows_text()[4], "#####");
    }
}
