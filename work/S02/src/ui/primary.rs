//! The command view: an asymmetric double-elbow LCARS frame around a
//! dominant clock, with telemetry cut into the frame, a sensor trace and a
//! concentric sweep gauge.

use super::font::Font;
use super::layout::{self, BlockKind, PrimaryLayout, Tier};
use super::shapes::{cut_right, pill, Align, Corner, Frame};
use super::theme::*;
use super::widgets::{self, Ring};
use super::{blink, condition, pulse, Condition, Focus, Scene};
use crate::canvas::{ellipsize, str_width, Canvas, Rect, Style};
use crate::color::Color;
use crate::format;

pub fn render(c: &mut Canvas, s: &Scene, font: &mut Font) {
    let l = layout::primary(c.width(), c.height());
    match l.tier {
        Tier::Tiny => tiny(c, s),
        Tier::Full => {
            full_frame(c, s, &l);
            header(c, s, &l);
            common(c, s, &l, font);
            gauge(c, s, &l);
        }
        Tier::Compact => {
            compact_frame(c, s, &l);
            common(c, s, &l, font);
            compact_ledger(c, s, l.ledger);
        }
    }
}

// ---------------------------------------------------------------- values

fn cpu_text(s: &Scene) -> String {
    match s.tel.cpu_pct {
        Ok(p) => format::pct(p),
        Err(m) => m.short().into(),
    }
}

fn mem_text(s: &Scene) -> String {
    match &s.tel.memory {
        Ok(m) => format::pct(m.used_fraction() * 100.0),
        Err(e) => e.short().into(),
    }
}

fn disk_text(s: &Scene) -> String {
    match &s.tel.disk {
        Ok(d) => format::pct(d.used_fraction() * 100.0),
        Err(e) => e.short().into(),
    }
}

fn net_text(s: &Scene) -> String {
    match s.tel.net {
        Ok(n) => format::bytes((n.rx_bps + n.tx_bps).round().max(0.0) as u64),
        Err(e) => e.short().into(),
    }
}

fn load_text(s: &Scene) -> String {
    match s.tel.load {
        Ok(l) => format::load(l[0]),
        Err(e) => e.short().into(),
    }
}

fn block_color(kind: BlockKind) -> Color {
    match kind {
        BlockKind::Cpu => GOLD,
        BlockKind::Mem => LAVENDER,
        BlockKind::Disk => SALMON,
        BlockKind::Net => LILAC,
        BlockKind::Load => ORANGE,
        BlockKind::Filler => PEACH,
    }
}

fn block_focus(kind: BlockKind) -> Option<Focus> {
    match kind {
        BlockKind::Cpu => Some(Focus::Cpu),
        BlockKind::Mem => Some(Focus::Memory),
        BlockKind::Net => Some(Focus::Network),
        BlockKind::Load => Some(Focus::Load),
        _ => None,
    }
}

fn block_label(s: &Scene, kind: BlockKind) -> (&'static str, String) {
    match kind {
        BlockKind::Cpu => ("CPU", cpu_text(s)),
        BlockKind::Mem => ("MEM", mem_text(s)),
        BlockKind::Disk => ("DSK", disk_text(s)),
        BlockKind::Net => ("NET", net_text(s)),
        BlockKind::Load => ("LOAD", load_text(s)),
        BlockKind::Filler => ("SEQ", format!("{:05}", s.tel.samples % 100_000)),
    }
}

// ---------------------------------------------------------------- frame

/// Sidebar blocks with their labels, shared by both tiers.
fn sidebar(f: &mut Frame, s: &Scene, l: &PrimaryLayout) {
    for b in &l.blocks {
        let focused = block_focus(b.kind) == Some(s.focus);
        let col = if focused { ICE } else { block_color(b.kind) };
        f.rect_px(0, 2 * b.y0 + 1, 2 * l.side_w, 2 * (b.y1 + 1), col);
    }
}

fn sidebar_labels(c: &mut Canvas, s: &Scene, l: &PrimaryLayout) {
    let right = l.side_w - 1;
    for (i, b) in l.blocks.iter().enumerate() {
        if b.y1 <= b.y0 {
            continue; // only a half row: nothing can be cut into it
        }
        let (name, value) = block_label(s, b.kind);
        let focused = block_focus(b.kind) == Some(s.focus);
        let roomy = b.y1 - 1 > b.y0;
        if str_width(name) + str_width(&value) + 3 <= l.side_w {
            // Name and value share the bottom row; code and focus above.
            c.label(1, b.y1, name, INK, true);
            cut_right(c, right, b.y1, &value, true);
            if roomy {
                if focused {
                    c.label(1, b.y1 - 1, "▸ TRACE", INK, false);
                }
                if !focused || l.side_w >= 12 {
                    cut_right(c, right, b.y1 - 1, &format!("{:02}", i + 3), false);
                }
            }
        } else if roomy {
            // Narrow sidebar: name above, value below.
            let tag = if focused { format!("▸{name}") } else { name.to_string() };
            c.label(1, b.y1 - 1, &tag, INK, true);
            cut_right(c, right, b.y1, &ellipsize(&value, l.side_w - 2), true);
        } else {
            cut_right(c, right, b.y1, &ellipsize(&value, l.side_w - 2), true);
        }
    }
}

struct Seg {
    x0: i32,
    x1: i32,
    color: Color,
}

/// Split [x0, x1) into segments at the given fractions with one-column gaps.
fn segments(x0: i32, x1: i32, fracs: &[f32], colors: &[Color]) -> Vec<Seg> {
    let span = (x1 - x0) as f32;
    let mut out = Vec::new();
    let mut start = x0;
    for (i, &color) in colors.iter().enumerate() {
        let end = if i + 1 == colors.len() { x1 } else { x0 + (span * fracs[i]) as i32 };
        if end - start >= 2 {
            out.push(Seg { x0: start, x1: end, color });
        }
        start = end + 1;
    }
    out
}

fn full_frame(c: &mut Canvas, s: &Scene, l: &PrimaryLayout) {
    let (w, h) = (c.width(), c.height());
    let sw = l.side_w;
    let u = l.upper.unwrap_or(6);
    let lt = l.lower_top;
    let mut f = Frame::new(w, h);

    // Upper frame: a capped sidebar block, then an elbow sweeping right
    // along the bottom of the header.
    let elbow_top = 2 * (u - 3);
    f.rounded_px((0, 0, 2 * sw, elbow_top - 1), (6, 3), [true, false, false, false], SALMON);
    let up_elbow_end = sw + (w - sw) * 38 / 100;
    f.elbow_px(0, elbow_top, 2 * up_elbow_end, 2 * u, 2 * sw, 3, Corner::BottomLeft, (12, 6), (6, 3), ORANGE);
    let upper_segs = segments(up_elbow_end + 1, w - 1, &[0.14, 0.2], &[LAVENDER, GOLD]);
    for (i, sg) in upper_segs.iter().enumerate() {
        let last = i + 1 == upper_segs.len();
        f.rounded_px((2 * sg.x0, 2 * u - 3, 2 * sg.x1, 2 * u), (3, 1), [false, last, last, false], sg.color);
    }

    // Lower frame: elbow into the long sidebar, rail segments across the top.
    let lo_elbow_end = sw + (w - sw) * 24 / 100;
    f.elbow_px(0, 2 * lt, 2 * lo_elbow_end, 2 * (lt + 4), 2 * sw, 3, Corner::TopLeft, (14, 7), (6, 3), PEACH);
    let lower_segs = segments(lo_elbow_end + 1, w - 1, &[0.1, 0.62], &[SALMON, GOLD, BLUE]);
    for (i, sg) in lower_segs.iter().enumerate() {
        let last = i + 1 == lower_segs.len();
        f.rounded_px((2 * sg.x0, 2 * lt, 2 * sg.x1, 2 * lt + 3), (3, 1), [false, last, last, false], sg.color);
    }

    sidebar(&mut f, s, l);
    let foot = sw + 4;
    f.elbow_px(0, 2 * (h - 4) + 1, 2 * foot, 2 * h, 2 * sw, 2, Corner::BottomLeft, (12, 6), (6, 3), LAVENDER);
    f.blit(c);

    // Scanner: a lighter pulse gliding along the long lower rail.
    if let Some(sg) = lower_segs.get(1) {
        if !s.paused {
            scanner(c, sg.x0, sg.x1, lt, s.phase);
        }
    }

    // Labels cut into the frame.
    c.label(1, u - 5, "LCARS", INK, true);
    cut_right(c, sw - 1, u - 5, "01", false);
    cut_right(c, sw - 1, u - 3, "OPS-01", true);
    if let Some(sg) = upper_segs.last() {
        let text = format!("{} · {}", s.sys.os.to_uppercase(), s.sys.arch.to_uppercase());
        cut_right(c, sg.x1 - 2, u - 1, &ellipsize(&text, sg.x1 - sg.x0 - 3), true);
    }
    cut_right(c, lo_elbow_end - 1, lt, "COMMAND", true);
    if let Some(sg) = lower_segs.last() {
        let t = format!("FOCUS {}", s.focus.name());
        cut_right(c, sg.x1 - 2, lt, &ellipsize(&t, sg.x1 - sg.x0 - 3), true);
    }
    sidebar_labels(c, s, l);
    feed_label(c, s, sw, h - 3);
    key_bar(c, foot + 1, h - 1, w);
}

fn compact_frame(c: &mut Canvas, s: &Scene, l: &PrimaryLayout) {
    let (w, h) = (c.width(), c.height());
    let sw = l.side_w;
    let mut f = Frame::new(w, h);
    let elbow_end = sw + (w - sw) * 30 / 100;
    f.elbow_px(0, 0, 2 * elbow_end, 6, 2 * sw, 3, Corner::TopLeft, (10, 5), (5, 3), ORANGE);
    let segs = segments(elbow_end + 1, w - 1, &[0.14], &[SALMON, BLUE]);
    for (i, sg) in segs.iter().enumerate() {
        let last = i + 1 == segs.len();
        f.rounded_px((2 * sg.x0, 0, 2 * sg.x1, 3), (3, 1), [false, last, last, false], sg.color);
    }
    sidebar(&mut f, s, l);
    let foot = sw + 3;
    f.elbow_px(0, 2 * (h - 3) + 1, 2 * foot, 2 * h, 2 * sw, 2, Corner::BottomLeft, (10, 5), (5, 3), LAVENDER);
    f.blit(c);

    cut_right(c, elbow_end - 1, 0, "COMMAND", true);
    if let Some(sg) = segs.last() {
        let host = host_upper(s);
        let t = format!("{}  ·  FOCUS {}", host, s.focus.name());
        cut_right(c, sg.x1 - 2, 0, &ellipsize(&t, sg.x1 - sg.x0 - 3), true);
    }
    sidebar_labels(c, s, l);
    feed_label(c, s, sw, h - 2);
    key_bar(c, foot + 1, h - 1, w);
}

/// LIVE / HOLD indicator in the lower sidebar elbow.
fn feed_label(c: &mut Canvas, s: &Scene, sw: i32, y: i32) {
    if s.paused {
        if blink(s.phase, 1.0) {
            c.label(1, y, "HOLD", INK, true);
        }
    } else {
        c.label(1, y, "LIVE", INK, true);
    }
    if sw >= 12 {
        cut_right(c, sw - 1, y, &format!("{:.1}S", s.interval.as_secs_f32()), false);
    }
}

fn scanner(c: &mut Canvas, x0: i32, x1: i32, y: i32, phase: f32) {
    let span = (x1 - x0).max(1) as f32;
    let head = x0 as f32 + (phase * 0.35).fract() * span;
    for x in x0..x1 {
        let d = (x as f32 - head).abs().min((x as f32 - head + span).abs());
        let glow = (1.0 - d / 5.0).max(0.0);
        if glow <= 0.0 {
            continue;
        }
        let col = GOLD.lerp(Color::rgb(255, 236, 196), glow);
        c.set_bg(x, y, col);
        if let Some(cell) = c.get(x, y + 1).copied() {
            if cell.ch == '▀' {
                c.put(x, y + 1, '▀', Style::on(col, cell.style.bg));
            }
        }
    }
}

const KEYS: [(&str, &str, Color); 5] =
    [("?", "HELP", AMBER), ("P", "DIAG", LAVENDER), ("TAB", "FOCUS", PEACH), ("SPC", "HOLD", GOLD), ("Q", "EXIT", SALMON)];

/// The bottom rail doubles as the key legend, so controls are discoverable.
fn key_bar(c: &mut Canvas, x0: i32, y: i32, w: i32) {
    let mut x = x0;
    for (key, what, col) in KEYS {
        let text = format!(" {key} {what} ");
        let tw = str_width(&text);
        if x + tw + 3 > w {
            break;
        }
        c.fill(Rect::new(x, y, tw, 1), ' ', Style::on(INK, col));
        c.label(x, y, &text, INK, true);
        x += tw + 1;
    }
    if w - 1 - x >= 3 {
        c.fill(Rect::new(x, y, w - 2 - x, 1), ' ', Style::on(INK, TRACK_COOL.lerp(BLUE, 0.6)));
        c.put(w - 2, y, '◗', Style::fg(TRACK_COOL.lerp(BLUE, 0.6)));
    }
}

// ---------------------------------------------------------------- content

fn host_upper(s: &Scene) -> String {
    match &s.tel.hostname {
        Ok(h) => h.to_uppercase(),
        Err(m) => format!("HOST {}", m.short()),
    }
}

fn header(c: &mut Canvas, s: &Scene, l: &PrimaryLayout) {
    let t = l.title;
    // Letter-spaced hostname as the station title, compacted when long.
    let host = host_upper(s);
    let spaced: String = host.chars().flat_map(|ch| [ch, ' ']).collect::<String>().trim_end().to_string();
    let title = if str_width(&spaced) <= t.w * 2 / 3 { spaced } else { ellipsize(&host, t.w * 2 / 3) };
    let tw = str_width(&title);
    c.text(t.right() - tw, t.y, &title, Style::fg(ORANGE).bold());
    c.text_max(t.x, t.y, "OPERATIONS MONITOR", Style::fg(DIM), t.w - tw - 2);

    let r = l.ledger;
    let rows = ledger_rows(s);
    for (i, row) in rows.iter().enumerate().take(r.h as usize) {
        draw_kv(c, r.x, r.y + i as i32, row, r.w);
    }
}

/// (tag, tag color, [(label, value, value color)])
type KvRow = (&'static str, Color, Vec<(&'static str, String, Color)>);

fn ledger_rows(s: &Scene) -> Vec<KvRow> {
    let unavailable = |m: &'static str| vec![("", m.to_string(), DIM)];
    let mem = match &s.tel.memory {
        Ok(m) => {
            vec![("USED", format::bytes(m.used()), LAVENDER), ("AVAIL", format::bytes(m.available), ICE), ("TOTAL", format::bytes(m.total), PEACH)]
        }
        Err(e) => unavailable(e.short()),
    };
    let dsk = match &s.tel.disk {
        Ok(d) => vec![
            ("", d.mount.clone(), SALMON),
            ("USED", format::bytes(d.used), SALMON),
            ("FREE", format::bytes(d.available), ICE),
            ("TOTAL", format::bytes(d.total), PEACH),
        ],
        Err(e) => unavailable(e.short()),
    };
    let net = match s.tel.net {
        Ok(n) => vec![("▼ RX", format::rate(n.rx_bps), ICE), ("▲ TX", format::rate(n.tx_bps), SALMON)],
        Err(e) => unavailable(e.short()),
    };
    vec![("MEM", LAVENDER, mem), ("DSK", SALMON, dsk), ("NET", BLUE, net)]
}

fn draw_kv(c: &mut Canvas, x: i32, y: i32, row: &KvRow, max_w: i32) {
    let (tag, tag_col, fields) = row;
    let right = x + max_w;
    let mut cx = x + c.text(x, y, tag, Style::fg(*tag_col).bold()) + 2;
    for (label, value, col) in fields {
        let need = str_width(label) + str_width(value) + if label.is_empty() { 0 } else { 1 };
        if cx + need > right {
            break;
        }
        if !label.is_empty() {
            cx += c.text(cx, y, label, Style::fg(DIM)) + 1;
        }
        cx += c.text(cx, y, value, Style::fg(*col).bold()) + 3;
    }
}

fn common(c: &mut Canvas, s: &Scene, l: &PrimaryLayout, font: &mut Font) {
    // Dominant focal element: the clock.
    let (cc, cr) = l.clock_cell;
    font.draw(c, (l.clock.x, l.clock.y), &s.time.hms(), (cc, cr), (ORANGE, AMBER));

    let d = l.date;
    let dw = c.text_max(d.x, d.y, &s.time.long_date(), Style::fg(SALMON).bold(), d.w);
    if !s.time.zone.is_empty() {
        c.text_max(d.x + dw + 2, d.y, &s.time.zone, Style::fg(DIM), d.w - dw - 2);
    }
    let x = d.x + c.text(d.x, d.y + 1, "UPTIME ", Style::fg(DIM));
    let up = match s.tel.uptime {
        Ok(u) => (format::uptime(u), LAVENDER),
        Err(m) => (m.short().to_string(), DIM),
    };
    c.text_max(x, d.y + 1, &up.0, Style::fg(up.1).bold(), d.right() - x);

    load_readout(c, s, l, font);
    status(c, s, l.status);
    sensor_trace(c, s, l.trace);
}

fn load_readout(c: &mut Canvas, s: &Scene, l: &PrimaryLayout, font: &mut Font) {
    let r = l.load;
    let (lc, lr) = l.load_cell;
    let pw = 8.min(r.w);
    pill(c, r.x, r.y, pw, LAVENDER, "LOAD", Align::Center);
    c.text_max(r.x + pw + 1, r.y, "AVERAGE · 1 MIN", Style::fg(DIM), r.w - pw - 1);
    match s.tel.load {
        Ok(ld) => {
            let digits = format::load(ld[0]);
            let dw = Font::measure(&digits, lc);
            font.draw(c, (r.x, r.y + 1), &digits, (lc, lr), (LAVENDER, LILAC));
            let tx = r.x + dw + 2;
            let tw = r.right() - tx;
            let cores = s.tel.core_pct.as_ref().map(|v| v.len()).unwrap_or(0);
            let per_cpu = if cores > 0 { format!("{:.2}", ld[0] / cores as f64) } else { "--".into() };
            let wide = tw >= 14;
            let lines = [
                (if wide { "5 MIN" } else { "5M" }, format::load(ld[1])),
                (if wide { "15 MIN" } else { "15M" }, format::load(ld[2])),
                (if wide { "PER CPU" } else { "/CPU" }, per_cpu),
            ];
            let kw = if wide { 8 } else { 5 };
            for (i, (k, v)) in lines.iter().enumerate().take(lr as usize) {
                let y = r.y + 1 + i as i32;
                c.text_max(tx, y, k, Style::fg(DIM), tw);
                c.text_max(tx + kw, y, v, Style::fg(LILAC).bold(), tw - kw);
            }
        }
        Err(m) => {
            c.text_max(r.x, r.y + 2, &format!("LOAD {}", m.short()), Style::fg(DIM).bold(), r.w);
        }
    }
}

fn status(c: &mut Canvas, s: &Scene, r: Rect) {
    if r.is_empty() {
        return;
    }
    let cond = condition(s.tel);
    let lit = cond != Condition::Alert || blink(s.phase, 1.2);
    let col = if lit { cond.color() } else { cond.color().scale(0.35) };
    // A two-row rounded status plate: the third focal readout.
    let bw = (str_width(cond.label()).max(9) + 6).min(r.w);
    let bh = r.h.min(2);
    let mut f = Frame::new(bw, bh);
    f.rounded_px((0, 0, 2 * bw, 2 * bh), (4, 2), [true, true, true, true], col);
    f.blit_at(c, r.x, r.y);
    if bh == 2 {
        c.label(r.x + 2, r.y, "CONDITION", INK, false);
        cut_right(c, r.x + bw - 2, r.y + 1, cond.label(), true);
    } else {
        cut_right(c, r.x + bw - 2, r.y, cond.label(), true);
    }

    let tx = r.x + bw + 2;
    let tw = r.right() - tx;
    if tw < 6 {
        return;
    }
    let (dot_col, word, text_col) = if s.paused {
        (if blink(s.phase, 1.0) { AMBER } else { TRACK }, "HOLD", AMBER)
    } else {
        (TRACK.lerp(cond.color(), 0.25 + 0.75 * pulse(s.phase, 0.7)), "LIVE", DIM)
    };
    c.put(tx, r.y, '●', Style::fg(dot_col));
    let text = if tw >= 16 { format!("TELEMETRY {word}") } else { word.to_string() };
    c.text_max(tx + 2, r.y, &text, Style::fg(text_col).bold(), tw - 2);
    if bh == 2 {
        let cores = s.tel.core_pct.as_ref().map(|v| v.len()).unwrap_or(0);
        let mut t = format!("{:.1}S", s.interval.as_secs_f32());
        let extra = format!(" · {cores} CPU");
        if cores > 0 && str_width(&t) + str_width(&extra) <= tw - 2 {
            t.push_str(&extra);
        }
        c.text_max(tx + 2, r.y + 1, &t, Style::fg(DIM), tw - 2);
    }
}

fn focus_series(s: &Scene, n: usize) -> (Vec<f32>, f32, String, String) {
    let h = s.history;
    match s.focus {
        Focus::Cpu => (h.cpu.last_n(n).collect(), 100.0, cpu_text(s), format::pct(h.cpu.max_last(n) as f64)),
        Focus::Memory => (h.mem.last_n(n).collect(), 100.0, mem_text(s), format::pct(h.mem.max_last(n) as f64)),
        Focus::Load => {
            let cores = s.tel.core_pct.as_ref().map(|v| v.len()).unwrap_or(1).max(1) as f32;
            let peak = h.load.max_last(n);
            (h.load.last_n(n).collect(), peak.max(cores), load_text(s), format::load(peak as f64))
        }
        Focus::Network => (vec![], 0.0, String::new(), String::new()),
    }
}

fn sensor_trace(c: &mut Canvas, s: &Scene, r: Rect) {
    if r.w < 12 || r.h < 4 {
        return;
    }
    let area = Rect::new(r.x, r.y + 1, r.w, r.h - 2);
    let n = (area.w * 2) as usize;
    let title = format!("{} TRACE", s.focus.name());
    let pw = (str_width(&title) + 4).min(r.w);
    pill(c, r.x, r.y, pw, ICE, &title, Align::Center);
    let info_x = r.x + pw + 2;
    let info_w = r.right() - info_x;

    if s.focus == Focus::Network {
        let rx: Vec<f32> = s.history.rx.last_n(n).collect();
        let tx: Vec<f32> = s.history.tx.last_n(n).collect();
        let peak = s.history.rx.max_last(n).max(s.history.tx.max_last(n)).max(1024.0);
        let info = match s.tel.net {
            Ok(v) => format!("▼ {}  ▲ {}  PEAK {}", format::rate(v.rx_bps), format::rate(v.tx_bps), format::rate(peak as f64)),
            Err(m) => m.short().to_string(),
        };
        c.text_max(info_x, r.y, &info, Style::fg(PEACH), info_w);
        widgets::mirror_trace(c, area, &rx, &tx, peak, ICE, SALMON);
    } else {
        let (vals, max, now, peak) = focus_series(s, n);
        c.text_max(info_x, r.y, &format!("NOW {now}  PEAK {peak}"), Style::fg(PEACH), info_w);
        widgets::trace(c, area, &vals, max, |f| if f < 0.5 { BLUE.lerp(LAVENDER, f * 2.0) } else { LAVENDER.lerp(ORANGE, (f - 0.5) * 2.0) });
        if vals.is_empty() {
            let msg = "ACQUIRING SENSOR DATA";
            c.text(area.x + (area.w - str_width(msg)) / 2, area.y + area.h / 2, msg, Style::fg(DIM));
        }
    }
    // Time axis: span of the visible window at the sampling interval.
    let y = r.bottom() - 1;
    let secs = (n as f32 * s.interval.as_secs_f32()).round() as u64;
    let span = if secs >= 120 { format!("-{}M", secs / 60) } else { format!("-{secs}S") };
    c.text(r.x, y, &span, Style::fg(DIM));
    c.text(r.right() - 3, y, "NOW", Style::fg(DIM));
    for x in (r.x + str_width(&span) + 2..r.right() - 5).step_by(6) {
        c.put(x, y, '·', Style::fg(TRACK));
    }
}

fn gauge(c: &mut Canvas, s: &Scene, l: &PrimaryLayout) {
    let Some(r) = l.gauge else { return };
    let frac = |v: Option<f64>| v.map(|p| (p / 100.0) as f32);
    let cpu = s.tel.cpu_pct.ok();
    let mem = s.tel.memory.as_ref().ok().map(|m| m.used_fraction() * 100.0);
    let dsk = s.tel.disk.as_ref().ok().map(|d| d.used_fraction() * 100.0);
    let rings = [
        Ring { label: "CPU", value: frac(cpu), color: GOLD },
        Ring { label: "MEM", value: frac(mem), color: LAVENDER },
        Ring { label: "DSK", value: frac(dsk), color: SALMON },
    ];
    let readings = [cpu_text(s), mem_text(s), disk_text(s)];
    widgets::sweep_gauge(c, r, &rings, &readings);

    // Hub: the innermost space carries the swap reading when there is one.
    let cx = r.x + r.w / 2;
    let hub = match &s.tel.swap {
        Ok(sw) if sw.total > 0 => format!("SWAP {}", format::pct(sw.used as f64 / sw.total as f64 * 100.0)),
        Ok(_) => "NO SWAP".to_string(),
        Err(m) => format!("SWAP {}", m.short()),
    };
    let (_, _, spans) = widgets::ring_spans(r, 3);
    if spans.last().map_or(true, |sp| sp.r_in < 6.0) {
        return; // hub too small for text
    }
    let tag = "SENSORS";
    c.text(cx - str_width(tag) / 2, r.bottom() - 3, tag, Style::fg(DIM));
    c.text(cx - str_width(&hub) / 2, r.bottom() - 2, &hub, Style::fg(PEACH).bold());
}

fn compact_ledger(c: &mut Canvas, s: &Scene, r: Rect) {
    if r.is_empty() {
        return;
    }
    let mut lines: Vec<(&str, &str, String, Color)> = Vec::new();
    match &s.tel.memory {
        Ok(m) => {
            lines.push(("MEM", "USED", format::bytes(m.used()), LAVENDER));
            lines.push(("", "AVAIL", format::bytes(m.available), ICE));
            lines.push(("", "TOTAL", format::bytes(m.total), PEACH));
        }
        Err(e) => lines.push(("MEM", "", e.short().into(), DIM)),
    }
    match &s.tel.disk {
        Ok(d) => lines.push(("DSK", "USED", format!("{} / {}", format::bytes(d.used), format::bytes(d.total)), SALMON)),
        Err(e) => lines.push(("DSK", "", e.short().into(), DIM)),
    }
    match s.tel.net {
        Ok(n) => {
            lines.push(("NET", "▼ RX", format::rate(n.rx_bps), ICE));
            lines.push(("", "▲ TX", format::rate(n.tx_bps), SALMON));
        }
        Err(e) => lines.push(("NET", "", e.short().into(), DIM)),
    }
    for (i, (tag, k, v, col)) in lines.iter().enumerate().take(r.h as usize) {
        let y = r.y + i as i32;
        let tag_col = match *tag {
            "MEM" => LAVENDER,
            "DSK" => SALMON,
            _ => BLUE,
        };
        c.text(r.x, y, tag, Style::fg(tag_col).bold());
        c.text(r.x + 4, y, k, Style::fg(DIM));
        c.text_max(r.x + 10, y, v, Style::fg(*col).bold(), r.w - 10);
    }
}

/// Below the minimum composition size: a clipped, still-live readout.
fn tiny(c: &mut Canvas, s: &Scene) {
    let (w, h) = (c.width(), c.height());
    if w <= 0 || h <= 0 {
        return;
    }
    c.fill(Rect::new(0, 0, w, 1), ' ', Style::on(INK, ORANGE));
    c.label(1, 0, &ellipsize(&format!("LCARS {}", host_upper(s)), w - 2), INK, true);
    c.text_max(1, 1, &s.time.hms(), Style::fg(AMBER).bold(), w - 1);
    let lines = [format!("CPU {}  MEM {}", cpu_text(s), mem_text(s)), format!("LOAD {}  DSK {}", load_text(s), disk_text(s))];
    for (i, t) in lines.iter().enumerate() {
        let y = 2 + i as i32;
        if y < h - 1 {
            c.text_max(1, y, t, Style::fg(PEACH), w - 1);
        }
    }
    if h >= 3 {
        c.text_max(1, h - 1, "ENLARGE VIEWPORT 44X14", Style::fg(DIM), w - 1);
    }
}
