//! The LCARS silhouette: a thick top bar sweeping through an elbow into a
//! segmented vertical rail, a thin bottom bar, and the pill/sweep primitives
//! the views build on. Everything is drawn with block glyphs in the
//! foreground colour so the silhouette survives in monochrome.

use crate::term::buffer::fit;
use crate::term::{Buffer, Color, Glyphs, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// One coloured segment of a horizontal bar.
#[derive(Clone, Debug)]
pub struct Segment {
    pub color: Color,
    pub label: String,
    /// Columns; the last segment stretches to fill the bar.
    pub width: i32,
}

impl Segment {
    pub fn new(color: Color, label: impl Into<String>, width: i32) -> Segment {
        Segment {
            color,
            label: label.into(),
            width,
        }
    }
}

/// One block of the vertical rail.
#[derive(Clone, Debug)]
pub struct RailBlock {
    pub color: Color,
    pub weight: f32,
    pub lines: Vec<String>,
    /// Optional 0..=1 fill drawn from the bottom in a brighter shade.
    pub fill: Option<f32>,
}

impl RailBlock {
    pub fn new(color: Color, weight: f32, lines: &[&str]) -> RailBlock {
        RailBlock {
            color,
            weight,
            lines: lines.iter().map(|s| s.to_string()).collect(),
            fill: None,
        }
    }
    pub fn with_fill(mut self, f: Option<f32>) -> RailBlock {
        self.fill = f;
        self
    }
}

pub struct FrameSpec<'a> {
    pub side: Side,
    pub rail_w: i32,
    pub top: &'a [Segment],
    pub bottom: &'a [Segment],
    pub rail: &'a [RailBlock],
    /// Position 0..1 of the scanner highlight travelling along the top bar.
    pub scan: Option<f32>,
}

/// Draw the full frame. Content area is returned as (x0, y0, x1, y1) with
/// exclusive upper bounds, already keeping clear of the inner fillets.
pub fn draw(buf: &mut Buffer, g: &Glyphs, spec: &FrameSpec) -> (i32, i32, i32, i32) {
    let w = buf.width() as i32;
    let h = buf.height() as i32;
    let rw = spec.rail_w.min(w / 3).max(1);
    let mirror = spec.side == Side::Right;
    // Logical x runs away from the elbow; `px` maps it onto the buffer.
    let px = |x: i32| if mirror { w - 1 - x } else { x };
    let gl = |c: char| if mirror { g.mirror(c) } else { c };

    // Top bar: row 0 is half-height for a rounded upper edge, row 1 is solid.
    let top_segments = layout_segments(spec.top, w);
    for (i, (x0, x1, seg)) in top_segments.iter().enumerate() {
        let st = Style::fg(seg.color);
        for x in *x0..*x1 {
            let (c0, c1) = if i == 0 && x == 0 {
                (' ', gl(g.right))
            } else if i == 0 && x == 1 {
                (gl(g.q_lr), g.full)
            } else if i == top_segments.len() - 1 && x == w - 1 {
                (gl(g.q_ll), gl(g.left))
            } else {
                (g.lower, g.full)
            };
            let mut st0 = st;
            if let Some(s) = spec.scan {
                // A three-cell bright pulse sweeping along the top edge.
                let pos = (s * (w as f32)) as i32;
                if x >= pos && x < pos + 3 && c0 == g.lower {
                    st0 = Style::fg(seg.color.mix(Color::IVORY, 0.55));
                }
            }
            buf.set(px(x), 0, c0, st0);
            buf.set(px(x), 1, c1, st);
        }
        if !seg.label.is_empty() {
            let inner = (*x1 - *x0 - 2).max(0) as usize;
            let text = fit(&seg.label, inner, g.ellipsis);
            if !text.is_empty() {
                let lab = Style::label(seg.color);
                let tw = crate::term::buffer::str_width(&text) as i32;
                let start = if mirror { x0 + 1 } else { x1 - 1 - tw };
                // In mirrored frames the logical axis runs right-to-left, so
                // write the text at its physical start to keep it readable.
                let phys = if mirror { px(start + tw - 1) } else { px(start) };
                buf.put_str(phys, 1, &text, lab);
            }
        }
    }

    // Bottom bar: one solid row with key hints.
    let by = h - 1;
    let bottom_segments = layout_segments(spec.bottom, w);
    for (i, (x0, x1, seg)) in bottom_segments.iter().enumerate() {
        let st = Style::fg(seg.color);
        for x in *x0..*x1 {
            let c = if i == 0 && x == 0 {
                gl(g.q_ur)
            } else if i == bottom_segments.len() - 1 && x == w - 1 {
                gl(g.left)
            } else {
                g.full
            };
            buf.set(px(x), by, c, st);
        }
        if !seg.label.is_empty() {
            let inner = (*x1 - *x0 - 2).max(0) as usize;
            let text = fit(&seg.label, inner, g.ellipsis);
            if !text.is_empty() {
                let tw = crate::term::buffer::str_width(&text) as i32;
                let start = if mirror { x0 + 1 } else { x1 - 1 - tw };
                let phys = if mirror { px(start + tw - 1) } else { px(start) };
                buf.put_str(phys, by, &text, Style::label(seg.color));
            }
        }
    }

    // Rail: rows 2 ..= h-2, blocks separated by one-row gaps. The first block
    // continues the top elbow colour, the last one the bottom bar colour.
    let ry0 = 2;
    let ry1 = h - 1; // exclusive
    let rows = (ry1 - ry0).max(0);
    let blocks = layout_blocks(spec.rail, rows);
    for (b0, b1, block) in &blocks {
        let base = Style::fg(block.color);
        for y in *b0..*b1 {
            let st = match block.fill {
                Some(f) => {
                    let filled_rows = ((f.clamp(0.0, 1.0)) * (*b1 - *b0) as f32).round() as i32;
                    if y >= *b1 - filled_rows {
                        Style::fg(block.color.mix(Color::IVORY, 0.35))
                    } else {
                        Style::fg(block.color.scale(0.62))
                    }
                }
                None => base,
            };
            for x in 0..rw {
                buf.set(px(x), ry0 + y, g.full, st);
            }
        }
        // Labels sit at the bottom of the block, flush with its inner edge.
        let n = block.lines.len() as i32;
        let avail = *b1 - *b0;
        for (i, line) in block.lines.iter().enumerate() {
            let y = ry0 + *b1 - n + i as i32;
            if y < ry0 + *b0 || avail < n {
                continue;
            }
            let text = fit(line, (rw - 2).max(0) as usize, g.ellipsis);
            let tw = crate::term::buffer::str_width(&text) as i32;
            let start = if mirror { 1 } else { rw - 1 - tw };
            let phys = if mirror { px(start + tw - 1) } else { px(start) };
            buf.put_str(phys, y, &text, Style::label(block.color));
        }
    }

    // Inner fillets where the bars meet the rail.
    if let Some((_, _, first)) = blocks.first() {
        let st = Style::fg(first.color);
        buf.set(px(rw), 2, gl(g.t_ul), st);
        buf.set(px(rw + 1), 2, g.upper, st);
    }
    if let Some((_, _, last)) = blocks.last() {
        let st = Style::fg(last.color);
        buf.set(px(rw), h - 2, gl(g.t_ll), st);
        buf.set(px(rw + 1), h - 2, g.lower, st);
    }

    let cx0 = rw + 3;
    let cx1 = w - 2;
    if mirror {
        (w - cx1, 3, w - cx0, h - 2)
    } else {
        (cx0, 3, cx1, h - 2)
    }
}

/// Distribute segments across `w` columns with one-column gaps; the final
/// segment absorbs whatever remains. Returns (x0, x1 exclusive, segment).
fn layout_segments(segs: &[Segment], w: i32) -> Vec<(i32, i32, &Segment)> {
    let mut out = Vec::new();
    let mut x = 0;
    for (i, s) in segs.iter().enumerate() {
        let last = i == segs.len() - 1;
        let mut x1 = if last { w } else { (x + s.width).min(w) };
        if !last && x1 > w - 4 {
            x1 = w;
        }
        if x >= w {
            break;
        }
        out.push((x, x1, s));
        if x1 >= w {
            break;
        }
        x = x1 + 1;
    }
    out
}

/// Distribute rail blocks over `rows` rows by weight, with one-row gaps.
fn layout_blocks(blocks: &[RailBlock], rows: i32) -> Vec<(i32, i32, &RailBlock)> {
    if blocks.is_empty() || rows <= 0 {
        return Vec::new();
    }
    let n = blocks.len() as i32;
    let gaps = n - 1;
    let usable = rows - gaps;
    if usable < n {
        // Too short for every block: draw as many as fit, one row each.
        let mut out = Vec::new();
        let mut y = 0;
        for b in blocks {
            if y >= rows {
                break;
            }
            out.push((y, y + 1, b));
            y += 2;
        }
        return out;
    }
    let total_w: f32 = blocks.iter().map(|b| b.weight.max(0.1)).sum();
    let mut heights: Vec<i32> = blocks
        .iter()
        .map(|b| ((b.weight.max(0.1) / total_w) * usable as f32).floor().max(1.0) as i32)
        .collect();
    let mut leftover = usable - heights.iter().sum::<i32>();
    // Hand remaining rows to the heaviest blocks first, which keeps the
    // deliberate asymmetry of the rail intact.
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    order.sort_by(|a, b| {
        blocks[*b]
            .weight
            .partial_cmp(&blocks[*a].weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut i = 0;
    while leftover > 0 {
        heights[order[i % order.len()]] += 1;
        leftover -= 1;
        i += 1;
    }
    let mut out = Vec::new();
    let mut y = 0;
    for (b, hgt) in blocks.iter().zip(heights) {
        out.push((y, y + hgt, b));
        y += hgt + 1;
    }
    out
}

/// A pill: text on a solid colour with half-block end caps. Returns next x.
pub fn pill(buf: &mut Buffer, g: &Glyphs, x: i32, y: i32, text: &str, color: Color) -> i32 {
    let st = Style::fg(color);
    buf.set(x, y, g.right, st);
    let end = buf.put_str(x + 1, y, text, Style::label(color));
    buf.set(end, y, g.left, st);
    end + 1
}

/// A section head: a pill followed by a thin sweep line to `x1` (exclusive)
/// ending in a small cap. This is the LCARS way of titling a region without
/// boxing it.
pub fn sweep(buf: &mut Buffer, g: &Glyphs, x: i32, y: i32, x1: i32, text: &str, color: Color) {
    let after = pill(buf, g, x, y, text, color);
    let st = Style::fg(color);
    for xx in after + 1..x1 - 1 {
        buf.set(xx, y, g.upper, st);
    }
    if x1 - 1 > after {
        buf.set(x1 - 1, y, g.q_ul, st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_fill_width_with_gaps() {
        let segs = vec![
            Segment::new(Color::ORANGE, "A", 10),
            Segment::new(Color::BLUE, "B", 10),
            Segment::new(Color::RED, "", 0),
        ];
        let l = layout_segments(&segs, 40);
        assert_eq!(
            l.iter().map(|(a, b, _)| (*a, *b)).collect::<Vec<_>>(),
            vec![(0, 10), (11, 21), (22, 40)]
        );
        // Narrow bars drop segments that cannot fit.
        let l = layout_segments(&segs, 12);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].1, 12);
    }

    #[test]
    fn blocks_cover_rows_with_single_gaps() {
        let blocks = vec![
            RailBlock::new(Color::ORANGE, 2.0, &[]),
            RailBlock::new(Color::BLUE, 1.0, &[]),
            RailBlock::new(Color::RED, 1.0, &[]),
        ];
        let l = layout_blocks(&blocks, 20);
        assert_eq!(l[0].0, 0);
        assert_eq!(l.last().unwrap().1, 20);
        for pair in l.windows(2) {
            assert_eq!(pair[1].0, pair[0].1 + 1);
        }
        assert!(l[0].1 - l[0].0 > l[1].1 - l[1].0);
        assert_eq!(layout_blocks(&blocks, 3).len(), 2);
        assert!(layout_blocks(&blocks, 0).is_empty());
    }

    #[test]
    fn frame_draws_on_any_size_and_mirrors() {
        let top = vec![
            Segment::new(Color::ORANGE, "", 20),
            Segment::new(Color::BLUE, "HOST", 0),
        ];
        let bottom = vec![
            Segment::new(Color::ORANGE, "", 20),
            Segment::new(Color::BLUE, "Q QUIT", 0),
        ];
        let rail = vec![
            RailBlock::new(Color::ORANGE, 1.0, &["ENG"]),
            RailBlock::new(Color::BLUE, 1.0, &["X"]).with_fill(Some(0.5)),
        ];
        for (w, h) in [(120, 36), (80, 24), (20, 6), (3, 2), (1, 1), (0, 0), (200, 60)] {
            for side in [Side::Left, Side::Right] {
                let mut b = Buffer::new(w, h);
                let spec = FrameSpec {
                    side,
                    rail_w: 12,
                    top: &top,
                    bottom: &bottom,
                    rail: &rail,
                    scan: Some(0.3),
                };
                draw(&mut b, &Glyphs::unicode(), &spec);
                draw(&mut b, &Glyphs::ascii(), &spec);
            }
        }
        let mut b = Buffer::new(60, 12);
        let spec = FrameSpec {
            side: Side::Right,
            rail_w: 10,
            top: &top,
            bottom: &bottom,
            rail: &rail,
            scan: None,
        };
        draw(&mut b, &Glyphs::unicode(), &spec);
        let rows = b.rows_text();
        assert!(rows[1].ends_with('▌'), "{:?}", rows[1]);
        assert!(rows[1].starts_with('▐'), "{:?}", rows[1]);
        assert!(rows[1].contains("HOST"));
        assert!(rows[11].contains("Q QUIT"));
        assert_eq!(b.get(59, 5).unwrap().ch, '█');
        assert_eq!(b.get(0, 5).unwrap().ch, ' ');
    }

    #[test]
    fn pill_and_sweep() {
        let mut b = Buffer::new(20, 1);
        sweep(&mut b, &Glyphs::unicode(), 0, 0, 20, "CPU", Color::LAVENDER);
        assert_eq!(b.rows_text()[0], "▐CPU▌ ▀▀▀▀▀▀▀▀▀▀▀▀▀▘");
    }
}
