//! Diff renderer: emits only the cells that changed between two buffers.

use super::buffer::{Buffer, Style, WIDE_TAIL};
use super::color::{Color, ColorMode};
use std::fmt::Write;

/// Append the ANSI stream that transforms the terminal showing `prev` into
/// `next`. With `full` set, every cell is written (first frame, resize).
/// Returns the number of cells written, which the tests use to prove that a
/// one-cell change produces a one-cell update.
pub fn diff(prev: Option<&Buffer>, next: &Buffer, mode: ColorMode, full: bool, out: &mut String) -> usize {
    let (w, h) = (next.width(), next.height());
    let mut written = 0usize;
    let mut cur_style: Option<Style> = None;
    let mut cursor: Option<(usize, usize)> = None;
    let mut x = 0usize;
    let mut y = 0usize;
    // SGR reset first so a stale attribute from a previous frame cannot leak.
    out.push_str("\x1b[0m");
    while y < h {
        let cell = match next.get(x, y) {
            Some(c) => c,
            None => break,
        };
        if cell.ch == WIDE_TAIL {
            x += 1;
            if x >= w {
                x = 0;
                y += 1;
            }
            continue;
        }
        let same = !full
            && prev
                .and_then(|p| p.get(x, y))
                .map(|p| p == cell && !tail_changed(prev, next, x, y))
                .unwrap_or(false);
        if !same {
            if cursor != Some((x, y)) {
                let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
            }
            if cur_style != Some(cell.style) {
                write_style(mode, cell.style, out);
                cur_style = Some(cell.style);
            }
            out.push(cell.ch);
            written += 1;
            let wide = next.get(x + 1, y).map(|c| c.ch == WIDE_TAIL).unwrap_or(false);
            let adv = if wide { 2 } else { 1 };
            // After writing in the last column, terminals differ on where the
            // cursor lands, so force an explicit move on the next write.
            cursor = if x + adv >= w { None } else { Some((x + adv, y)) };
        }
        x += 1;
        if x >= w {
            x = 0;
            y += 1;
        }
    }
    out.push_str("\x1b[0m");
    written
}

fn tail_changed(prev: Option<&Buffer>, next: &Buffer, x: usize, y: usize) -> bool {
    let n = next.get(x + 1, y).map(|c| c.ch == WIDE_TAIL).unwrap_or(false);
    let p = prev
        .and_then(|p| p.get(x + 1, y))
        .map(|c| c.ch == WIDE_TAIL)
        .unwrap_or(false);
    n != p
}

fn write_style(mode: ColorMode, st: Style, out: &mut String) {
    out.push_str("\x1b[0");
    if mode == ColorMode::Mono {
        // Monochrome keeps structure: coloured backgrounds become reverse
        // video so labels on rails stay legible, foreground colours vanish.
        if st.bg != Color::Default {
            out.push_str(";7");
        }
        if st.bold {
            out.push_str(";1");
        }
        out.push('m');
        return;
    }
    if st.bold {
        out.push_str(";1");
    }
    out.push(';');
    mode.write_fg(st.fg, out);
    out.push(';');
    mode.write_bg(st.bg, out);
    out.push('m');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_cell_change_writes_one_cell() {
        let mut a = Buffer::new(20, 5);
        a.put_str(0, 0, "hello", Style::default());
        let mut b = a.clone();
        b.set(2, 0, 'L', Style::default());
        let mut out = String::new();
        let n = diff(Some(&a), &b, ColorMode::TrueColor, false, &mut out);
        assert_eq!(n, 1);
        assert!(out.contains("\x1b[1;3H"), "{out:?}");
        assert!(out.ends_with("L\x1b[0m"), "{out:?}");
    }

    #[test]
    fn full_frame_writes_every_cell() {
        let b = Buffer::new(7, 3);
        let mut out = String::new();
        assert_eq!(diff(None, &b, ColorMode::Ansi256, true, &mut out), 21);
    }

    #[test]
    fn mono_mode_emits_no_colour_sequences() {
        let mut b = Buffer::new(4, 1);
        b.put_str(0, 0, "ab", Style::label(Color::ORANGE));
        let mut out = String::new();
        diff(None, &b, ColorMode::Mono, true, &mut out);
        assert!(!out.contains("38;"));
        assert!(!out.contains("48;"));
        assert!(out.contains("\x1b[0;7;1m"));
        let mut tc = String::new();
        diff(None, &b, ColorMode::TrueColor, true, &mut tc);
        assert!(tc.contains("48;2;255;153;0"));
        let mut c256 = String::new();
        diff(None, &b, ColorMode::Ansi256, true, &mut c256);
        assert!(c256.contains("48;5;208"));
    }

    #[test]
    fn wide_chars_are_written_once() {
        let mut b = Buffer::new(4, 1);
        b.put_str(0, 0, "日x", Style::default());
        let mut out = String::new();
        let n = diff(None, &b, ColorMode::TrueColor, true, &mut out);
        assert_eq!(n, 3); // 日, x, trailing space
        assert!(out.contains("日x"));
    }
}
