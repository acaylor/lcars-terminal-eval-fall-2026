use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate};
use std::io::{self, Write};
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Background,
    Text,
    Dim,
    Orange,
    Amber,
    Salmon,
    Lavender,
    Blue,
    Mint,
    Ink,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Ansi,
    Mono,
}

impl ColorMode {
    pub fn detect(mono: bool) -> Self {
        if mono || std::env::var_os("NO_COLOR").is_some() {
            return Self::Mono;
        }
        let color_term = std::env::var("COLORTERM")
            .unwrap_or_default()
            .to_lowercase();
        let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
        if color_term.contains("truecolor")
            || color_term.contains("24bit")
            || term.contains("truecolor")
            || term.contains("direct")
        {
            Self::TrueColor
        } else {
            Self::Ansi
        }
    }

    pub fn color(self, tone: Tone) -> Color {
        if self == Self::Mono {
            return match tone {
                Tone::Background => Color::Black,
                Tone::Ink => Color::Black,
                Tone::Dim => Color::DarkGrey,
                _ => Color::White,
            };
        }
        if self == Self::Ansi {
            return match tone {
                Tone::Background | Tone::Ink => Color::Black,
                Tone::Text => Color::White,
                Tone::Dim => Color::DarkGrey,
                Tone::Orange | Tone::Amber => Color::Yellow,
                Tone::Salmon => Color::Red,
                Tone::Lavender => Color::Magenta,
                Tone::Blue => Color::Cyan,
                Tone::Mint => Color::Green,
            };
        }
        match tone {
            Tone::Background => Color::Rgb {
                r: 14,
                g: 12,
                b: 22,
            },
            Tone::Text => Color::Rgb {
                r: 244,
                g: 237,
                b: 222,
            },
            Tone::Dim => Color::Rgb {
                r: 119,
                g: 109,
                b: 132,
            },
            Tone::Orange => Color::Rgb {
                r: 255,
                g: 153,
                b: 72,
            },
            Tone::Amber => Color::Rgb {
                r: 255,
                g: 198,
                b: 92,
            },
            Tone::Salmon => Color::Rgb {
                r: 255,
                g: 119,
                b: 111,
            },
            Tone::Lavender => Color::Rgb {
                r: 177,
                g: 157,
                b: 224,
            },
            Tone::Blue => Color::Rgb {
                r: 104,
                g: 178,
                b: 226,
            },
            Tone::Mint => Color::Rgb {
                r: 124,
                g: 220,
                b: 181,
            },
            Tone::Ink => Color::Rgb {
                r: 25,
                g: 19,
                b: 30,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub glyph: char,
    pub fg: Tone,
    pub bg: Tone,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            glyph: ' ',
            fg: Tone::Text,
            bg: Tone::Background,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffer {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
}

impl Buffer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); usize::from(width) * usize::from(height)],
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn get(&self, x: u16, y: u16) -> Option<Cell> {
        (x < self.width && y < self.height).then(|| self.cells[self.index(x, y)])
    }

    pub fn set(&mut self, x: u16, y: u16, glyph: char, fg: Tone, bg: Tone) {
        if x >= self.width || y >= self.height {
            return;
        }
        let index = self.index(x, y);
        self.cells[index] = Cell { glyph, fg, bg };
    }

    pub fn fill(&mut self, x: u16, y: u16, width: u16, height: u16, tone: Tone) {
        let x_end = x.saturating_add(width).min(self.width);
        let y_end = y.saturating_add(height).min(self.height);
        for row in y..y_end {
            for column in x..x_end {
                self.set(column, row, ' ', Tone::Ink, tone);
            }
        }
    }

    /// Writes clipped, terminal-width-aware text. Wide glyphs reserve a
    /// continuation cell, preventing a long non-ASCII name from shifting a row.
    pub fn text(&mut self, x: u16, y: u16, text: &str, fg: Tone, bg: Tone) -> u16 {
        if y >= self.height || x >= self.width {
            return 0;
        }
        let mut column = x;
        for character in text.chars() {
            if character.is_control() {
                continue;
            }
            let width = UnicodeWidthChar::width(character).unwrap_or(0);
            if width == 0 {
                continue;
            }
            if column.saturating_add(width as u16) > self.width {
                break;
            }
            self.set(column, y, character, fg, bg);
            if width == 2 {
                self.set(column + 1, y, '\0', fg, bg);
            }
            column += width as u16;
        }
        column - x
    }

    pub fn text_right(&mut self, right: u16, y: u16, text: &str, fg: Tone, bg: Tone) {
        let width = text
            .chars()
            .filter_map(UnicodeWidthChar::width)
            .sum::<usize>()
            .min(u16::MAX as usize) as u16;
        self.text(right.saturating_sub(width), y, text, fg, bg);
    }

    pub fn plain_text(&self) -> String {
        let mut output = String::new();
        for y in 0..self.height {
            let mut row = String::new();
            for x in 0..self.width {
                let glyph = self.cells[self.index(x, y)].glyph;
                if glyph != '\0' {
                    row.push(glyph);
                }
            }
            output.push_str(row.trim_end());
            if y + 1 < self.height {
                output.push('\n');
            }
        }
        output
    }

    pub fn count_background(&self, tone: Tone) -> usize {
        self.cells.iter().filter(|cell| cell.bg == tone).count()
    }

    pub fn make_ascii_safe(&mut self) {
        for cell in &mut self.cells {
            if !cell.glyph.is_ascii() && cell.glyph != '\0' {
                cell.glyph = '?';
            }
        }
    }

    fn index(&self, x: u16, y: u16) -> usize {
        usize::from(y) * usize::from(self.width) + usize::from(x)
    }
}

/// Diffing is performed against complete styled cells. Resizes invalidate the
/// previous frame and clear once; regular updates never clear the screen.
pub struct DiffRenderer {
    previous: Option<Buffer>,
    color_mode: ColorMode,
}

impl DiffRenderer {
    pub fn new(color_mode: ColorMode) -> Self {
        Self {
            previous: None,
            color_mode,
        }
    }

    pub fn invalidate(&mut self) {
        self.previous = None;
    }

    pub fn draw<W: Write>(&mut self, writer: &mut W, next: &Buffer) -> io::Result<()> {
        let resized = self
            .previous
            .as_ref()
            .is_none_or(|old| old.width != next.width || old.height != next.height);
        queue!(writer, BeginSynchronizedUpdate)?;
        if resized {
            queue!(writer, Clear(ClearType::All))?;
        }
        let empty = Buffer::new(0, 0);
        let old = self.previous.as_ref().unwrap_or(&empty);
        for y in 0..next.height {
            let mut x = 0;
            while x < next.width {
                let cell = next.get(x, y).expect("coordinates are in bounds");
                if !resized && old.get(x, y) == Some(cell) {
                    x += 1;
                    continue;
                }
                let start = x;
                let style = (cell.fg, cell.bg);
                let mut text = String::new();
                while x < next.width {
                    let current = next.get(x, y).expect("coordinates are in bounds");
                    if (current.fg, current.bg) != style
                        || (!resized && old.get(x, y) == Some(current))
                    {
                        break;
                    }
                    if current.glyph != '\0' {
                        text.push(current.glyph);
                    }
                    x += 1;
                }
                queue!(
                    writer,
                    MoveTo(start, y),
                    SetForegroundColor(self.color_mode.color(style.0)),
                    SetBackgroundColor(self.color_mode.color(style.1)),
                    Print(text)
                )?;
            }
        }
        queue!(writer, ResetColor, EndSynchronizedUpdate)?;
        writer.flush()?;
        self.previous = Some(next.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_unicode_is_clipped_without_losing_grid_alignment() {
        let mut buffer = Buffer::new(5, 1);
        buffer.text(0, 0, "A界B", Tone::Text, Tone::Background);
        assert_eq!(buffer.get(1, 0).unwrap().glyph, '界');
        assert_eq!(buffer.get(2, 0).unwrap().glyph, '\0');
        assert_eq!(buffer.get(3, 0).unwrap().glyph, 'B');
        assert_eq!(buffer.plain_text(), "A界B");
        buffer.text(4, 0, "界", Tone::Text, Tone::Background);
        assert_eq!(buffer.get(4, 0).unwrap().glyph, ' ');
    }

    #[test]
    fn tiny_buffers_never_panic() {
        for width in 0..4 {
            for height in 0..4 {
                let mut buffer = Buffer::new(width, height);
                buffer.fill(0, 0, 10, 10, Tone::Orange);
                buffer.text(3, 2, "out of bounds", Tone::Text, Tone::Background);
                buffer.text_right(2, 1, "wide 界", Tone::Text, Tone::Background);
            }
        }
    }
}
