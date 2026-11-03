use super::buffer::Buffer;
use super::color::{Rgb, BLACK};

pub fn big_digit(d: u8) -> &'static [&'static str] {
    match d {
        0 => &B0,
        1 => &B1,
        2 => &B2,
        3 => &B3,
        4 => &B4,
        5 => &B5,
        6 => &B6,
        7 => &B7,
        8 => &B8,
        _ => &B9,
    }
}

pub fn med_digit(d: u8) -> &'static [&'static str] {
    match d {
        0 => &M0,
        1 => &M1,
        2 => &M2,
        3 => &M3,
        4 => &M4,
        5 => &M5,
        6 => &M6,
        7 => &M7,
        8 => &M8,
        _ => &M9,
    }
}

const B0: [&str; 9] = [
    " ###### ", "##    ##", "##    ##", "##    ##", "##    ##", "##    ##", "##    ##", "##    ##",
    " ###### ",
];
const B1: [&str; 9] = [
    "   ##   ", "  ###   ", "   ##   ", "   ##   ", "   ##   ", "   ##   ", "   ##   ", "   ##   ",
    " ###### ",
];
const B2: [&str; 9] = [
    " ###### ", "##    ##", "      ##", "     ## ", "   ##   ", "  ##    ", " ##     ", "##      ",
    "########",
];
const B3: [&str; 9] = [
    " ###### ", "##    ##", "      ##", "      ##", "  ##### ", "      ##", "      ##", "##    ##",
    " ###### ",
];
const B4: [&str; 9] = [
    "     ## ", "    ### ", "   # ## ", "  #  ## ", " #   ## ", "########", "     ## ", "     ## ",
    "     ## ",
];
const B5: [&str; 9] = [
    "########", "##      ", "##      ", "######  ", "      ##", "      ##", "      ##", "##    ##",
    " ###### ",
];
const B6: [&str; 9] = [
    " ###### ", "##      ", "##      ", "######  ", "##    ##", "##    ##", "##    ##", "##    ##",
    " ###### ",
];
const B7: [&str; 9] = [
    "########", "      ##", "     ## ", "    ##  ", "   ##   ", "   ##   ", "   ##   ", "   ##   ",
    "   ##   ",
];
const B8: [&str; 9] = [
    " ###### ", "##    ##", "##    ##", "##    ##", " ###### ", "##    ##", "##    ##", "##    ##",
    " ###### ",
];
const B9: [&str; 9] = [
    " ###### ", "##    ##", "##    ##", "##    ##", " ###### ", "      ##", "      ##", "##    ##",
    " ###### ",
];

pub const COLON: [&str; 9] = [
    "   ", "   ", " # ", " # ", "   ", " # ", " # ", "   ", "   ",
];

const M0: [&str; 5] = [" ### ", "#   #", "#   #", "#   #", " ### "];
const M1: [&str; 5] = ["  #  ", " ##  ", "  #  ", "  #  ", " ### "];
const M2: [&str; 5] = [" ### ", "#   #", "   # ", " #   ", "#####"];
const M3: [&str; 5] = [" ### ", "    #", " ### ", "    #", " ### "];
const M4: [&str; 5] = ["#   #", "#   #", "#####", "    #", "    #"];
const M5: [&str; 5] = ["#####", "#    ", "#### ", "    #", "#### "];
const M6: [&str; 5] = [" ### ", "#    ", "#### ", "#   #", " ### "];
const M7: [&str; 5] = ["#####", "    #", "   # ", "  #  ", "  #  "];
const M8: [&str; 5] = [" ### ", "#   #", " ### ", "#   #", " ### "];
const M9: [&str; 5] = [" ### ", "#   #", " ####", "    #", " ### "];

pub const MCOLON: [&str; 5] = ["   ", " # ", "   ", " # ", "   "];

pub fn blit_cells(
    buf: &mut Buffer,
    x: i32,
    y: i32,
    rows: &[&str],
    color: Rgb,
    unicode: bool,
) -> (i32, i32) {
    let h = rows.len() as i32;
    let w = rows.first().map(|r| r.len()).unwrap_or(0) as i32;
    for (dy, row) in rows.iter().enumerate() {
        for (dx, ch) in row.chars().enumerate() {
            if ch != '#' {
                continue;
            }
            let px = x + dx as i32;
            let py = y + dy as i32;
            if unicode {
                let up = filled(rows, dx as i32, dy as i32 - 1);
                let down = filled(rows, dx as i32, dy as i32 + 1);
                let left = filled(rows, dx as i32 - 1, dy as i32);
                let right = filled(rows, dx as i32 + 1, dy as i32);
                if !up && !left && down && right {
                    buf.set(px, py, '▗', color, BLACK);
                } else if !up && !right && down && left {
                    buf.set(px, py, '▖', color, BLACK);
                } else if !down && !left && up && right {
                    buf.set(px, py, '▝', color, BLACK);
                } else if !down && !right && up && left {
                    buf.set(px, py, '▘', color, BLACK);
                } else {
                    buf.set(px, py, ' ', color, color);
                }
            } else {
                buf.set(px, py, '#', color, BLACK);
            }
        }
    }
    (w, h)
}

fn filled(rows: &[&str], x: i32, y: i32) -> bool {
    if x < 0 || y < 0 {
        return false;
    }
    rows.get(y as usize)
        .and_then(|r| r.chars().nth(x as usize))
        .is_some_and(|c| c == '#')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_glyphs_are_rectangles() {
        for d in 0..10 {
            let g = big_digit(d);
            assert_eq!(g.len(), 9);
            assert!(g.iter().all(|r| r.len() == 8));
            let m = med_digit(d);
            assert_eq!(m.len(), 5);
            assert!(m.iter().all(|r| r.len() == 5));
        }
        assert!(COLON.iter().all(|r| r.len() == 3));
        assert!(MCOLON.iter().all(|r| r.len() == 3));
    }
}
