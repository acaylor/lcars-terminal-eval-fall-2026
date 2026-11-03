//! Decoding raw terminal input bytes into keys.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Esc,
    Tab,
    BackTab,
    CtrlC,
    CtrlZ,
    /// Anything we don't act on (arrows, function keys, Alt-combos...).
    Other,
}

/// Decode one read's worth of bytes. A lone ESC is the Escape key; ESC
/// followed by '[' or 'O' starts a control sequence that is consumed whole
/// so e.g. an arrow key's trailing 'A' is never mistaken for a command.
pub fn decode(bytes: &[u8]) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            0x1b => {
                match bytes.get(i + 1) {
                    None => {
                        keys.push(Key::Esc);
                        i += 1;
                    }
                    Some(b'[') => {
                        // CSI: parameters then a final byte in 0x40..=0x7E.
                        let mut j = i + 2;
                        while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                            j += 1;
                        }
                        let fin = bytes.get(j).copied();
                        keys.push(if fin == Some(b'Z') { Key::BackTab } else { Key::Other });
                        i = j + 1;
                    }
                    Some(b'O') => {
                        keys.push(Key::Other);
                        i += 3;
                    }
                    Some(0x1b) => {
                        keys.push(Key::Esc);
                        i += 1;
                    }
                    Some(_) => {
                        keys.push(Key::Other); // Alt+key
                        i += 2;
                    }
                }
                continue;
            }
            0x03 => keys.push(Key::CtrlC),
            0x1a => keys.push(Key::CtrlZ),
            b'\t' => keys.push(Key::Tab),
            0x20..=0x7e => keys.push(Key::Char(b as char)),
            _ => {
                if b >= 0x80 {
                    // Skip the rest of a UTF-8 sequence.
                    let len = match b {
                        0xc0..=0xdf => 2,
                        0xe0..=0xef => 3,
                        0xf0..=0xf7 => 4,
                        _ => 1,
                    };
                    keys.push(Key::Other);
                    i += len;
                    continue;
                }
                keys.push(Key::Other);
            }
        }
        i += 1;
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_keys() {
        assert_eq!(decode(b"q? p\t"), vec![Key::Char('q'), Key::Char('?'), Key::Char(' '), Key::Char('p'), Key::Tab]);
        assert_eq!(decode(&[0x03, 0x1a]), vec![Key::CtrlC, Key::CtrlZ]);
    }

    #[test]
    fn escape_versus_sequences() {
        assert_eq!(decode(b"\x1b"), vec![Key::Esc]);
        assert_eq!(decode(b"\x1b[A"), vec![Key::Other]);
        assert_eq!(decode(b"\x1b[1;5Cq"), vec![Key::Other, Key::Char('q')]);
        assert_eq!(decode(b"\x1b[Z"), vec![Key::BackTab]);
        assert_eq!(decode(b"\x1bOP"), vec![Key::Other]);
        assert_eq!(decode(b"\x1bx"), vec![Key::Other]);
        assert_eq!(decode(b"\x1b\x1b"), vec![Key::Esc, Key::Esc]);
    }

    #[test]
    fn truncated_and_non_ascii_input_is_safe() {
        assert_eq!(decode(b"\x1b[12"), vec![Key::Other]);
        assert_eq!(decode("é日q".as_bytes()), vec![Key::Other, Key::Other, Key::Char('q')]);
    }
}
