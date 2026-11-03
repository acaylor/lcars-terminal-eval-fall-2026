use std::fmt::Write as FmtWrite;

use crate::theme::{ColorMode, ColorSpec, GlyphMode, LcarsTone};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: ColorSpec,
    pub bg: ColorSpec,
    pub bold: bool,
    pub dim: bool,
    pub reverse: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: ColorSpec::Reset,
            bg: ColorSpec::Reset,
            bold: false,
            dim: false,
            reverse: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub glyph_mode: GlyphMode,
    pub cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(width: usize, height: usize, glyph_mode: GlyphMode) -> Self {
        let len = width.saturating_mul(height);
        Self {
            width,
            height,
            glyph_mode,
            cells: vec![Cell::default(); len],
        }
    }

    pub fn resize_and_clear(&mut self, width: usize, height: usize, glyph_mode: GlyphMode) {
        self.width = width;
        self.height = height;
        self.glyph_mode = glyph_mode;
        let len = width.saturating_mul(height);
        self.cells.clear();
        self.cells.resize(len, Cell::default());
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
    }

    #[inline]
    pub fn in_bounds(&self, x: usize, y: usize) -> bool {
        x < self.width && y < self.height
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Option<&Cell> {
        if self.in_bounds(x, y) {
            Some(&self.cells[y * self.width + x])
        } else {
            None
        }
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, cell: Cell) {
        if self.in_bounds(x, y) {
            self.cells[y * self.width + x] = cell;
        }
    }

    #[inline]
    pub fn put_char(&mut self, x: usize, y: usize, ch: char, fg: ColorSpec, bg: ColorSpec) {
        self.set(
            x,
            y,
            Cell {
                ch: self.map_glyph(ch),
                fg,
                bg,
                bold: false,
                dim: false,
                reverse: false,
            },
        );
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn put_styled(
        &mut self,
        x: usize,
        y: usize,
        ch: char,
        fg: ColorSpec,
        bg: ColorSpec,
        bold: bool,
        dim: bool,
    ) {
        self.set(
            x,
            y,
            Cell {
                ch: self.map_glyph(ch),
                fg,
                bg,
                bold,
                dim,
                reverse: false,
            },
        );
    }

    /// Maps Unicode structural glyphs to safe 7-bit ASCII equivalents when in `GlyphMode::Ascii`.
    #[inline]
    pub fn map_glyph(&self, ch: char) -> char {
        if self.glyph_mode == GlyphMode::Unicode || ch.is_ascii() {
            return ch;
        }
        match ch {
            '█' | '▉' | '▊' | '▋' => '#',
            '▌' | '▐' => '|',
            '▀' => '^',
            '▄' => '_',
            '▛' | '╭' | '┌' => '/',
            '▜' | '╮' | '┐' => '\\',
            '▙' | '╰' | '└' => '\\',
            '▟' | '╯' | '┘' => '/',
            '━' | '─' | '═' => '=',
            '┃' | '│' => '|',
            '◖' => '[',
            '◗' => ']',
            '●' | '◉' | '◆' => '*',
            '○' | '◇' => 'o',
            '▸' | '▶' => '>',
            '◂' | '◀' => '<',
            '…' => '.',
            c if ('\u{2800}'..='\u{28FF}').contains(&c) => {
                let bits = (c as u32) - 0x2800;
                match bits.count_ones() {
                    0 => ' ',
                    1..=2 => '.',
                    3..=4 => ':',
                    5..=6 => '+',
                    _ => '*',
                }
            }
            _ => '#',
        }
    }

    /// Draws a horizontal string clipped safely to canvas bounds.
    pub fn draw_text(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        fg: ColorSpec,
        bg: ColorSpec,
        bold: bool,
    ) {
        if y >= self.height {
            return;
        }
        for (i, ch) in text.chars().enumerate() {
            let cx = x.saturating_add(i);
            if cx >= self.width {
                break;
            }
            self.set(
                cx,
                y,
                Cell {
                    ch: self.map_glyph(ch),
                    fg,
                    bg,
                    bold,
                    dim: false,
                    reverse: false,
                },
            );
        }
    }

    /// Draws a solid structural block rectangle (`█` in Unicode, `#` in ASCII) with optional gradient.
    pub fn fill_solid_block(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        tone_start: LcarsTone,
        tone_end: LcarsTone,
    ) {
        if w == 0 || h == 0 {
            return;
        }
        for dy in 0..h {
            let cy = y + dy;
            if cy >= self.height {
                break;
            }
            for dx in 0..w {
                let cx = x + dx;
                if cx >= self.width {
                    break;
                }
                let ratio = if w > 1 {
                    dx as f32 / (w - 1) as f32
                } else {
                    0.0
                };
                let fg = ColorSpec::interpolate(tone_start, tone_end, ratio);
                self.put_char(cx, cy, '█', fg, ColorSpec::Reset);
            }
        }
    }

    /// Stamps a compact LCARS cutout stencil label inside a solid spine block or rail,
    /// keeping the outer structural glyphs intact so the silhouette remains solid.
    pub fn stamp_stencil_label(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        rail_tone: LcarsTone,
    ) {
        if y >= self.height {
            return;
        }
        for (i, ch) in text.chars().enumerate() {
            let cx = x + i;
            if cx >= self.width {
                break;
            }
            self.set(
                cx,
                y,
                Cell {
                    ch: self.map_glyph(ch),
                    fg: ColorSpec::Tone(LcarsTone::CutoutDark),
                    bg: ColorSpec::Tone(rail_tone),
                    bold: true,
                    dim: false,
                    reverse: true,
                },
            );
        }
    }

    /// Draws an authentic sweeping LCARS Upper-Left Elbow:
    /// - Rounded outer corner (`▄`, `▟`, `▐`)
    /// - Thick vertical spine shoulder (`spine_w` columns wide)
    /// - Concave inner fillet (`▛`, `▀`) curving into a horizontal rail of `rail_w` width
    #[allow(clippy::too_many_arguments)]
    pub fn draw_upper_left_elbow(
        &mut self,
        x: usize,
        y: usize,
        spine_w: usize,
        elbow_h: usize,
        rail_w: usize,
        rail_h: usize,
        tone_start: LcarsTone,
        tone_end: LcarsTone,
    ) {
        if spine_w < 3 || elbow_h < 2 || rail_w < spine_w {
            return;
        }
        let rh = rail_h.max(1).min(elbow_h);

        // 1. Draw the horizontal top rail across `x .. x + rail_w`
        for dy in 0..rh {
            let cy = y + dy;
            for dx in 0..rail_w {
                let cx = x + dx;
                // Carve outer upper-left curve on top-left corner
                if dy == 0 && dx < 2 {
                    continue;
                }
                if dy == 1 && dx == 0 && rh >= 2 {
                    let fg = ColorSpec::Tone(tone_start);
                    self.put_char(cx, cy, '▟', fg, ColorSpec::Reset);
                    continue;
                }
                let ratio = if rail_w > 1 {
                    dx as f32 / (rail_w - 1) as f32
                } else {
                    0.0
                };
                let fg = ColorSpec::interpolate(tone_start, tone_end, ratio);
                let ch = if dy == 0 && dx < 4 { '▄' } else { '█' };
                self.put_char(cx, cy, ch, fg, ColorSpec::Reset);
            }
        }

        // Rounded right cap on the top rail
        let cap_x = x + rail_w - 1;
        let end_fg = ColorSpec::Tone(tone_end);
        if rh == 1 {
            self.put_char(cap_x, y, '◗', end_fg, ColorSpec::Reset);
        } else {
            self.put_char(cap_x, y, '▜', end_fg, ColorSpec::Reset);
            self.put_char(cap_x, y + rh - 1, '▟', end_fg, ColorSpec::Reset);
        }

        // 2. Draw vertical spine rows below the rail + concave inner fillet
        let spine_fg = ColorSpec::Tone(tone_start);
        for dy in rh..elbow_h {
            let cy = y + dy;
            for dx in 0..spine_w {
                let cx = x + dx;
                let ch = if dy == rh && dx == 0 {
                    '▐'
                } else if dx == spine_w - 1 {
                    '▌'
                } else {
                    '█'
                };
                self.put_char(cx, cy, ch, spine_fg, ColorSpec::Reset);
            }
            // Concave inner curve right under the horizontal rail
            if dy == rh && spine_w + 3 <= rail_w {
                self.put_char(x + spine_w - 1, cy, '█', spine_fg, ColorSpec::Reset);
                self.put_char(x + spine_w, cy, '▛', spine_fg, ColorSpec::Reset);
                self.put_char(x + spine_w + 1, cy, '▀', spine_fg, ColorSpec::Reset);
                self.put_char(x + spine_w + 2, cy, '▀', spine_fg, ColorSpec::Reset);
            }
        }
    }

    /// Draws an authentic sweeping LCARS Lower-Left Elbow with outer rounded corner
    /// and inner concave fillet (`▙`, `▄`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_lower_left_elbow(
        &mut self,
        x: usize,
        y: usize,
        spine_w: usize,
        elbow_h: usize,
        rail_w: usize,
        rail_h: usize,
        tone_start: LcarsTone,
        tone_end: LcarsTone,
    ) {
        if spine_w < 3 || elbow_h < 2 || rail_w < spine_w {
            return;
        }
        let rh = rail_h.max(1).min(elbow_h);
        let spine_rows = elbow_h - rh;
        let spine_fg = ColorSpec::Tone(tone_start);

        // 1. Vertical spine portion above the bottom rail + concave inner fillet
        for dy in 0..spine_rows {
            let cy = y + dy;
            for dx in 0..spine_w {
                let cx = x + dx;
                let ch = if dy + 1 == spine_rows && dx == 0 {
                    '▐'
                } else if dx == spine_w - 1 {
                    '▌'
                } else {
                    '█'
                };
                self.put_char(cx, cy, ch, spine_fg, ColorSpec::Reset);
            }
            if dy + 1 == spine_rows && spine_w + 3 <= rail_w {
                self.put_char(x + spine_w - 1, cy, '█', spine_fg, ColorSpec::Reset);
                self.put_char(x + spine_w, cy, '▙', spine_fg, ColorSpec::Reset);
                self.put_char(x + spine_w + 1, cy, '▄', spine_fg, ColorSpec::Reset);
                self.put_char(x + spine_w + 2, cy, '▄', spine_fg, ColorSpec::Reset);
            }
        }

        // 2. Bottom horizontal rail with outer lower-left curve
        for dy in 0..rh {
            let cy = y + spine_rows + dy;
            let is_last_row = dy + 1 == rh;
            for dx in 0..rail_w {
                let cx = x + dx;
                if is_last_row && dx < 2 {
                    continue;
                }
                if dy == 0 && rh >= 2 && dx == 0 {
                    self.put_char(cx, cy, '▜', spine_fg, ColorSpec::Reset);
                    continue;
                }
                let ratio = if rail_w > 1 {
                    dx as f32 / (rail_w - 1) as f32
                } else {
                    0.0
                };
                let fg = ColorSpec::interpolate(tone_start, tone_end, ratio);
                let ch = if is_last_row && dx < 4 { '▀' } else { '█' };
                self.put_char(cx, cy, ch, fg, ColorSpec::Reset);
            }
        }

        // Rounded right cap
        let cap_x = x + rail_w - 1;
        let end_fg = ColorSpec::Tone(tone_end);
        let base_y = y + spine_rows;
        if rh == 1 {
            self.put_char(cap_x, base_y, '◗', end_fg, ColorSpec::Reset);
        } else {
            self.put_char(cap_x, base_y, '▜', end_fg, ColorSpec::Reset);
            self.put_char(cap_x, base_y + rh - 1, '▟', end_fg, ColorSpec::Reset);
        }
    }

    /// Draws an LCARS Mid-Spine T-Bridge with both upper and lower concave fillets (`▙▄▄` and `▛▀▀`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_mid_left_bridge(
        &mut self,
        x: usize,
        y: usize,
        spine_w: usize,
        rail_w: usize,
        tone_start: LcarsTone,
        tone_end: LcarsTone,
    ) {
        if spine_w < 3 || rail_w < spine_w + 4 {
            return;
        }
        let fg_start = ColorSpec::Tone(tone_start);

        // Row y: upper spine row + upper concave fillet (`▙▄▄`)
        for dx in 0..spine_w {
            self.put_char(x + dx, y, '█', fg_start, ColorSpec::Reset);
        }
        self.put_char(x + spine_w, y, '▙', fg_start, ColorSpec::Reset);
        self.put_char(x + spine_w + 1, y, '▄', fg_start, ColorSpec::Reset);
        self.put_char(x + spine_w + 2, y, '▄', fg_start, ColorSpec::Reset);

        // Row y + 1: main horizontal bridge rail
        for dx in 0..rail_w {
            let ratio = dx as f32 / (rail_w - 1).max(1) as f32;
            let fg = ColorSpec::interpolate(tone_start, tone_end, ratio);
            let ch = if dx + 1 == rail_w { '◗' } else { '█' };
            self.put_char(x + dx, y + 1, ch, fg, ColorSpec::Reset);
        }

        // Row y + 2: lower spine row + lower concave fillet (`▛▀▀`)
        for dx in 0..spine_w {
            self.put_char(x + dx, y + 2, '█', fg_start, ColorSpec::Reset);
        }
        self.put_char(x + spine_w, y + 2, '▛', fg_start, ColorSpec::Reset);
        self.put_char(x + spine_w + 1, y + 2, '▀', fg_start, ColorSpec::Reset);
        self.put_char(x + spine_w + 2, y + 2, '▀', fg_start, ColorSpec::Reset);
    }

    /// Draws an Upper-Right Sweeping Elbow (used in the Secondary Diagnostic View for a distinct right-heavy silhouette).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_upper_right_elbow(
        &mut self,
        right_x: usize,
        y: usize,
        spine_w: usize,
        elbow_h: usize,
        rail_w: usize,
        rail_h: usize,
        tone_left: LcarsTone,
        tone_right: LcarsTone,
    ) {
        if spine_w < 3 || elbow_h < 2 || rail_w < spine_w || right_x + 1 < rail_w {
            return;
        }
        let start_x = right_x + 1 - rail_w;
        let spine_x = right_x + 1 - spine_w;
        let rh = rail_h.max(1).min(elbow_h);

        // 1. Horizontal top rail from `start_x ..= right_x`
        for dy in 0..rh {
            let cy = y + dy;
            for dx in 0..rail_w {
                let cx = start_x + dx;
                let from_right = rail_w - 1 - dx;
                if dy == 0 && from_right < 2 {
                    continue;
                }
                if dy == 1 && from_right == 0 && rh >= 2 {
                    self.put_char(cx, cy, '▙', ColorSpec::Tone(tone_right), ColorSpec::Reset);
                    continue;
                }
                let ratio = dx as f32 / (rail_w - 1).max(1) as f32;
                let fg = ColorSpec::interpolate(tone_left, tone_right, ratio);
                let ch = if dx == 0 {
                    '◖'
                } else if dy == 0 && from_right < 4 {
                    '▄'
                } else {
                    '█'
                };
                self.put_char(cx, cy, ch, fg, ColorSpec::Reset);
            }
        }

        // 2. Right-hand vertical spine + inner left-facing concave fillet (`▜`, `▀`)
        let spine_fg = ColorSpec::Tone(tone_right);
        for dy in rh..elbow_h {
            let cy = y + dy;
            for dx in 0..spine_w {
                let cx = spine_x + dx;
                let ch = if dy == rh && dx + 1 == spine_w {
                    '▌'
                } else if dx == 0 {
                    '▐'
                } else {
                    '█'
                };
                self.put_char(cx, cy, ch, spine_fg, ColorSpec::Reset);
            }
            if dy == rh && spine_x >= 3 {
                self.put_char(spine_x, cy, '█', spine_fg, ColorSpec::Reset);
                self.put_char(spine_x - 1, cy, '▜', spine_fg, ColorSpec::Reset);
                self.put_char(spine_x - 2, cy, '▀', spine_fg, ColorSpec::Reset);
                self.put_char(spine_x - 3, cy, '▀', spine_fg, ColorSpec::Reset);
            }
        }
    }

    /// Draws a Lower-Right Sweeping Elbow for the Secondary Diagnostic View.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_lower_right_elbow(
        &mut self,
        right_x: usize,
        y: usize,
        spine_w: usize,
        elbow_h: usize,
        rail_w: usize,
        rail_h: usize,
        tone_left: LcarsTone,
        tone_right: LcarsTone,
    ) {
        if spine_w < 3 || elbow_h < 2 || rail_w < spine_w || right_x + 1 < rail_w {
            return;
        }
        let start_x = right_x + 1 - rail_w;
        let spine_x = right_x + 1 - spine_w;
        let rh = rail_h.max(1).min(elbow_h);
        let spine_rows = elbow_h - rh;
        let spine_fg = ColorSpec::Tone(tone_right);

        for dy in 0..spine_rows {
            let cy = y + dy;
            for dx in 0..spine_w {
                let cx = spine_x + dx;
                let ch = if dy + 1 == spine_rows && dx + 1 == spine_w {
                    '▌'
                } else if dx == 0 {
                    '▐'
                } else {
                    '█'
                };
                self.put_char(cx, cy, ch, spine_fg, ColorSpec::Reset);
            }
            if dy + 1 == spine_rows && spine_x >= 3 {
                self.put_char(spine_x, cy, '█', spine_fg, ColorSpec::Reset);
                self.put_char(spine_x - 1, cy, '▟', spine_fg, ColorSpec::Reset);
                self.put_char(spine_x - 2, cy, '▄', spine_fg, ColorSpec::Reset);
                self.put_char(spine_x - 3, cy, '▄', spine_fg, ColorSpec::Reset);
            }
        }

        for dy in 0..rh {
            let cy = y + spine_rows + dy;
            let is_last = dy + 1 == rh;
            for dx in 0..rail_w {
                let cx = start_x + dx;
                let from_right = rail_w - 1 - dx;
                if is_last && from_right < 2 {
                    continue;
                }
                let ratio = dx as f32 / (rail_w - 1).max(1) as f32;
                let fg = ColorSpec::interpolate(tone_left, tone_right, ratio);
                let ch = if dx == 0 {
                    '◖'
                } else if is_last && from_right < 4 {
                    '▀'
                } else {
                    '█'
                };
                self.put_char(cx, cy, ch, fg, ColorSpec::Reset);
            }
        }
    }

    /// Draws a 1-row rounded LCARS pill (`◖███◗`) with optional stencil label inside.
    pub fn draw_pill(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        tone: LcarsTone,
        label: Option<&str>,
    ) {
        if width < 2 || y >= self.height {
            return;
        }
        let fg = ColorSpec::Tone(tone);
        self.put_char(x, y, '◖', fg, ColorSpec::Reset);
        for dx in 1..width - 1 {
            self.put_char(x + dx, y, '█', fg, ColorSpec::Reset);
        }
        self.put_char(x + width - 1, y, '◗', fg, ColorSpec::Reset);

        if let Some(lbl) = label {
            let max_lbl = width.saturating_sub(2);
            if max_lbl > 0 {
                let clipped: String = lbl.chars().take(max_lbl).collect();
                let char_len = clipped.chars().count();
                let start_x = x + 1 + (max_lbl.saturating_sub(char_len)) / 2;
                self.stamp_stencil_label(start_x, y, &clipped, tone);
            }
        }
    }

    /// Draws an integrated LCARS power conduit / gauge rail using sub-block resolution and half-block guide rails.
    pub fn draw_conduit_gauge(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        pct: f32,
        tone_low: LcarsTone,
        tone_high: LcarsTone,
    ) {
        if width < 4 || y >= self.height {
            return;
        }
        let clamped = pct.clamp(0.0, 100.0) / 100.0;
        let inner_w = width - 2;
        let half_steps = (clamped * (inner_w as f32 * 2.0)).round() as usize;
        let full_cells = half_steps / 2;
        let has_half = (half_steps % 2) == 1;

        let cap_left_fg = ColorSpec::Tone(tone_low);
        self.put_char(x, y, '◖', cap_left_fg, ColorSpec::Reset);

        for i in 0..inner_w {
            let cx = x + 1 + i;
            let ratio = i as f32 / inner_w.max(1) as f32;
            let active_fg = ColorSpec::interpolate(tone_low, tone_high, ratio);
            if i < full_cells {
                self.put_char(cx, y, '█', active_fg, ColorSpec::Reset);
            } else if i == full_cells && has_half {
                self.put_char(
                    cx,
                    y,
                    '▌',
                    active_fg,
                    ColorSpec::Tone(LcarsTone::TrackDark),
                );
            } else {
                // Structural guide track (`━`) so the conduit remains visible even without color/text
                self.put_char(
                    cx,
                    y,
                    '━',
                    ColorSpec::Tone(LcarsTone::TrackDark),
                    ColorSpec::Reset,
                );
            }
        }

        let end_tone = if clamped > 0.85 {
            tone_high
        } else {
            LcarsTone::TrackDark
        };
        self.put_char(x + width - 1, y, '◗', ColorSpec::Tone(end_tone), ColorSpec::Reset);
    }

    /// Draws 3-row high custom LCARS vector digits (`0-9`, `:`, `.`, `%`, `-`, ` `) using half-block and quadrant glyphs.
    /// Returns the total horizontal width drawn.
    pub fn draw_big_digits(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        tone_top: LcarsTone,
        tone_bot: LcarsTone,
    ) -> usize {
        let mut cur_x = x;
        for ch in text.chars() {
            let glyph = big_digit_glyph(ch);
            let gw = glyph[0].chars().count();
            for (row_idx, row_str) in glyph.iter().enumerate() {
                let cy = y + row_idx;
                let ratio = row_idx as f32 / 2.0;
                let fg = ColorSpec::interpolate(tone_top, tone_bot, ratio);
                for (col_idx, gch) in row_str.chars().enumerate() {
                    if gch != ' ' {
                        self.put_styled(cur_x + col_idx, cy, gch, fg, ColorSpec::Reset, true, false);
                    }
                }
            }
            cur_x += gw + 1;
        }
        cur_x.saturating_sub(x)
    }

    /// Renders the canvas as a plain string without ANSI escape codes (for snapshot testing).
    pub fn render_plain(&self) -> String {
        let mut out = String::with_capacity((self.width + 1) * self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                out.push(self.cells[y * self.width + x].ch);
            }
            if y + 1 < self.height {
                out.push('\n');
            }
        }
        out
    }

    /// Renders the canvas with all alphanumeric text replaced by spaces `' '` (or solid block
    /// where stencil text was cut into a solid LCARS rail/pill), directly verifying Visual
    /// Acceptance Test #5 ("Would the layout remain recognizable if all text were replaced by blanks?").
    pub fn render_plain_text_stripped(&self) -> String {
        let mut out = String::with_capacity((self.width + 1) * self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = &self.cells[y * self.width + x];
                let ch = cell.ch;
                if ch.is_alphanumeric()
                    || matches!(
                        ch,
                        ':' | '.' | '%' | '/' | '-' | '[' | ']' | '?' | '(' | ')'
                    )
                {
                    if cell.reverse {
                        out.push(self.map_glyph('█'));
                    } else {
                        out.push(' ');
                    }
                } else if cell.reverse && ch == ' ' {
                    out.push(self.map_glyph('█'));
                } else {
                    out.push(ch);
                }
            }
            if y + 1 < self.height {
                out.push('\n');
            }
        }
        out
    }

    /// Renders a complete multi-line ANSI string suitable for terminal display or file capture.
    pub fn render_ansi_snapshot(&self, color_mode: ColorMode) -> String {
        let mut out = String::with_capacity(self.width * self.height * 4);
        for y in 0..self.height {
            let mut last_style: Option<(ColorSpec, ColorSpec, bool, bool, bool)> = None;
            for x in 0..self.width {
                let cell = &self.cells[y * self.width + x];
                let style = (cell.fg, cell.bg, cell.bold, cell.dim, cell.reverse);
                if last_style != Some(style) {
                    write_sgr(&mut out, cell, color_mode);
                    last_style = Some(style);
                }
                out.push(cell.ch);
            }
            if color_mode != ColorMode::Monochrome || last_style.is_some() {
                out.push_str("\x1b[0m");
            }
            if y + 1 < self.height {
                out.push('\n');
            }
        }
        out
    }

    /// Renders only the changed cells between `prev` and `self` using cursor positioning
    /// and Synchronized Output (`DECSET 2026`) so the live terminal never flickers.
    pub fn render_diff(&self, prev: Option<&Canvas>, color_mode: ColorMode, out: &mut String) {
        out.clear();
        // Begin synchronized update
        out.push_str("\x1b[?2026h");

        let full_repaint = match prev {
            Some(p) => p.width != self.width || p.height != self.height,
            None => true,
        };

        let mut cur_x: Option<usize> = None;
        let mut cur_y: Option<usize> = None;
        let mut last_style: Option<(ColorSpec, ColorSpec, bool, bool, bool)> = None;

        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y * self.width + x;
                let cell = &self.cells[idx];
                if !full_repaint {
                    if let Some(p) = prev {
                        if &p.cells[idx] == cell {
                            continue;
                        }
                    }
                }

                if cur_y != Some(y) || cur_x != Some(x) {
                    let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
                }

                let style = (cell.fg, cell.bg, cell.bold, cell.dim, cell.reverse);
                if last_style != Some(style) {
                    write_sgr(out, cell, color_mode);
                    last_style = Some(style);
                }

                out.push(cell.ch);
                cur_y = Some(y);
                cur_x = Some(x + 1);
            }
        }

        out.push_str("\x1b[0m\x1b[?2026l");
    }
}

fn write_sgr(out: &mut String, cell: &Cell, color_mode: ColorMode) {
    out.push_str("\x1b[0");
    if cell.bold {
        out.push_str(";1");
    }
    if cell.dim {
        out.push_str(";2");
    }
    if cell.reverse && color_mode == ColorMode::Monochrome {
        out.push_str(";7");
    }
    if color_mode != ColorMode::Monochrome {
        write_color_code(out, cell.fg, true, color_mode);
        write_color_code(out, cell.bg, false, color_mode);
    }
    out.push('m');
}

fn write_color_code(out: &mut String, spec: ColorSpec, is_fg: bool, mode: ColorMode) {
    match mode {
        ColorMode::Monochrome => {}
        ColorMode::TrueColor => {
            if let Some((r, g, b)) = spec.resolve_rgb() {
                if is_fg {
                    let _ = write!(out, ";38;2;{};{};{}", r, g, b);
                } else {
                    let _ = write!(out, ";48;2;{};{}", r, g);
                    let _ = write!(out, ";{}", b);
                }
            }
        }
        ColorMode::Ansi256 => {
            if let Some(tone) = spec.fallback_tone() {
                let idx = tone.ansi256();
                if is_fg {
                    let _ = write!(out, ";38;5;{}", idx);
                } else {
                    let _ = write!(out, ";48;5;{}", idx);
                }
            }
        }
        ColorMode::Ansi16 => {
            if let Some(tone) = spec.fallback_tone() {
                let code = if is_fg {
                    tone.ansi16_fg()
                } else {
                    tone.ansi16_bg()
                };
                let _ = write!(out, ";{}", code);
            }
        }
    }
}

/// 3-row high vector glyphs using Unicode half-block and quadrant characters.
fn big_digit_glyph(ch: char) -> [&'static str; 3] {
    match ch {
        '0' => ["▛▀▜", "▌ ▐", "▙▄▟"],
        '1' => ["▄█ ", " █ ", "▄█▄"],
        '2' => ["▀▀▜", "▛▀▀", "▙▄▄"],
        '3' => ["▀▀▜", " ▀▜", "▄▄▟"],
        '4' => ["▌ ▐", "▙▄█", "  ▐"],
        '5' => ["▛▀▀", "▀▀▜", "▄▄▟"],
        '6' => ["▛▀▀", "▛▀▜", "▙▄▟"],
        '7' => ["▀▀█", "  ▐", "  ▐"],
        '8' => ["▛▀▜", "▛▀▜", "▙▄▟"],
        '9' => ["▛▀▜", "▙▄█", "▄▄▟"],
        ':' => [" ▄ ", "   ", " ▀ "],
        '.' => ["   ", "   ", " ▄ "],
        '%' => ["▙ ▞", " ▞ ", "▞ ▜"],
        '-' => ["   ", "▀▀▀", "   "],
        _ => ["   ", "   ", "   "],
    }
}

/// Sub-cell `2x4` Unicode Braille dot matrix canvas (`width * 2` x `height * 4` sub-pixels).
pub struct BrailleGrid {
    pub cell_w: usize,
    pub cell_h: usize,
    dots: Vec<u8>,
    tones: Vec<Option<LcarsTone>>,
}

impl BrailleGrid {
    pub fn new(cell_w: usize, cell_h: usize) -> Self {
        let len = cell_w.saturating_mul(cell_h);
        Self {
            cell_w,
            cell_h,
            dots: vec![0u8; len],
            tones: vec![None; len],
        }
    }

    #[inline]
    pub fn sub_width(&self) -> usize {
        self.cell_w * 2
    }

    #[inline]
    pub fn sub_height(&self) -> usize {
        self.cell_h * 4
    }

    /// Sets a single sub-pixel dot at `(sx, sy)` where `sx < cell_w * 2` and `sy < cell_h * 4`.
    pub fn set_dot(&mut self, sx: usize, sy: usize, tone: LcarsTone) {
        let cx = sx / 2;
        let cy = sy / 4;
        if cx >= self.cell_w || cy >= self.cell_h {
            return;
        }
        let dx = sx % 2;
        let dy = sy % 4;
        let bit = match (dx, dy) {
            (0, 0) => 0x01,
            (0, 1) => 0x02,
            (0, 2) => 0x04,
            (1, 0) => 0x08,
            (1, 1) => 0x10,
            (1, 2) => 0x20,
            (0, 3) => 0x40,
            (1, 3) => 0x80,
            _ => 0,
        };
        let idx = cy * self.cell_w + cx;
        self.dots[idx] |= bit;
        self.tones[idx] = Some(tone);
    }

    /// Draws a sub-pixel line between `(x0, y0)` and `(x1, y1)` using Bresenham's algorithm.
    pub fn draw_line(&mut self, x0: isize, y0: isize, x1: isize, y1: isize, tone: LcarsTone) {
        let mut x = x0;
        let mut y = y0;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            if x >= 0 && y >= 0 {
                self.set_dot(x as usize, y as usize, tone);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Blits non-empty Braille cells onto the target `Canvas` at `(dest_x, dest_y)`.
    pub fn blit_to(&self, canvas: &mut Canvas, dest_x: usize, dest_y: usize) {
        for cy in 0..self.cell_h {
            for cx in 0..self.cell_w {
                let idx = cy * self.cell_w + cx;
                let mask = self.dots[idx];
                if mask == 0 {
                    continue;
                }
                let ch = char::from_u32(0x2800 + mask as u32).unwrap_or(' ');
                let tone = self.tones[idx].unwrap_or(LcarsTone::CoolBlue);
                canvas.put_char(
                    dest_x + cx,
                    dest_y + cy,
                    ch,
                    ColorSpec::Tone(tone),
                    ColorSpec::Reset,
                );
            }
        }
    }
}
