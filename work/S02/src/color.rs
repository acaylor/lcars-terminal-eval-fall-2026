//! Colors, terminal color-depth detection and down-sampling.
//!
//! Everything is drawn in 24-bit RGB. The emitter converts to whatever the
//! terminal supports at the very last moment, so a reduced palette never
//! changes layout — only fidelity.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Color {
    /// The screen background (black in color modes, terminal default in mono).
    Reset,
    Rgb(u8, u8, u8),
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color::Rgb(r, g, b)
    }

    /// Linear interpolation; `Reset` is treated as black.
    pub fn lerp(self, other: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let (r1, g1, b1) = self.components();
        let (r2, g2, b2) = other.components();
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2))
    }

    pub fn scale(self, f: f32) -> Color {
        Color::Rgb(0, 0, 0).lerp(self, f)
    }

    pub fn components(self) -> (u8, u8, u8) {
        match self {
            Color::Reset => (0, 0, 0),
            Color::Rgb(r, g, b) => (r, g, b),
        }
    }

    /// Relative luminance in 0..=1 (sRGB weights, no gamma — good enough to
    /// decide bold/faint in monochrome mode).
    pub fn luma(self) -> f32 {
        let (r, g, b) = self.components();
        (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Ansi256,
    Ansi16,
    Mono,
}

impl ColorMode {
    pub fn parse(s: &str) -> Option<ColorMode> {
        match s.to_ascii_lowercase().as_str() {
            "truecolor" | "24bit" | "rgb" => Some(ColorMode::TrueColor),
            "256" | "ansi256" => Some(ColorMode::Ansi256),
            "16" | "ansi16" | "ansi" => Some(ColorMode::Ansi16),
            "mono" | "none" | "monochrome" => Some(ColorMode::Mono),
            _ => None,
        }
    }

    /// Detect color depth from the environment. `env` is injected so the
    /// decision table is unit-testable.
    pub fn detect(env: impl Fn(&str) -> Option<String>) -> ColorMode {
        if env("NO_COLOR").is_some_and(|v| !v.is_empty()) {
            return ColorMode::Mono;
        }
        let colorterm = env("COLORTERM").unwrap_or_default().to_ascii_lowercase();
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            return ColorMode::TrueColor;
        }
        let term = env("TERM").unwrap_or_default().to_ascii_lowercase();
        // Terminals known to support 24-bit color even without COLORTERM
        // (common when COLORTERM is dropped by ssh/sudo).
        let tp = env("TERM_PROGRAM").unwrap_or_default();
        if term.contains("direct")
            || term.starts_with("xterm-kitty")
            || term.starts_with("wezterm")
            || term.starts_with("alacritty")
            || term.contains("ghostty")
            || matches!(tp.as_str(), "iTerm.app" | "WezTerm" | "ghostty" | "vscode")
        {
            return ColorMode::TrueColor;
        }
        if term.contains("256") {
            return ColorMode::Ansi256;
        }
        if term.is_empty() || term == "dumb" {
            return ColorMode::Mono;
        }
        ColorMode::Ansi16
    }
}

/// Nearest xterm-256 index (6x6x6 cube or 24-step gray ramp).
pub fn to_ansi256(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let cube_idx = |v: u8| -> usize {
        let mut best = 0;
        for (i, l) in LEVELS.iter().enumerate() {
            if (v as i32 - *l as i32).abs() < (v as i32 - LEVELS[best] as i32).abs() {
                best = i;
            }
        }
        best
    };
    let (ri, gi, bi) = (cube_idx(r), cube_idx(g), cube_idx(b));
    let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let avg = (r as u32 + g as u32 + b as u32) / 3;
    let gray_i = if avg < 8 { 0 } else { ((avg - 8) / 10).min(23) } as u8;
    let gray_v = 8 + 10 * gray_i as u32;
    let d = |c: (u32, u32, u32)| {
        let dr = r as i32 - c.0 as i32;
        let dg = g as i32 - c.1 as i32;
        let db = b as i32 - c.2 as i32;
        dr * dr + dg * dg + db * db
    };
    let cube_d = d((cube.0 as u32, cube.1 as u32, cube.2 as u32));
    let gray_d = d((gray_v, gray_v, gray_v));
    if gray_d < cube_d {
        232 + gray_i
    } else {
        16 + (36 * ri + 6 * gi + bi) as u8
    }
}

/// Nearest of the 16 standard ANSI colors (xterm defaults), returned 0..=15.
pub fn to_ansi16(r: u8, g: u8, b: u8) -> u8 {
    const TABLE: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 0, 0),
        (0, 205, 0),
        (205, 205, 0),
        (0, 0, 238),
        (205, 0, 205),
        (0, 205, 205),
        (229, 229, 229),
        (127, 127, 127),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (92, 92, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    let mut best = 0;
    let mut best_d = i32::MAX;
    for (i, c) in TABLE.iter().enumerate() {
        let dr = r as i32 - c.0 as i32;
        let dg = g as i32 - c.1 as i32;
        let db = b as i32 - c.2 as i32;
        let dist = dr * dr + dg * dg + db * db;
        if dist < best_d {
            best_d = dist;
            best = i;
        }
    }
    best as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn detection_table() {
        assert_eq!(ColorMode::detect(env_of(&[("COLORTERM", "truecolor"), ("TERM", "xterm")])), ColorMode::TrueColor);
        assert_eq!(ColorMode::detect(env_of(&[("TERM", "xterm-256color")])), ColorMode::Ansi256);
        assert_eq!(ColorMode::detect(env_of(&[("TERM", "xterm")])), ColorMode::Ansi16);
        assert_eq!(ColorMode::detect(env_of(&[("TERM", "dumb")])), ColorMode::Mono);
        assert_eq!(ColorMode::detect(env_of(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")])), ColorMode::Mono);
    }

    #[test]
    fn quantizers_hit_exact_palette_entries() {
        assert_eq!(to_ansi256(255, 135, 0), 16 + 36 * 5 + 6 * 2);
        assert_eq!(to_ansi256(0, 0, 0), 16);
        assert_eq!(to_ansi256(128, 128, 128), 244);
        assert_eq!(to_ansi16(255, 0, 0), 9);
        assert_eq!(to_ansi16(250, 250, 250), 15);
    }

    #[test]
    fn lerp_endpoints() {
        let a = Color::rgb(0, 0, 0);
        let b = Color::rgb(200, 100, 50);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert_eq!(a.lerp(b, 0.5), Color::rgb(100, 50, 25));
    }
}
