#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub const BLACK: Rgb = Rgb(0, 0, 0);
pub const ORANGE: Rgb = Rgb(255, 156, 26);
pub const AMBER: Rgb = Rgb(247, 201, 72);
pub const SALMON: Rgb = Rgb(255, 122, 110);
pub const LAVENDER: Rgb = Rgb(183, 160, 232);
pub const BLUE: Rgb = Rgb(91, 141, 239);
pub const BLUE_DEEP: Rgb = Rgb(36, 64, 120);
pub const PEACH: Rgb = Rgb(255, 176, 122);
pub const INK: Rgb = Rgb(22, 14, 8);
pub const CREAM: Rgb = Rgb(244, 232, 212);
pub const DIM: Rgb = Rgb(120, 104, 84);
pub const SCAN: Rgb = Rgb(255, 236, 190);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    True,
    Ansi16,
    Mono,
}

pub fn detect_color() -> ColorMode {
    let mono = env_flag("LCARS_MONO") || env_flag("NO_COLOR");
    let term = std::env::var("TERM").unwrap_or_default();
    if mono || term == "dumb" {
        return ColorMode::Mono;
    }
    if env_flag("LCARS_TRUECOLOR") {
        return ColorMode::True;
    }
    let ct = std::env::var("COLORTERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ct.contains("truecolor") || ct.contains("24bit") || term.contains("truecolor") {
        ColorMode::True
    } else {
        ColorMode::Ansi16
    }
}

pub fn detect_unicode() -> bool {
    if env_flag("LCARS_ASCII") {
        return false;
    }
    if std::env::var("TERM").unwrap_or_default() == "dumb" {
        return false;
    }
    let lang = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_CTYPE"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    if lang.is_empty() {
        return true;
    }
    let l = lang.to_ascii_lowercase();
    l.contains("utf-8") || l.contains("utf8")
}

fn env_flag(key: &str) -> bool {
    matches!(
        std::env::var(key).ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

pub fn ink_for(bg: Rgb) -> Rgb {
    let lum = (bg.0 as u16) * 3 + (bg.1 as u16) * 6 + (bg.2 as u16);
    if lum > 1100 {
        INK
    } else {
        CREAM
    }
}

pub fn shape_char(bg: Rgb, ch: char) -> char {
    if bg == BLACK && (ch == ' ' || ch == '\0') {
        return ' ';
    }
    match bg {
        ORANGE => 'O',
        AMBER => 'A',
        SALMON => 'S',
        LAVENDER => 'L',
        BLUE => 'B',
        BLUE_DEEP => 'D',
        PEACH => 'P',
        SCAN => 'W',
        CREAM => 'C',
        BLACK => '#',
        _ => '*',
    }
}

pub fn ansi_sgr(mode: ColorMode, fg: Rgb, bg: Rgb) -> String {
    match mode {
        ColorMode::True => format!(
            "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m",
            fg.0, fg.1, fg.2, bg.0, bg.1, bg.2
        ),
        ColorMode::Ansi16 => format!("\x1b[{};{}m", ansi_fg(fg), ansi_bg(bg)),
        ColorMode::Mono => mono_sgr(fg, bg),
    }
}

fn mono_sgr(fg: Rgb, bg: Rgb) -> String {
    if bg == BLACK {
        if fg == DIM {
            "\x1b[0;2m".into()
        } else if fg == AMBER || fg == SCAN || fg == CREAM {
            "\x1b[0;1m".into()
        } else {
            "\x1b[0m".into()
        }
    } else if bg == BLUE || bg == BLUE_DEEP || bg == LAVENDER {
        "\x1b[0;2;7m".into()
    } else if bg == AMBER || bg == ORANGE || bg == SCAN {
        "\x1b[0;1;7m".into()
    } else {
        "\x1b[0;7m".into()
    }
}

fn ansi_fg(c: Rgb) -> u8 {
    match c {
        ORANGE | AMBER | PEACH | SCAN => 93,
        SALMON => 91,
        LAVENDER => 95,
        BLUE => 94,
        BLUE_DEEP => 34,
        INK => 30,
        DIM => 90,
        CREAM => 97,
        BLACK => 30,
        _ => 37,
    }
}

fn ansi_bg(c: Rgb) -> u8 {
    if c == BLACK {
        40
    } else {
        ansi_fg(c) + 10
    }
}
