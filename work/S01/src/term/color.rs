//! Colour model and the restrained LCARS palette.

use std::fmt::Write;

/// A colour as the buffer stores it. Quantisation to the terminal's real
/// capability happens only at output time, so views never care about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Rgb(u8, u8, u8),
}

impl Color {
    pub const BLACK: Color = Color::Rgb(0, 0, 0);
    pub const ORANGE: Color = Color::Rgb(255, 153, 0);
    pub const AMBER: Color = Color::Rgb(255, 204, 102);
    pub const SALMON: Color = Color::Rgb(238, 140, 110);
    pub const LAVENDER: Color = Color::Rgb(204, 153, 204);
    pub const BLUE: Color = Color::Rgb(153, 153, 255);
    pub const DEEP_BLUE: Color = Color::Rgb(102, 102, 204);
    pub const RED: Color = Color::Rgb(220, 70, 60);
    pub const IVORY: Color = Color::Rgb(245, 235, 215);
    pub const DIM: Color = Color::Rgb(110, 105, 140);
    pub const SHADOW: Color = Color::Rgb(45, 42, 70);

    /// Scale brightness; `k` of 1.0 is identity, below darkens, above lightens.
    pub fn scale(self, k: f32) -> Color {
        match self {
            Color::Default => Color::Default,
            Color::Rgb(r, g, b) => {
                let f = |c: u8| ((c as f32) * k).round().clamp(0.0, 255.0) as u8;
                Color::Rgb(f(r), f(g), f(b))
            }
        }
    }

    /// Linear blend from `self` (t=0) to `other` (t=1); used for gradients.
    pub fn mix(self, other: Color, t: f32) -> Color {
        match (self, other) {
            (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
                let t = t.clamp(0.0, 1.0);
                let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
                Color::Rgb(f(r1, r2), f(g1, g2), f(b1, b2))
            }
            (a, _) => a,
        }
    }

    /// Cool blue -> amber -> orange -> red ramp used by load-driven elements.
    pub fn heat(t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        if t < 0.5 {
            Color::BLUE.mix(Color::AMBER, t * 2.0)
        } else if t < 0.8 {
            Color::AMBER.mix(Color::ORANGE, (t - 0.5) / 0.3)
        } else {
            Color::ORANGE.mix(Color::RED, (t - 0.8) / 0.2)
        }
    }

    /// Nearest entry in the xterm 256-colour cube / grey ramp.
    pub fn to_ansi256(self) -> Option<u8> {
        match self {
            Color::Default => None,
            Color::Rgb(r, g, b) => {
                // Grey ramp if the channels are close together.
                if r.abs_diff(g) < 10 && g.abs_diff(b) < 10 && r.abs_diff(b) < 10 {
                    if r < 8 {
                        return Some(16);
                    }
                    if r > 238 {
                        return Some(231);
                    }
                    return Some(232 + ((r as u16 - 8) / 10) as u8);
                }
                let q = |c: u8| -> u8 {
                    if c < 48 {
                        0
                    } else if c < 115 {
                        1
                    } else {
                        ((c as u16 - 35) / 40) as u8
                    }
                };
                Some(16 + 36 * q(r) + 6 * q(g) + q(b))
            }
        }
    }
}

/// What the output stage is allowed to emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Ansi256,
    Mono,
}

impl ColorMode {
    /// Detect from the environment. True colour is a progressive enhancement:
    /// anything we cannot identify degrades to 256 colours, and `NO_COLOR`
    /// or a dumb terminal degrades to monochrome.
    pub fn detect(env: impl Fn(&str) -> Option<String>) -> ColorMode {
        if env("NO_COLOR").map(|v| !v.is_empty()).unwrap_or(false) {
            return ColorMode::Mono;
        }
        let term = env("TERM").unwrap_or_default();
        if term == "dumb" || term.is_empty() {
            return ColorMode::Mono;
        }
        let colorterm = env("COLORTERM").unwrap_or_default().to_ascii_lowercase();
        if colorterm == "truecolor" || colorterm == "24bit" {
            return ColorMode::TrueColor;
        }
        // Terminals known to support 24-bit colour without advertising it.
        let program = env("TERM_PROGRAM").unwrap_or_default();
        if matches!(
            program.as_str(),
            "iTerm.app" | "WezTerm" | "vscode" | "ghostty" | "Hyper" | "kitty"
        ) || term.starts_with("xterm-kitty")
            || term.contains("direct")
        {
            return ColorMode::TrueColor;
        }
        ColorMode::Ansi256
    }

    pub fn label(self) -> &'static str {
        match self {
            ColorMode::TrueColor => "24-BIT",
            ColorMode::Ansi256 => "256",
            ColorMode::Mono => "MONO",
        }
    }

    /// Append the SGR parameters for a foreground colour.
    pub fn write_fg(self, c: Color, out: &mut String) {
        match (self, c) {
            (_, Color::Default) | (ColorMode::Mono, _) => out.push_str("39"),
            (ColorMode::TrueColor, Color::Rgb(r, g, b)) => {
                let _ = write!(out, "38;2;{r};{g};{b}");
            }
            (ColorMode::Ansi256, c) => {
                let _ = write!(out, "38;5;{}", c.to_ansi256().unwrap_or(7));
            }
        }
    }

    /// Append the SGR parameters for a background colour.
    pub fn write_bg(self, c: Color, out: &mut String) {
        match (self, c) {
            (_, Color::Default) | (ColorMode::Mono, _) => out.push_str("49"),
            (ColorMode::TrueColor, Color::Rgb(r, g, b)) => {
                let _ = write!(out, "48;2;{r};{g};{b}");
            }
            (ColorMode::Ansi256, c) => {
                let _ = write!(out, "48;5;{}", c.to_ansi256().unwrap_or(0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_prefers_no_color_then_colorterm() {
        let env = |pairs: &'static [(&str, &str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(
            ColorMode::detect(env(&[("COLORTERM", "truecolor"), ("TERM", "xterm")])),
            ColorMode::TrueColor
        );
        assert_eq!(
            ColorMode::detect(env(&[("COLORTERM", "truecolor"), ("NO_COLOR", "1"), ("TERM", "xterm")])),
            ColorMode::Mono
        );
        assert_eq!(
            ColorMode::detect(env(&[("TERM", "xterm-256color")])),
            ColorMode::Ansi256
        );
        assert_eq!(ColorMode::detect(env(&[("TERM", "dumb")])), ColorMode::Mono);
        assert_eq!(ColorMode::detect(env(&[])), ColorMode::Mono);
    }

    #[test]
    fn ansi256_quantisation_hits_cube_and_grey() {
        assert_eq!(Color::Rgb(0, 0, 0).to_ansi256(), Some(16));
        assert_eq!(Color::Rgb(255, 255, 255).to_ansi256(), Some(231));
        assert_eq!(Color::Rgb(255, 0, 0).to_ansi256(), Some(196));
        assert_eq!(Color::Rgb(128, 128, 128).to_ansi256(), Some(244));
        assert_eq!(Color::Default.to_ansi256(), None);
    }

    #[test]
    fn heat_ramp_is_monotone_in_hue_endpoints() {
        assert_eq!(Color::heat(0.0), Color::BLUE);
        assert_eq!(Color::heat(1.0), Color::RED);
        assert_eq!(Color::BLUE.mix(Color::RED, 0.0), Color::BLUE);
    }
}
