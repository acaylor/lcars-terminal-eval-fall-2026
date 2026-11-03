use std::env;

/// Terminal color capability level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Ansi256,
    Ansi16,
    Monochrome,
}

impl ColorMode {
    /// Detects terminal color capabilities from standard environment variables.
    pub fn detect() -> Self {
        Self::detect_from_env(
            env::var("NO_COLOR").ok().as_deref(),
            env::var("COLORTERM").ok().as_deref(),
            env::var("TERM").ok().as_deref(),
        )
    }

    pub fn detect_from_env(
        no_color: Option<&str>,
        colorterm: Option<&str>,
        term: Option<&str>,
    ) -> Self {
        if let Some(nc) = no_color {
            if !nc.is_empty() {
                return ColorMode::Monochrome;
            }
        }
        if let Some(t) = term {
            if t.eq_ignore_ascii_case("dumb") {
                return ColorMode::Monochrome;
            }
        }
        if let Some(ct) = colorterm {
            let ct_low = ct.to_ascii_lowercase();
            if ct_low.contains("truecolor") || ct_low.contains("24bit") {
                return ColorMode::TrueColor;
            }
        }
        if let Some(t) = term {
            let t_low = t.to_ascii_lowercase();
            if t_low.contains("direct") || t_low.contains("truecolor") {
                return ColorMode::TrueColor;
            }
            if t_low.contains("256color") || t_low.contains("kitty") || t_low.contains("alacritty")
            {
                return ColorMode::Ansi256;
            }
        }
        ColorMode::Ansi16
    }

    pub fn cycle_next(self) -> Self {
        match self {
            ColorMode::TrueColor => ColorMode::Ansi256,
            ColorMode::Ansi256 => ColorMode::Ansi16,
            ColorMode::Ansi16 => ColorMode::Monochrome,
            ColorMode::Monochrome => ColorMode::TrueColor,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ColorMode::TrueColor => "24-BIT TRUECOLOR",
            ColorMode::Ansi256 => "256-COLOR PALETTE",
            ColorMode::Ansi16 => "16-COLOR ANSI",
            ColorMode::Monochrome => "MONOCHROME",
        }
    }
}

/// Terminal glyph capability mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphMode {
    Unicode,
    Ascii,
}

impl GlyphMode {
    pub fn detect() -> Self {
        Self::detect_from_env(
            env::var("LC_ALL").ok().as_deref(),
            env::var("LC_CTYPE").ok().as_deref(),
            env::var("LANG").ok().as_deref(),
        )
    }

    pub fn detect_from_env(
        lc_all: Option<&str>,
        lc_ctype: Option<&str>,
        lang: Option<&str>,
    ) -> Self {
        let active = lc_all
            .filter(|s| !s.is_empty())
            .or_else(|| lc_ctype.filter(|s| !s.is_empty()))
            .or_else(|| lang.filter(|s| !s.is_empty()));

        if let Some(loc) = active {
            let up = loc.to_ascii_uppercase();
            if up == "C" || up == "POSIX" || up.contains("ASCII") {
                return GlyphMode::Ascii;
            }
            if up.contains("UTF-8") || up.contains("UTF8") {
                return GlyphMode::Unicode;
            }
        }
        // Modern Linux and macOS terminals default to UTF-8 unless restricted by C/POSIX
        GlyphMode::Unicode
    }

    pub fn toggle(self) -> Self {
        match self {
            GlyphMode::Unicode => GlyphMode::Ascii,
            GlyphMode::Ascii => GlyphMode::Unicode,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GlyphMode::Unicode => "UNICODE HI-RES",
            GlyphMode::Ascii => "ASCII FALLBACK",
        }
    }
}

/// Restrained Starfleet LCARS palette: warm orange, golden amber, coral salmon,
/// soft lavender/periwinkle, and cool blue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LcarsTone {
    Orange,
    Amber,
    Salmon,
    Lavender,
    Periwinkle,
    CoolBlue,
    IceBlue,
    Cream,
    Muted,
    TrackDark,
    CutoutDark,
}

impl LcarsTone {
    pub const fn rgb(self) -> (u8, u8, u8) {
        match self {
            LcarsTone::Orange => (255, 153, 51),      // #FF9933 Warm Orange
            LcarsTone::Amber => (255, 204, 102),      // #FFCC66 Golden Amber
            LcarsTone::Salmon => (255, 119, 102),     // #FF7766 Coral Salmon
            LcarsTone::Lavender => (204, 153, 204),   // #CC99CC Soft Lavender
            LcarsTone::Periwinkle => (153, 153, 255), // #9999FF Periwinkle
            LcarsTone::CoolBlue => (102, 178, 255),   // #66B2FF Cool Blue
            LcarsTone::IceBlue => (153, 214, 255),    // #99D6FF Ice Blue
            LcarsTone::Cream => (255, 242, 214),      // #FFF2D6 Warm Cream
            LcarsTone::Muted => (130, 142, 168),      // #828EA8 Muted Slate
            LcarsTone::TrackDark => (32, 38, 54),     // #202636 Recessed Track
            LcarsTone::CutoutDark => (10, 12, 18),    // #0A0C12 Stencil Cutout
        }
    }

    pub const fn ansi256(self) -> u8 {
        match self {
            LcarsTone::Orange => 208,
            LcarsTone::Amber => 221,
            LcarsTone::Salmon => 209,
            LcarsTone::Lavender => 176,
            LcarsTone::Periwinkle => 141,
            LcarsTone::CoolBlue => 75,
            LcarsTone::IceBlue => 117,
            LcarsTone::Cream => 230,
            LcarsTone::Muted => 103,
            LcarsTone::TrackDark => 236,
            LcarsTone::CutoutDark => 232,
        }
    }

    pub const fn ansi16_fg(self) -> u8 {
        match self {
            LcarsTone::Orange => 33,      // Yellow/Orange
            LcarsTone::Amber => 93,       // Bright Yellow
            LcarsTone::Salmon => 91,      // Bright Red
            LcarsTone::Lavender => 35,    // Magenta
            LcarsTone::Periwinkle => 95,  // Bright Magenta
            LcarsTone::CoolBlue => 94,    // Bright Blue
            LcarsTone::IceBlue => 96,     // Bright Cyan
            LcarsTone::Cream => 97,       // Bright White
            LcarsTone::Muted => 37,       // White/Gray
            LcarsTone::TrackDark => 90,   // Bright Black (Dark Gray)
            LcarsTone::CutoutDark => 30,  // Black
        }
    }

    pub const fn ansi16_bg(self) -> u8 {
        self.ansi16_fg() + 10
    }
}

/// Color specification for a cell foreground or background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSpec {
    #[default]
    Reset,
    Tone(LcarsTone),
    /// Smooth 24-bit interpolation between two `LcarsTone`s (`t` in `0..=255`).
    /// Degrades gracefully to the closer `LcarsTone` when TrueColor is unavailable.
    Gradient(LcarsTone, LcarsTone, u8),
}

impl ColorSpec {
    pub fn interpolate(a: LcarsTone, b: LcarsTone, ratio: f32) -> Self {
        let t = (ratio.clamp(0.0, 1.0) * 255.0).round() as u8;
        if t == 0 {
            ColorSpec::Tone(a)
        } else if t == 255 {
            ColorSpec::Tone(b)
        } else {
            ColorSpec::Gradient(a, b, t)
        }
    }

    pub fn resolve_rgb(self) -> Option<(u8, u8, u8)> {
        match self {
            ColorSpec::Reset => None,
            ColorSpec::Tone(t) => Some(t.rgb()),
            ColorSpec::Gradient(a, b, t) => {
                let (r1, g1, b1) = a.rgb();
                let (r2, g2, b2) = b.rgb();
                let mix = |c1: u8, c2: u8| -> u8 {
                    let v = (c1 as u32 * (255 - t as u32) + c2 as u32 * (t as u32) + 127) / 255;
                    v as u8
                };
                Some((mix(r1, r2), mix(g1, g2), mix(b1, b2)))
            }
        }
    }

    pub fn fallback_tone(self) -> Option<LcarsTone> {
        match self {
            ColorSpec::Reset => None,
            ColorSpec::Tone(t) => Some(t),
            ColorSpec::Gradient(a, b, t) => {
                if t < 128 {
                    Some(a)
                } else {
                    Some(b)
                }
            }
        }
    }
}
