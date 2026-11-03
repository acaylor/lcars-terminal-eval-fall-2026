//! Converts canvases into terminal escape sequences.
//!
//! Only cells that differ from the previous frame are emitted, the SGR state
//! is tracked to avoid redundant attribute changes, and each frame is wrapped
//! in DEC synchronized-output markers (mode 2026) so supporting terminals
//! present it atomically. Unsupported terminals ignore those markers.

use crate::canvas::{Canvas, Cell, Style, WIDE_CONT};
use crate::color::{to_ansi16, to_ansi256, Color, ColorMode};
use std::io::Write;
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderMode {
    pub color: ColorMode,
    pub ascii: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct Sgr {
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
    faint: bool,
    reverse: bool,
}

/// Map a Unicode drawing glyph to an ASCII-safe equivalent. Solid-looking
/// glyphs become colored spaces so shapes survive even without Unicode.
pub fn ascii_cell(cell: Cell) -> Cell {
    let Cell { ch, style } = cell;
    if ch.is_ascii() {
        return cell;
    }
    let solid = |on: bool| {
        let bg = if on { style.fg } else { style.bg };
        Cell { ch: ' ', style: Style { fg: style.fg, bg, bold: style.bold } }
    };
    let plain = |c: char| Cell { ch: c, style };
    match ch {
        '█' | '◖' | '◗' => solid(true),
        // Quadrants downsample to one cell by looking at the top half, so the
        // half-row gaps between frame blocks survive as full-row gaps
        // instead of blocks merging into one slab.
        '▀' | '▌' | '▐' | '▛' | '▜' | '▙' | '▟' | '▞' | '▚' => solid(true),
        '▄' | '▘' | '▝' | '▖' | '▗' => solid(false),
        '▁' | '▂' | '▃' => plain('_'),
        '▅' | '▆' | '▇' => solid(true),
        '▔' => plain('-'),
        '\u{2800}'..='\u{28FF}' => {
            let dots = (ch as u32 - 0x2800).count_ones();
            plain(match dots {
                0 => ' ',
                1 | 2 => '.',
                3 | 4 => ':',
                5 | 6 => '+',
                _ => '#',
            })
        }
        '─' | '━' | '╌' => plain('-'),
        '│' | '┃' | '╎' => plain('|'),
        '═' => plain('='),
        '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' | '╭' | '╮' | '╰' | '╯' => plain('+'),
        '●' | '◆' | '■' | '▪' => plain('*'),
        '○' | '◇' | '□' => plain('o'),
        '▲' | '↑' | '▴' => plain('^'),
        '▼' | '↓' | '▾' => plain('v'),
        '▶' | '▸' | '→' => plain('>'),
        '◀' | '◂' | '←' => plain('<'),
        '·' | '•' => plain('.'),
        '…' => plain('~'),
        '×' => plain('x'),
        '°' => plain('o'),
        WIDE_CONT => plain('?'),
        _ => plain('?'),
    }
}

fn is_block_glyph(ch: char) -> bool {
    matches!(ch, '\u{2580}'..='\u{259F}' | '◖' | '◗')
}

fn map_style(ch: char, style: Style, mode: ColorMode) -> (char, Sgr) {
    match mode {
        ColorMode::Mono => {
            // Without color, "a colored region" becomes reverse video and
            // dim foreground colors become faint, preserving the silhouette.
            // Dim tracks stay dark; only real frame colors become "on".
            let bg_on = style.bg != Color::Reset && style.bg.luma() > 0.3;
            if bg_on {
                let ch = if is_block_glyph(ch) { ' ' } else { ch };
                (ch, Sgr { reverse: true, bold: style.bold, ..Sgr::default() })
            } else {
                let faint = style.fg != Color::Reset && style.fg.luma() < 0.28;
                (ch, Sgr { bold: style.bold, faint, ..Sgr::default() })
            }
        }
        _ => {
            let bg = match style.bg {
                Color::Reset => Color::Rgb(0, 0, 0),
                c => c,
            };
            let fg = match style.fg {
                Color::Reset => None,
                c => Some(c),
            };
            (ch, Sgr { fg, bg: Some(bg), bold: style.bold, ..Sgr::default() })
        }
    }
}

fn push_color(out: &mut Vec<u8>, c: Color, is_bg: bool, mode: ColorMode) {
    let (r, g, b) = c.components();
    match mode {
        ColorMode::TrueColor => {
            let _ = write!(out, ";{};2;{};{};{}", if is_bg { 48 } else { 38 }, r, g, b);
        }
        ColorMode::Ansi256 => {
            let _ = write!(out, ";{};5;{}", if is_bg { 48 } else { 38 }, to_ansi256(r, g, b));
        }
        ColorMode::Ansi16 => {
            let n = to_ansi16(r, g, b);
            let code = match (n < 8, is_bg) {
                (true, false) => 30 + n as u32,
                (true, true) => 40 + n as u32,
                (false, false) => 90 + (n - 8) as u32,
                (false, true) => 100 + (n - 8) as u32,
            };
            let _ = write!(out, ";{}", code);
        }
        ColorMode::Mono => {}
    }
}

fn push_sgr(out: &mut Vec<u8>, s: &Sgr, mode: ColorMode) {
    out.extend_from_slice(b"\x1b[0");
    if s.bold {
        out.extend_from_slice(b";1");
    }
    if s.faint {
        out.extend_from_slice(b";2");
    }
    if s.reverse {
        out.extend_from_slice(b";7");
    }
    if let Some(fg) = s.fg {
        push_color(out, fg, false, mode);
    }
    if let Some(bg) = s.bg {
        push_color(out, bg, true, mode);
    }
    out.push(b'm');
}

/// Change only what differs from the current attributes. Colors can be
/// switched independently; flag changes need a reset, so fall back to a
/// full SGR then.
fn push_sgr_delta(out: &mut Vec<u8>, cur: &Sgr, next: &Sgr, mode: ColorMode) {
    if cur.bold != next.bold || cur.faint != next.faint || cur.reverse != next.reverse {
        push_sgr(out, next, mode);
        return;
    }
    out.extend_from_slice(b"\x1b[");
    let start = out.len();
    if cur.fg != next.fg {
        match next.fg {
            Some(c) => push_color(out, c, false, mode),
            None => out.extend_from_slice(b";39"),
        }
    }
    if cur.bg != next.bg {
        match next.bg {
            Some(c) => push_color(out, c, true, mode),
            None => out.extend_from_slice(b";49"),
        }
    }
    // push_color writes a leading ';' — drop the first one.
    if out.len() > start && out[start] == b';' {
        out.remove(start);
    }
    out.push(b'm');
}

/// Append the escape sequences that turn `prev` into `cur`. With no previous
/// frame (first paint or after a resize) every cell is written — without a
/// screen clear, so there is no blank flash.
pub fn encode_diff(prev: Option<&Canvas>, cur: &Canvas, mode: RenderMode, out: &mut Vec<u8>) {
    let prev = prev.filter(|p| p.size() == cur.size());
    let (w, h) = (cur.width(), cur.height());
    let start_len = out.len();
    out.extend_from_slice(b"\x1b[?2026h");
    let body_start = out.len();
    let mut cursor: Option<(i32, i32)> = None;
    let mut sgr: Option<Sgr> = None;
    let mut dirty = vec![false; w as usize];
    let mut buf = [0u8; 4];

    for y in 0..h {
        let row = cur.row(y);
        match prev {
            Some(p) => {
                let prow = p.row(y);
                for x in 0..w as usize {
                    dirty[x] = row[x] != prow[x];
                }
            }
            None => dirty.fill(true),
        }
        if !mode.ascii {
            // A changed continuation cell means its wide lead must be redrawn.
            for x in 1..w as usize {
                if dirty[x] && row[x].ch == WIDE_CONT {
                    dirty[x - 1] = true;
                }
            }
        }
        let mut x = 0i32;
        while x < w {
            let cell = row[x as usize];
            if !dirty[x as usize] || (!mode.ascii && cell.ch == WIDE_CONT) {
                x += 1;
                continue;
            }
            let cell = if mode.ascii { ascii_cell(cell) } else { cell };
            let (ch, s) = map_style(cell.ch, cell.style, mode.color);
            let width = if mode.ascii { 1 } else { ch.width().unwrap_or(1).max(1) as i32 };
            if cursor != Some((x, y)) {
                let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
            }
            if sgr != Some(s) {
                match sgr {
                    Some(prev) => push_sgr_delta(out, &prev, &s, mode.color),
                    None => push_sgr(out, &s, mode.color),
                }
                sgr = Some(s);
            }
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            x += width;
            cursor = Some((x, y));
        }
    }

    if out.len() == body_start {
        out.truncate(start_len); // nothing changed: emit nothing at all
    } else {
        out.extend_from_slice(b"\x1b[0m\x1b[?2026l");
    }
}

/// Render a whole canvas as newline-separated ANSI text (for `--snapshot
/// --ansi`), suitable for `cat` in any terminal.
pub fn to_ansi_lines(cur: &Canvas, mode: RenderMode) -> String {
    let mut out = Vec::new();
    let mut buf = [0u8; 4];
    for y in 0..cur.height() {
        let mut sgr: Option<Sgr> = None;
        for &cell in cur.row(y) {
            if !mode.ascii && cell.ch == WIDE_CONT {
                continue;
            }
            let cell = if mode.ascii { ascii_cell(cell) } else { cell };
            let (ch, s) = map_style(cell.ch, cell.style, mode.color);
            if sgr != Some(s) {
                push_sgr(&mut out, &s, mode.color);
                sgr = Some(s);
            }
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
        out.extend_from_slice(b"\x1b[0m\n");
    }
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TC: RenderMode = RenderMode { color: ColorMode::TrueColor, ascii: false };

    #[test]
    fn identical_frames_emit_nothing() {
        let mut a = Canvas::new(10, 3);
        a.text(1, 1, "hello", Style::fg(Color::rgb(255, 153, 0)));
        let mut out = Vec::new();
        encode_diff(Some(&a), &a.clone(), TC, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn single_cell_change_is_a_small_update() {
        let a = Canvas::new(80, 24);
        let mut b = a.clone();
        b.put(40, 12, 'X', Style::fg(Color::rgb(255, 0, 0)));
        let mut out = Vec::new();
        encode_diff(Some(&a), &b, TC, &mut out);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\x1b[13;41H"));
        assert!(s.ends_with("X\x1b[0m\x1b[?2026l"));
        assert!(s.len() < 80, "diff too large: {} bytes", s.len());
    }

    #[test]
    fn color_only_changes_use_short_sgr() {
        let a = Canvas::new(4, 1);
        let mut b = a.clone();
        b.put(0, 0, 'a', Style::fg(Color::rgb(1, 2, 3)));
        b.put(1, 0, 'b', Style::fg(Color::rgb(4, 5, 6)));
        let mut out = Vec::new();
        encode_diff(Some(&a), &b, TC, &mut out);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("a\x1b[38;2;4;5;6mb"), "{s:?}");
    }

    #[test]
    fn ascii_mode_emits_only_ascii() {
        let mut c = Canvas::new(12, 2);
        c.text(0, 0, "◖▄█⠿日本●…", Style::fg(Color::rgb(200, 100, 50)));
        c.text(0, 1, "┃─▲▼·", Style::default());
        let mut out = Vec::new();
        encode_diff(None, &c, RenderMode { color: ColorMode::Mono, ascii: true }, &mut out);
        assert!(out.iter().all(|b| b.is_ascii()));
    }

    #[test]
    fn mono_mode_uses_no_color_codes() {
        let mut c = Canvas::new(4, 1);
        c.put(0, 0, ' ', Style::on(Color::rgb(255, 153, 0), Color::rgb(255, 153, 0)));
        let mut out = Vec::new();
        encode_diff(None, &c, RenderMode { color: ColorMode::Mono, ascii: false }, &mut out);
        let s = String::from_utf8(out).unwrap();
        assert!(!s.contains("38;") && !s.contains("48;"));
        assert!(s.contains("\x1b[0;7m"));
    }

    #[test]
    fn wide_char_change_redraws_lead_cell() {
        let mut a = Canvas::new(4, 1);
        a.text(0, 0, "日", Style::default());
        let mut b = Canvas::new(4, 1);
        b.text(0, 0, "本", Style::default());
        let mut out = Vec::new();
        encode_diff(Some(&a), &b, TC, &mut out);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains('本'));
        assert!(!s.contains('\0'));
    }
}
