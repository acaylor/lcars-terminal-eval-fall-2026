use std::env;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Default,
    Black,
    Ink,
    White,
    Gray,
    Dim,
    Orange,
    Amber,
    Salmon,
    Lavender,
    Blue,
    Track,
}

pub const BOLD: u8 = 1 << 0;
pub const DIM: u8 = 1 << 1;
pub const REVERSE: u8 = 1 << 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorCapability {
    Truecolor,
    Ansi256,
    Ansi16,
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub capability: ColorCapability,
    pub mono: bool,
}

impl Theme {
    pub fn detect(mono: bool) -> Self {
        let capability =
            if env::var_os("NO_COLOR").is_some() || env::var("TERM").is_ok_and(|t| t == "dumb") {
                ColorCapability::Ansi16
            } else if env::var("COLORTERM")
                .map(|v| {
                    let v = v.to_ascii_lowercase();
                    v.contains("truecolor") || v.contains("24bit")
                })
                .unwrap_or(false)
                || env::var("TERM")
                    .map(|v| v.contains("direct") || v.contains("24bit"))
                    .unwrap_or(false)
            {
                ColorCapability::Truecolor
            } else if env::var("TERM")
                .map(|v| v.contains("256color"))
                .unwrap_or(false)
            {
                ColorCapability::Ansi256
            } else {
                ColorCapability::Ansi16
            };
        Self {
            capability,
            mono: mono || env::var_os("NO_COLOR").is_some(),
        }
    }

    pub fn with_capability(capability: ColorCapability, mono: bool) -> Self {
        Self { capability, mono }
    }

    pub fn fg_sgr(&self, color: Ink) -> String {
        if self.mono {
            return mono_fg(color, self.capability).to_string();
        }
        match self.capability {
            ColorCapability::Truecolor => match rgb(color) {
                Some((r, g, b)) => format!("38;2;{r};{g};{b}"),
                None => "39".to_string(),
            },
            ColorCapability::Ansi256 => match ansi256(color) {
                Some(n) => format!("38;5;{n}"),
                None => "39".to_string(),
            },
            ColorCapability::Ansi16 => ansi16_fg(color).to_string(),
        }
    }

    pub fn bg_sgr(&self, color: Ink) -> String {
        if self.mono {
            return mono_bg(color, self.capability).to_string();
        }
        match self.capability {
            ColorCapability::Truecolor => match rgb(color) {
                Some((r, g, b)) => format!("48;2;{r};{g};{b}"),
                None => "49".to_string(),
            },
            ColorCapability::Ansi256 => match ansi256(color) {
                Some(n) => format!("48;5;{n}"),
                None => "49".to_string(),
            },
            ColorCapability::Ansi16 => ansi16_bg(color).to_string(),
        }
    }

    pub fn color_enabled(&self) -> bool {
        !(self.mono && self.capability == ColorCapability::Ansi16)
    }
}

pub fn rgb(color: Ink) -> Option<(u8, u8, u8)> {
    Some(match color {
        Ink::Default => return None,
        Ink::Black => (0, 0, 0),
        Ink::Ink => (10, 9, 15),
        Ink::White => (247, 242, 231),
        Ink::Gray => (163, 158, 174),
        Ink::Dim => (111, 106, 122),
        Ink::Orange => (255, 157, 69),
        Ink::Amber => (255, 201, 120),
        Ink::Salmon => (255, 122, 110),
        Ink::Lavender => (195, 166, 255),
        Ink::Blue => (127, 169, 255),
        Ink::Track => (43, 41, 51),
    })
}

pub fn ansi256(color: Ink) -> Option<u8> {
    Some(match color {
        Ink::Default => return None,
        Ink::Black => 16,
        Ink::Ink => 232,
        Ink::White => 255,
        Ink::Gray => 247,
        Ink::Dim => 241,
        Ink::Orange => 215,
        Ink::Amber => 222,
        Ink::Salmon => 210,
        Ink::Lavender => 183,
        Ink::Blue => 111,
        Ink::Track => 236,
    })
}

pub fn ansi16_fg(color: Ink) -> u8 {
    match color {
        Ink::Default => 39,
        Ink::Black | Ink::Ink => 30,
        Ink::White => 97,
        Ink::Gray => 37,
        Ink::Dim => 90,
        Ink::Orange => 33,
        Ink::Amber => 93,
        Ink::Salmon => 91,
        Ink::Lavender => 95,
        Ink::Blue => 94,
        Ink::Track => 90,
    }
}

pub fn ansi16_bg(color: Ink) -> u8 {
    match color {
        Ink::Default => 49,
        Ink::Black | Ink::Ink => 40,
        Ink::White => 107,
        Ink::Gray => 47,
        Ink::Dim => 100,
        Ink::Orange => 43,
        Ink::Amber => 103,
        Ink::Salmon => 101,
        Ink::Lavender => 105,
        Ink::Blue => 104,
        Ink::Track => 40,
    }
}

fn mono_fg(color: Ink, capability: ColorCapability) -> &'static str {
    match capability {
        ColorCapability::Truecolor => match color {
            Ink::Default => "39",
            Ink::Black | Ink::Ink => "38;2;0;0;0",
            Ink::White | Ink::Orange | Ink::Amber | Ink::Salmon | Ink::Lavender | Ink::Blue => {
                "38;2;240;240;240"
            }
            Ink::Gray => "38;2;190;190;190",
            Ink::Dim => "38;2;135;135;135",
            Ink::Track => "38;2;120;120;120",
        },
        ColorCapability::Ansi256 => match color {
            Ink::Default => "39",
            Ink::Black | Ink::Ink => "38;5;16",
            Ink::White | Ink::Orange | Ink::Amber | Ink::Salmon | Ink::Lavender | Ink::Blue => {
                "38;5;255"
            }
            Ink::Track => "38;5;240",
            Ink::Gray => "38;5;250",
            Ink::Dim => "38;5;243",
        },
        ColorCapability::Ansi16 => match color {
            Ink::Default => "39",
            Ink::Black | Ink::Ink => "30",
            Ink::White | Ink::Orange | Ink::Amber | Ink::Salmon | Ink::Lavender | Ink::Blue => "97",
            Ink::Gray | Ink::Dim | Ink::Track => "37",
        },
    }
}

fn mono_bg(color: Ink, capability: ColorCapability) -> &'static str {
    match capability {
        ColorCapability::Truecolor => match color {
            Ink::Default => "49",
            Ink::Black | Ink::Ink => "48;2;0;0;0",
            Ink::White => "48;2;220;220;220",
            Ink::Gray => "48;2;150;150;150",
            Ink::Dim => "48;2;95;95;95",
            Ink::Orange | Ink::Amber => "48;2;185;185;185",
            Ink::Salmon | Ink::Lavender | Ink::Blue => "48;2;120;120;120",
            Ink::Track => "48;2;30;30;34",
        },
        ColorCapability::Ansi256 => match color {
            Ink::Default => "49",
            Ink::Black | Ink::Ink => "48;5;16",
            Ink::White => "48;5;252",
            Ink::Gray => "48;5;246",
            Ink::Dim => "48;5;239",
            Ink::Orange | Ink::Amber => "48;5;250",
            Ink::Salmon | Ink::Lavender | Ink::Blue => "48;5;243",
            Ink::Track => "48;5;235",
        },
        ColorCapability::Ansi16 => match color {
            Ink::Default => "49",
            Ink::Black | Ink::Ink => "40",
            Ink::White | Ink::Orange | Ink::Amber => "107",
            Ink::Gray | Ink::Salmon | Ink::Lavender | Ink::Blue => "47",
            Ink::Dim | Ink::Track => "100",
        },
    }
}

pub fn unicode_locale() -> bool {
    if let Ok(term) = env::var("TERM") {
        if term == "dumb" {
            return false;
        }
    }
    let locale = env::var("LC_ALL")
        .or_else(|_| env::var("LC_CTYPE"))
        .or_else(|_| env::var("LANG"))
        .unwrap_or_default()
        .to_ascii_uppercase();
    locale.contains("UTF-8") || locale.contains("UTF8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truecolor_and_256_mappings_are_present() {
        let t = Theme::with_capability(ColorCapability::Truecolor, false);
        assert_eq!(t.fg_sgr(Ink::Orange), "38;2;255;157;69");
        let t = Theme::with_capability(ColorCapability::Ansi256, false);
        assert_eq!(t.bg_sgr(Ink::Blue), "48;5;111");
        let t = Theme::with_capability(ColorCapability::Ansi16, false);
        assert_eq!(t.fg_sgr(Ink::Amber), "93");
    }

    #[test]
    fn mono_still_separates_tones() {
        let t = Theme::with_capability(ColorCapability::Ansi256, true);
        assert_ne!(t.bg_sgr(Ink::Orange), t.bg_sgr(Ink::Blue));
        assert_eq!(t.fg_sgr(Ink::Lavender), "38;5;255");
    }
}
