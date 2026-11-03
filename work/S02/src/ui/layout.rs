//! Size-dependent composition of the command view. The layout is computed
//! separately from drawing so it can be tested for fit and overlap at every
//! terminal size.

use crate::canvas::Rect;
use crate::ui::font::Font;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// Too small for the composition: a minimal readout.
    Tiny,
    /// ~80x24: one "[" frame, fewer ornaments.
    Compact,
    /// >=100x30: double frame, sweep gauge, header telemetry.
    Full,
}

pub fn tier(w: i32, h: i32) -> Tier {
    if w < 44 || h < 14 {
        Tier::Tiny
    } else if w >= 100 && h >= 30 {
        Tier::Full
    } else {
        Tier::Compact
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Cpu,
    Mem,
    Disk,
    Net,
    Load,
    Filler,
}

/// A sidebar block. Rows are inclusive; the first row is drawn as a half
/// row so consecutive blocks are separated by a thin half-cell gap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub y0: i32,
    pub y1: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrimaryLayout {
    pub tier: Tier,
    pub w: i32,
    pub h: i32,
    pub side_w: i32,
    /// Height in rows of the upper frame (Full tier only).
    pub upper: Option<i32>,
    /// First row of the lower frame's top rail.
    pub lower_top: i32,
    pub blocks: Vec<Block>,
    /// Telemetry grid in the upper frame (Full) or right column (Compact).
    pub ledger: Rect,
    pub title: Rect,
    pub clock: Rect,
    pub clock_cell: (i32, i32),
    pub date: Rect,
    pub load: Rect,
    pub load_cell: (i32, i32),
    pub status: Rect,
    pub trace: Rect,
    pub gauge: Option<Rect>,
}

impl PrimaryLayout {
    /// Content rectangles that must never overlap each other.
    pub fn content_rects(&self) -> Vec<(&'static str, Rect)> {
        let mut v = vec![
            ("ledger", self.ledger),
            ("title", self.title),
            ("clock", self.clock),
            ("date", self.date),
            ("load", self.load),
            ("status", self.status),
            ("trace", self.trace),
        ];
        if let Some(g) = self.gauge {
            v.push(("gauge", g));
        }
        v.retain(|(_, r)| !r.is_empty());
        v
    }
}

/// Largest clock digit size whose "00:00:00" fits in `max_w` columns.
fn fit_clock(max_w: i32, mut rows: i32) -> (i32, i32) {
    loop {
        let cols = ((rows as f32) * 0.86).round().max(2.0) as i32;
        if Font::measure("00:00:00", cols) <= max_w || rows <= 3 {
            return (cols, rows);
        }
        rows -= 1;
    }
}

fn sidebar_blocks(kinds: &[BlockKind], top: i32, bottom: i32, min_h: i32, max_h: i32) -> Vec<Block> {
    let avail = bottom - top + 1;
    let n = kinds.len() as i32;
    if n == 0 || avail < min_h {
        return vec![];
    }
    let fit = (avail / min_h).min(n);
    let bh = (avail / fit.max(1)).clamp(min_h, max_h);
    let mut out = Vec::new();
    let mut y = top;
    for &kind in kinds.iter().take(fit as usize) {
        out.push(Block { kind, y0: y, y1: y + bh - 1 });
        y += bh;
    }
    if bottom - y + 1 >= 3 {
        out.push(Block { kind: BlockKind::Filler, y0: y, y1: bottom });
    } else if let Some(last) = out.last_mut() {
        last.y1 = bottom; // a sliver would render as a stray half-row
    }
    out
}

const KINDS: [BlockKind; 5] = [BlockKind::Cpu, BlockKind::Mem, BlockKind::Disk, BlockKind::Net, BlockKind::Load];

pub fn primary(w: i32, h: i32) -> PrimaryLayout {
    match tier(w, h) {
        Tier::Full => full(w, h),
        Tier::Compact => compact(w, h),
        Tier::Tiny => PrimaryLayout {
            tier: Tier::Tiny,
            w,
            h,
            side_w: 0,
            upper: None,
            lower_top: 0,
            blocks: vec![],
            ledger: Rect::default(),
            title: Rect::default(),
            clock: Rect::default(),
            clock_cell: (0, 0),
            date: Rect::default(),
            load: Rect::default(),
            load_cell: (0, 0),
            status: Rect::default(),
            trace: Rect::default(),
            gauge: None,
        },
    }
}

fn full(w: i32, h: i32) -> PrimaryLayout {
    let side_w = (w * 2 / 15).clamp(14, 24);
    // At least 6 rows so the header ledger keeps all three telemetry lines.
    let upper = (h / 6).clamp(6, 9);
    let lower_top = upper + 1;
    let cx0 = side_w + 3;
    let cx1 = w - 2;
    let cy0 = lower_top + 3;
    let cy1 = h - 3; // exclusive-ish: last content row is h-3
    let cw = cx1 - cx0;
    let ch = cy1 - cy0 + 1;

    let title = Rect::new(cx0, 0, cx1 - cx0 + 1, 1);
    let ledger = Rect::new(cx0, 1, cw, (upper - 3).max(1));

    let (ccols, crows) = fit_clock((cw as f32 * 0.6) as i32, (ch as f32 * 0.3).round().clamp(5.0, 11.0) as i32);
    let clock_w = Font::measure("00:00:00", ccols);
    let clock = Rect::new(cx0 + 1, cy0 + 1, clock_w, crows);
    let date = Rect::new(clock.x, clock.bottom() + 1, clock_w, 2);

    let rx = clock.right() + if cw > 150 { 8 } else { 4 };
    let rw = cx1 - rx;
    // Clearly subordinate to the clock, but never too narrow to read.
    let lrows = ((crows as f32) * 0.42).round().clamp(3.0, 5.0) as i32;
    let lcols = ((lrows as f32) * 0.8).round().max(3.0) as i32;
    let load = Rect::new(rx, clock.y - 1, rw, lrows + 2);
    let status = Rect::new(rx, load.bottom() + 1, rw, 3.min(date.bottom() - load.bottom()).max(2));

    let low_y = date.bottom() + 2;
    let low_h = cy1 - low_y + 1;
    let gauge_w = ((cw as f32 * 0.42) as i32).min(low_h * 4 + 2).max(0);
    let gauge = (gauge_w >= 30 && low_h >= 9).then(|| Rect::new(cx1 - gauge_w, low_y, gauge_w, low_h));
    let trace_right = gauge.map(|g| g.x - 4).unwrap_or(cx1);
    let trace = Rect::new(cx0 + 1, low_y, trace_right - cx0 - 1, low_h);

    let blocks = sidebar_blocks(&KINDS, lower_top + 4, h - 5, 3, 6);
    PrimaryLayout {
        tier: Tier::Full,
        w,
        h,
        side_w,
        upper: Some(upper),
        lower_top,
        blocks,
        ledger,
        title,
        clock,
        clock_cell: (ccols, crows),
        date,
        load,
        load_cell: (lcols, lrows),
        status,
        trace,
        gauge,
    }
}

fn compact(w: i32, h: i32) -> PrimaryLayout {
    let side_w = (w * 2 / 15).clamp(9, 13);
    let lower_top = 0;
    let cx0 = side_w + 2;
    let cx1 = w - 2;
    let cy0 = 3;
    let cy1 = h - 3;
    let cw = cx1 - cx0;

    let (ccols, crows) = fit_clock((cw as f32 * 0.56) as i32, if h >= 22 { 5 } else { 4 });
    let clock_w = Font::measure("00:00:00", ccols);
    let clock = Rect::new(cx0 + 1, cy0, clock_w, crows);
    let date = Rect::new(clock.x, clock.bottom() + 1, clock_w, 2);

    let rx = clock.right() + 3;
    let rw = cx1 - rx;
    let lrows = 3;
    let lcols = 3;
    let load = Rect::new(rx, cy0, rw, lrows + 1);
    let status = Rect::new(rx, load.bottom() + 1, rw, (date.bottom() - load.bottom() - 1).max(1));

    let low_y = date.bottom() + 1;
    let ledger_w = 25.min(cw / 2);
    let ledger = Rect::new(cx1 - ledger_w, low_y, ledger_w, (cy1 - low_y + 1).min(6));
    let trace = Rect::new(cx0 + 1, low_y, ledger.x - cx0 - 3, cy1 - low_y + 1);

    let blocks = sidebar_blocks(&KINDS, 3, h - 4, 3, 4);
    PrimaryLayout {
        tier: Tier::Compact,
        w,
        h,
        side_w,
        upper: None,
        lower_top,
        blocks,
        ledger,
        title: Rect::new(side_w + 3, 0, w - side_w - 5, 1),
        clock,
        clock_cell: (ccols, crows),
        date,
        load,
        load_cell: (lcols, lrows),
        status,
        trace,
        gauge: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_at_reference_sizes() {
        assert_eq!(tier(80, 24), Tier::Compact);
        assert_eq!(tier(120, 36), Tier::Full);
        assert_eq!(tier(30, 10), Tier::Tiny);
        assert_eq!(tier(200, 20), Tier::Compact);
    }

    #[test]
    fn content_fits_and_never_overlaps_at_any_supported_size() {
        for w in 44..=260 {
            for h in 14..=90 {
                let l = primary(w, h);
                let screen = Rect::new(0, 0, w, h);
                let rects = l.content_rects();
                for (name, r) in &rects {
                    assert!(screen.contains_rect(r), "{name} {r:?} outside {w}x{h}");
                    assert!(r.x >= l.side_w, "{name} overlaps sidebar at {w}x{h}");
                }
                for i in 0..rects.len() {
                    for j in i + 1..rects.len() {
                        assert!(
                            !rects[i].1.intersects(&rects[j].1),
                            "{} {:?} overlaps {} {:?} at {w}x{h}",
                            rects[i].0,
                            rects[i].1,
                            rects[j].0,
                            rects[j].1
                        );
                    }
                }
                for b in &l.blocks {
                    assert!(b.y0 <= b.y1 && b.y1 < h, "block {b:?} at {w}x{h}");
                }
            }
        }
    }

    #[test]
    fn full_tier_reveals_more_than_compact() {
        let c = primary(80, 24);
        let f = primary(120, 36);
        assert!(c.gauge.is_none() && c.upper.is_none());
        assert!(f.gauge.is_some() && f.upper.is_some());
        assert!(f.clock_cell.1 > c.clock_cell.1, "clock grows on larger terminals");
        let big = primary(220, 60);
        assert!(big.clock_cell.1 > f.clock_cell.1);
    }
}
