use unicode_width::UnicodeWidthChar;

use crate::theme::{Ink, Theme, BOLD, DIM, REVERSE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Ink,
    pub bg: Ink,
    pub attrs: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Ink::Default,
            bg: Ink::Default,
            attrs: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Buffer {
    pub width: i32,
    pub height: i32,
    pub cells: Vec<Cell>,
    pub ascii: bool,
}

impl Buffer {
    pub fn new(width: i32, height: i32, ascii: bool) -> Self {
        let width = width.max(0);
        let height = height.max(0);
        Self {
            width,
            height,
            cells: vec![Cell::default(); (width * height) as usize],
            ascii,
        }
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
    }

    pub fn cell(&self, x: i32, y: i32) -> Option<&Cell> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        self.cells.get((y * self.width + x) as usize)
    }

    pub fn put(&mut self, x: i32, y: i32, ch: char, fg: Ink, bg: Ink, attrs: u8) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let mut ch = ch;
        if self.ascii && !ch.is_ascii() {
            ch = '?';
        }
        if ch.is_control() || ch == '\u{0}' {
            ch = '?';
        }
        let idx = (y * self.width + x) as usize;
        if let Some(cell) = self.cells.get_mut(idx) {
            *cell = Cell { ch, fg, bg, attrs };
        }
    }

    pub fn fill_bg(&mut self, x: i32, y: i32, w: i32, h: i32, bg: Ink) {
        self.fill(x, y, w, h, ' ', Ink::Default, bg, 0);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, ch: char, fg: Ink, bg: Ink, attrs: u8) {
        for yy in y..(y + h) {
            for xx in x..(x + w) {
                self.put(xx, yy, ch, fg, bg, attrs);
            }
        }
    }

    pub fn text(&mut self, x: i32, y: i32, text: &str, fg: Ink, bg: Ink, attrs: u8) -> i32 {
        self.write_text(x, y, text, fg, bg, attrs, false)
    }

    /// Draws text without destroying a color field that is already in place.
    pub fn text_keep_bg(&mut self, x: i32, y: i32, text: &str, fg: Ink, attrs: u8) -> i32 {
        self.write_text(x, y, text, fg, Ink::Default, attrs, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn write_text(
        &mut self,
        x: i32,
        y: i32,
        text: &str,
        fg: Ink,
        bg: Ink,
        attrs: u8,
        keep_bg: bool,
    ) -> i32 {
        let mut cx = x;
        for ch in text.chars() {
            let mut ch = ch;
            if ch.is_control() {
                ch = ' ';
            }
            let w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if w != 1 {
                if w == 0 {
                    continue;
                }
                ch = '?';
            }
            let cell_bg = if keep_bg {
                self.cell(cx, y).map(|c| c.bg).unwrap_or(Ink::Default)
            } else {
                bg
            };
            self.put(cx, y, ch, fg, cell_bg, attrs);
            cx += 1;
        }
        cx
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text_center(&mut self, x: i32, y: i32, w: i32, text: &str, fg: Ink, bg: Ink, attrs: u8) {
        let len = text.chars().count() as i32;
        let start = x + (w - len).max(0) / 2;
        self.text(start, y, text, fg, bg, attrs);
    }

    pub fn text_center_keep_bg(&mut self, x: i32, y: i32, w: i32, text: &str, fg: Ink, attrs: u8) {
        let len = text.chars().count() as i32;
        let start = x + (w - len).max(0) / 2;
        self.text_keep_bg(start, y, text, fg, attrs);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text_right(&mut self, x: i32, y: i32, w: i32, text: &str, fg: Ink, bg: Ink, attrs: u8) {
        let len = text.chars().count() as i32;
        let start = x + (w - len).max(0);
        self.text(start, y, text, fg, bg, attrs);
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, ch: char, fg: Ink, bg: Ink) {
        self.fill(x, y, w, 1, ch, fg, bg, 0);
    }

    pub fn vline(&mut self, x: i32, y: i32, h: i32, ch: char, fg: Ink, bg: Ink) {
        self.fill(x, y, 1, h, ch, fg, bg, 0);
    }

    pub fn plain_text(&self) -> String {
        let mut out = String::with_capacity(((self.width + 1) * self.height) as usize);
        for y in 0..self.height {
            for x in 0..self.width {
                out.push(self.cell(x, y).map(|c| c.ch).unwrap_or(' '));
            }
            if y + 1 < self.height {
                out.push('\n');
            }
        }
        out
    }

    pub fn row_text(&self, y: i32) -> String {
        let mut out = String::new();
        for x in 0..self.width {
            out.push(self.cell(x, y).map(|c| c.ch).unwrap_or(' '));
        }
        out
    }
}

pub struct Presenter {
    prev: Vec<Cell>,
    width: i32,
    height: i32,
    full: bool,
}

impl Default for Presenter {
    fn default() -> Self {
        Self::new()
    }
}

impl Presenter {
    pub fn new() -> Self {
        Self {
            prev: Vec::new(),
            width: 0,
            height: 0,
            full: true,
        }
    }

    pub fn invalidate(&mut self) {
        self.full = true;
    }

    pub fn present(&mut self, buffer: &Buffer, theme: &Theme) -> String {
        let resized = self.width != buffer.width || self.height != buffer.height;
        if resized {
            self.width = buffer.width;
            self.height = buffer.height;
            self.prev = vec![Cell::default(); buffer.cells.len()];
            self.full = true;
        }

        let mut out = String::with_capacity(2048);
        if self.full {
            out.push_str("\x1b[0m\x1b[2J\x1b[H");
            self.full = false;
        }

        let mut last_style: Option<(Ink, Ink, u8)> = None;
        for y in 0..buffer.height {
            let row_start = (y * buffer.width) as usize;
            let row_end = row_start + buffer.width as usize;
            if !resized && buffer.cells[row_start..row_end] == self.prev[row_start..row_end] {
                continue;
            }
            let mut x = 0;
            while x < buffer.width {
                let idx = row_start + x as usize;
                if buffer.cells[idx] == self.prev[idx] {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < buffer.width {
                    let i = row_start + x as usize;
                    if buffer.cells[i] == self.prev[i] {
                        break;
                    }
                    x += 1;
                }
                out.push_str(&format!("\x1b[{};{}H", y + 1, start + 1));
                let cell = &buffer.cells[row_start + start as usize];
                let style = (cell.fg, cell.bg, cell.attrs);
                if last_style != Some(style) {
                    out.push_str(&sgr(theme, cell.fg, cell.bg, cell.attrs));
                    last_style = Some(style);
                }
                let mut current_style = style;
                for i in start..x {
                    let c = &buffer.cells[row_start + i as usize];
                    let next_style = (c.fg, c.bg, c.attrs);
                    if next_style != current_style {
                        out.push_str(&sgr(theme, c.fg, c.bg, c.attrs));
                        last_style = Some(next_style);
                        current_style = next_style;
                    }
                    out.push(c.ch);
                }
            }
        }

        self.prev.clone_from(&buffer.cells);
        if !out.is_empty() {
            out.push_str("\x1b[0m");
        }
        out
    }
}

pub fn sgr(theme: &Theme, fg: Ink, bg: Ink, attrs: u8) -> String {
    let mut codes = String::from("\x1b[0");
    if fg != Ink::Default {
        codes.push(';');
        codes.push_str(&theme.fg_sgr(fg));
    }
    if bg != Ink::Default {
        codes.push(';');
        codes.push_str(&theme.bg_sgr(bg));
    }
    if attrs & BOLD != 0 {
        codes.push_str(";1");
    }
    if attrs & DIM != 0 {
        codes.push_str(";2");
    }
    if attrs & REVERSE != 0 {
        codes.push_str(";7");
    }
    codes.push('m');
    codes
}

/// Synthetic metrics used only by deterministic tests.
#[cfg(test)]
pub fn sample_metrics() -> crate::model::Metrics {
    use crate::model::{LoadAvg, Metrics, ProcInfo};
    Metrics {
        hostname: Some("USS-LINUX".to_string()),
        os_name: Some("Ubuntu".to_string()),
        os_version: Some("24.04".to_string()),
        kernel: Some("7.0.0-38-generic".to_string()),
        arch: "x86_64".to_string(),
        uptime_secs: Some(363_487),
        load: Some(LoadAvg {
            one: 0.84,
            five: 1.02,
            fifteen: 0.91,
        }),
        cpu: Some(37.0),
        per_core: vec![18.0, 61.0, 42.0, 9.0, 73.0, 22.0, 55.0, 31.0],
        mem_total: Some(33_285_996_544),
        mem_used: Some(13_207_428_096),
        mem_available: Some(20_078_548_448),
        swap_total: Some(8_589_934_592),
        swap_used: Some(1_073_741_824),
        disk_mount: Some("/".to_string()),
        disk_total: Some(549_755_813_888),
        disk_available: Some(391_915_225_088),
        net_rx_rate: Some(1_258_291.0),
        net_tx_rate: Some(348_160.0),
        net_ifaces: 3,
        process_count: Some(412),
        top_processes: vec![
            ProcInfo {
                pid: 24111,
                name: "rustc".to_string(),
                cpu: 84.2,
                memory: 1_610_612_736,
            },
            ProcInfo {
                pid: 1872,
                name: "firefox".to_string(),
                cpu: 31.7,
                memory: 2_147_483_648,
            },
            ProcInfo {
                pid: 44201,
                name: "lcars-mon".to_string(),
                cpu: 12.4,
                memory: 12_582_912,
            },
            ProcInfo {
                pid: 902,
                name: "gnome-shell".to_string(),
                cpu: 8.1,
                memory: 587_202_560,
            },
            ProcInfo {
                pid: 51,
                name: "kworker/0:1".to_string(),
                cpu: 2.6,
                memory: 0,
            },
        ],
        collect_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ColorCapability;

    #[test]
    fn drawing_clips_without_panicking() {
        let mut b = Buffer::new(10, 5, false);
        b.text(-4, -4, "hello", Ink::Amber, Ink::Default, 0);
        b.fill_bg(8, 4, 20, 20, Ink::Blue);
        b.put(100, 100, 'x', Ink::White, Ink::Default, 0);
        assert_eq!(b.cell(9, 4).unwrap().bg, Ink::Blue);
    }

    #[test]
    fn ascii_buffer_rejects_unicode() {
        let mut b = Buffer::new(8, 2, true);
        b.text(0, 0, "a·日", Ink::White, Ink::Default, 0);
        assert!(b.plain_text().is_ascii());
    }

    #[test]
    fn presenter_only_atomizes_changes() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let mut p = Presenter::new();
        let mut a = Buffer::new(4, 2, false);
        a.text(0, 0, "abcd", Ink::White, Ink::Default, 0);
        let first = p.present(&a, &theme);
        assert!(first.contains("abcd"));
        let second = p.present(&a, &theme);
        assert_eq!(second, "");
        a.text(2, 0, "XY", Ink::Amber, Ink::Default, 0);
        let third = p.present(&a, &theme);
        assert!(third.contains("XY"));
        assert!(!third.contains("ab"));
    }

    #[test]
    fn full_width_rows_are_complete() {
        let b = Buffer::new(120, 36, false);
        let text = b.plain_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 36);
        assert!(lines.iter().all(|l| l.chars().count() == 120));
    }
}
