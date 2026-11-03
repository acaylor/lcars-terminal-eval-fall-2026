use super::color::{ansi_sgr, shape_char, ColorMode, Rgb, BLACK, CREAM};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub skip: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: CREAM,
            bg: BLACK,
            skip: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Buffer {
    pub width: u16,
    pub height: u16,
    cells: Vec<Cell>,
}

impl Buffer {
    pub fn new(width: u16, height: u16) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let n = width as usize * height as usize;
        Self {
            width,
            height,
            cells: vec![Cell::default(); n],
        }
    }

    pub fn cell(&self, x: i32, y: i32) -> Cell {
        if !self.in_bounds(x, y) {
            return Cell::default();
        }
        self.cells[self.idx(x, y)]
    }

    pub fn set(&mut self, x: i32, y: i32, ch: char, fg: Rgb, bg: Rgb) {
        if !self.in_bounds(x, y) {
            return;
        }
        let i = self.idx(x, y);
        self.cells[i] = Cell {
            ch,
            fg,
            bg,
            skip: false,
        };
    }

    pub fn set_skip(&mut self, x: i32, y: i32) {
        if !self.in_bounds(x, y) {
            return;
        }
        let i = self.idx(x, y);
        self.cells[i].skip = true;
        self.cells[i].ch = ' ';
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, bg: Rgb) {
        if w <= 0 || h <= 0 {
            return;
        }
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, ' ', bg, bg);
            }
        }
    }

    pub fn clear_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        self.fill(x, y, w, h, BLACK);
        if w <= 0 || h <= 0 {
            return;
        }
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, ' ', CREAM, BLACK);
            }
        }
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, fg: Rgb, bg: Rgb) -> i32 {
        let mut col = x;
        for ch in s.chars() {
            if ch.is_control() {
                continue;
            }
            let width = display_width(ch);
            if width == 0 {
                continue;
            }
            if col + width > self.width as i32 {
                break;
            }
            self.set(col, y, ch, fg, bg);
            if width == 2 {
                self.set_skip(col + 1, y);
                let i = self.idx(col + 1, y);
                self.cells[i].fg = fg;
                self.cells[i].bg = bg;
            }
            col += width;
        }
        col
    }

    pub fn empty_ratio(&self) -> f64 {
        let empty = self
            .cells
            .iter()
            .filter(|c| c.bg == BLACK && c.ch == ' ')
            .count();
        empty as f64 / self.cells.len() as f64
    }

    pub fn text_content(&self) -> String {
        let mut s = String::new();
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let c = self.cell(x, y);
                if !c.skip {
                    s.push(c.ch);
                }
            }
            s.push('\n');
        }
        s
    }

    pub fn shape_map(&self) -> String {
        let mut s = String::new();
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let c = self.cell(x, y);
                s.push(shape_char(c.bg, c.ch));
            }
            s.push('\n');
        }
        s
    }

    pub fn row_changed(&self, other: &Buffer, y: i32) -> bool {
        if self.width != other.width || self.height != other.height {
            return true;
        }
        let start = y as usize * self.width as usize;
        let end = start + self.width as usize;
        self.cells[start..end] != other.cells[start..end]
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }

    fn idx(&self, x: i32, y: i32) -> usize {
        y as usize * self.width as usize + x as usize
    }
}

pub fn display_width(ch: char) -> i32 {
    if ch.is_control() {
        return 0;
    }
    match ch as u32 {
        0x1100..=0x115F
        | 0x2329..=0x232A
        | 0x2E80..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE10..=0xFE19
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

pub fn fit(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    let chars: Vec<char> = s.chars().filter(|c| !c.is_control()).collect();
    let total: usize = chars
        .iter()
        .map(|c| display_width(*c).max(0) as usize)
        .sum();
    if total <= width {
        return chars.into_iter().collect();
    }
    let limit = width.saturating_sub(1);
    for ch in chars {
        let w = display_width(ch).max(0) as usize;
        if used + w > limit {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

pub fn fit_ascii(s: &str, width: usize) -> String {
    let t = fit(s, width);
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in t.chars() {
        let ch = if ch == '…' { '.' } else { ch };
        let w = display_width(ch).max(1) as usize;
        if used + w > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

pub fn diff_rows(prev: &Buffer, next: &Buffer, mode: ColorMode, force: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(next.cells.len() * 8);
    if force || prev.width != next.width || prev.height != next.height {
        out.extend_from_slice(b"\x1b[2J\x1b[H");
    }
    for y in 0..next.height as i32 {
        if !force
            && prev.width == next.width
            && prev.height == next.height
            && !next.row_changed(prev, y)
        {
            continue;
        }
        push_move(&mut out, y, 0);
        let mut fg = Rgb(0, 0, 0);
        let mut bg = Rgb(1, 1, 1);
        let mut styled = false;
        for x in 0..next.width as i32 {
            let c = next.cell(x, y);
            if c.skip {
                continue;
            }
            if !styled || c.fg != fg || c.bg != bg {
                out.extend_from_slice(ansi_sgr(mode, c.fg, c.bg).as_bytes());
                fg = c.fg;
                bg = c.bg;
                styled = true;
            }
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.ch.encode_utf8(&mut buf).as_bytes());
        }
    }
    out.extend_from_slice(b"\x1b[0m");
    out
}

fn push_move(out: &mut Vec<u8>, y: i32, x: i32) {
    out.extend_from_slice(format!("\x1b[{};{}H", y + 1, x + 1).as_bytes());
}

pub fn fmt_bytes(n: u64) -> String {
    const K: f64 = 1024.0;
    let v = n as f64;
    if v >= K * K * K * K {
        format!("{:.1}T", v / (K * K * K * K))
    } else if v >= K * K * K {
        format!("{:.1}G", v / (K * K * K))
    } else if v >= K * K {
        format!("{:.1}M", v / (K * K))
    } else if v >= K {
        format!("{:.0}K", v / K)
    } else {
        format!("{n}B")
    }
}

pub fn fmt_rate(n: f64) -> String {
    if !n.is_finite() || n < 0.0 {
        return "--".into();
    }
    format!("{}/s", fmt_bytes(n.round() as u64))
}

pub fn fmt_pct(n: f64) -> String {
    if !n.is_finite() {
        return "--".into();
    }
    format!("{:.0}%", n.clamp(0.0, 999.0))
}

pub fn fmt_load(n: f64) -> String {
    if !n.is_finite() {
        return "-.--".into();
    }
    format!("{n:.2}")
}

pub fn uptime_label(d: std::time::Duration) -> String {
    let s = d.as_secs();
    let days = s / 86400;
    let hours = (s % 86400) / 3600;
    let mins = (s % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours:02}h {mins:02}m")
    } else if hours > 0 {
        format!("{hours}h {mins:02}m")
    } else {
        format!("{mins}m {:02}s", s % 60)
    }
}

pub fn opt_bytes(v: Option<u64>) -> String {
    v.map(fmt_bytes).unwrap_or_else(|| "--".into())
}

pub fn dash(unicode: bool) -> &'static str {
    if unicode {
        "—"
    } else {
        "--"
    }
}
