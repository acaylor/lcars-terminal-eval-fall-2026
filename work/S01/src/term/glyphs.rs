//! Glyph sets. Views draw through this table so the same composition can be
//! rendered with Unicode block elements or with a 7-bit ASCII fallback.

#[derive(Clone, Copy, Debug)]
pub struct Glyphs {
    pub unicode: bool,
    pub full: char,
    pub upper: char,
    pub lower: char,
    pub left: char,
    pub right: char,
    /// Quadrants: lower-right, lower-left, upper-right, upper-left.
    pub q_lr: char,
    pub q_ll: char,
    pub q_ur: char,
    pub q_ul: char,
    /// Three-quadrant blocks: missing lower-right, lower-left, upper-right, upper-left.
    pub t_ul: char,
    pub t_ur: char,
    pub t_ll: char,
    pub t_lr: char,
    /// Vertical fill levels, 0..=8 eighths.
    pub levels: [char; 9],
    /// Horizontal fill levels, 0..=8 eighths (left-anchored).
    pub hlevels: [char; 9],
    pub track: char,
    pub ellipsis: &'static str,
    pub bullet: char,
    pub pulse_on: char,
    pub pulse_off: char,
    pub unavailable: &'static str,
}

impl Glyphs {
    pub const fn unicode() -> Glyphs {
        Glyphs {
            unicode: true,
            full: '█',
            upper: '▀',
            lower: '▄',
            left: '▌',
            right: '▐',
            q_lr: '▗',
            q_ll: '▖',
            q_ur: '▝',
            q_ul: '▘',
            t_ul: '▛',
            t_ur: '▜',
            t_ll: '▙',
            t_lr: '▟',
            levels: [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'],
            hlevels: [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'],
            track: '▁',
            ellipsis: "…",
            bullet: '·',
            pulse_on: '◆',
            pulse_off: '◇',
            unavailable: "──",
        }
    }

    pub const fn ascii() -> Glyphs {
        Glyphs {
            unicode: false,
            full: '#',
            upper: '"',
            lower: '_',
            left: '#',
            right: '#',
            q_lr: '.',
            q_ll: '.',
            q_ur: '\'',
            q_ul: '\'',
            t_ul: '#',
            t_ur: '#',
            t_ll: '#',
            t_lr: '#',
            levels: [' ', '.', '.', ':', ':', '+', '+', '#', '#'],
            hlevels: [' ', ' ', ' ', ' ', ':', ':', ':', ':', '#'],
            track: '_',
            ellipsis: "~",
            bullet: '*',
            pulse_on: '*',
            pulse_off: 'o',
            unavailable: "--",
        }
    }

    /// Decide from the locale whether the terminal can be expected to render
    /// block elements. `LC_ALL` overrides `LC_CTYPE`, which overrides `LANG`.
    pub fn detect(env: impl Fn(&str) -> Option<String>) -> Glyphs {
        let locale = env("LC_ALL")
            .filter(|v| !v.is_empty())
            .or_else(|| env("LC_CTYPE").filter(|v| !v.is_empty()))
            .or_else(|| env("LANG"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if locale.contains("utf-8") || locale.contains("utf8") {
            Glyphs::unicode()
        } else {
            Glyphs::ascii()
        }
    }

    /// Data strings (hostnames, process names) pass through untouched in
    /// Unicode mode; in ASCII mode every non-ASCII character becomes `?` so
    /// a terminal without UTF-8 never receives multibyte sequences.
    pub fn text<'a>(&self, s: &'a str) -> std::borrow::Cow<'a, str> {
        if self.unicode || s.is_ascii() {
            std::borrow::Cow::Borrowed(s)
        } else {
            std::borrow::Cow::Owned(s.chars().map(|c| if c.is_ascii() { c } else { '?' }).collect())
        }
    }

    /// Mirror a frame glyph horizontally (used for the right-handed frame).
    pub fn mirror(&self, c: char) -> char {
        if c == self.left {
            self.right
        } else if c == self.right {
            self.left
        } else if c == self.q_lr {
            self.q_ll
        } else if c == self.q_ll {
            self.q_lr
        } else if c == self.q_ur {
            self.q_ul
        } else if c == self.q_ul {
            self.q_ur
        } else if c == self.t_ul {
            self.t_ur
        } else if c == self.t_ur {
            self.t_ul
        } else if c == self.t_ll {
            self.t_lr
        } else if c == self.t_lr {
            self.t_ll
        } else {
            c
        }
    }

    /// Braille cell from a 2x4 dot bitmap given as column bit masks (bit 0 = top).
    pub fn braille(&self, left: u8, right: u8) -> char {
        if !self.unicode {
            // ASCII fallback: height of the taller column as a crude level glyph.
            let h = |m: u8| (0..4).rev().find(|b| m & (1 << b) != 0).map(|b| 4 - b).unwrap_or(0);
            let lvl = h(left).max(h(right));
            return self.levels[lvl * 2];
        }
        const L: [u32; 4] = [0x01, 0x02, 0x04, 0x40];
        const R: [u32; 4] = [0x08, 0x10, 0x20, 0x80];
        let mut bits = 0u32;
        for i in 0..4 {
            if left & (1 << i) != 0 {
                bits |= L[i];
            }
            if right & (1 << i) != 0 {
                bits |= R[i];
            }
        }
        char::from_u32(0x2800 + bits).unwrap_or(' ')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_detection() {
        let env = |pairs: &'static [(&str, &str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert!(Glyphs::detect(env(&[("LANG", "en_US.UTF-8")])).unicode);
        assert!(!Glyphs::detect(env(&[("LANG", "C")])).unicode);
        assert!(!Glyphs::detect(env(&[("LANG", "en_US.UTF-8"), ("LC_ALL", "POSIX")])).unicode);
        assert!(!Glyphs::detect(env(&[])).unicode);
    }

    #[test]
    fn ascii_mode_sanitises_data_text() {
        assert_eq!(Glyphs::ascii().text("日本語 proc"), "??? proc");
        assert_eq!(Glyphs::unicode().text("日本語 proc"), "日本語 proc");
        assert_eq!(Glyphs::ascii().text("plain"), "plain");
    }

    #[test]
    fn braille_bits() {
        let g = Glyphs::unicode();
        assert_eq!(g.braille(0, 0), '\u{2800}');
        assert_eq!(g.braille(0b1111, 0b1111), '\u{28FF}');
        assert_eq!(g.braille(0b0001, 0), '\u{2801}');
        assert_eq!(g.braille(0b1000, 0), '\u{2840}');
        assert_eq!(Glyphs::ascii().braille(0b1000, 0), '.');
        assert_eq!(Glyphs::ascii().braille(0b0001, 0), '#');
    }

    #[test]
    fn mirror_is_an_involution() {
        let g = Glyphs::unicode();
        for c in ['▌', '▐', '▗', '▖', '▝', '▘', '▛', '▜', '▙', '▟', 'x'] {
            assert_eq!(g.mirror(g.mirror(c)), c);
        }
    }
}
