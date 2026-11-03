//! The command view: the primary composition.
//!
//! Silhouette: a thick orange top bar sweeping through an elbow into a
//! segmented left rail; a thin bottom bar carrying the key hints; the clock
//! as the dominant focal readout in the upper right; CPU as a second, smaller
//! focal readout with a sweeping trace; memory, disk and network as paired
//! sweeps side by side; and the lower part of the screen left deliberately
//! empty apart from a small sampling readout in the corner.

use super::font::{self, Font};
use super::frame::{self, FrameSpec, RailBlock, Segment, Side};
use super::widgets;
use super::{key_segments, Scene, Status, Tier};
use crate::metrics::units;
use crate::term::buffer::fit;
use crate::term::{Buffer, Color, Glyphs, Style};

pub fn draw(buf: &mut Buffer, g: &Glyphs, scene: &Scene, tier: Tier) {
    let t = scene.telemetry;
    let status = Status::derive(t, scene.info.cores);
    let rail_w = tier.rail_width();

    // ---- Frame ---------------------------------------------------------
    let host = g.text(&scene.info.hostname).to_uppercase();
    let top = match tier {
        Tier::Tiny => vec![
            Segment::new(Color::ORANGE, "", rail_w + 2),
            Segment::new(Color::LAVENDER, "LCARS", 0),
        ],
        Tier::Compact => vec![
            Segment::new(Color::ORANGE, "", rail_w + 10),
            Segment::new(Color::LAVENDER, host.as_str(), 30),
            Segment::new(Color::SALMON, scene.view.code(), 0),
        ],
        Tier::Standard => vec![
            Segment::new(Color::ORANGE, "", rail_w + 18),
            Segment::new(Color::LAVENDER, host.as_str(), 36),
            Segment::new(Color::SALMON, scene.view.code(), 14),
            Segment::new(
                Color::BLUE,
                format!("{} {}", scene.info.os.to_uppercase(), scene.info.arch.to_uppercase()),
                0,
            ),
        ],
    };
    let bottom = key_segments(scene, tier);
    let mem_fill = t.memory.map(|m| m.used as f32 / m.total.max(1) as f32);
    let disk_fill = t.disk.as_ref().map(|d| d.used as f32 / d.total.max(1) as f32);
    let mem_pct = units::percent(mem_fill);
    let disk_pct = units::percent(disk_fill);
    let up_short = t
        .uptime
        .map(units::uptime_short)
        .unwrap_or_else(|| g.unavailable.into());
    let load1 = t
        .load
        .map(|l| units::load(l[0]))
        .unwrap_or_else(|| g.unavailable.into());
    let rail: Vec<RailBlock> = match tier {
        Tier::Tiny => vec![
            RailBlock::new(Color::ORANGE, 2.0, &[]),
            RailBlock::new(Color::LAVENDER, 1.0, &[]),
            RailBlock::new(Color::ORANGE, 1.0, &[]),
        ],
        Tier::Compact => vec![
            RailBlock::new(Color::ORANGE, 2.0, &["ENG"]),
            RailBlock::new(Color::LAVENDER, 2.0, &["UP", &up_short]),
            RailBlock::new(Color::BLUE, 1.5, &["MEM", mem_pct.trim()]).with_fill(mem_fill),
            RailBlock::new(Color::ORANGE, 1.0, &["DSK"]).with_fill(disk_fill),
        ],
        Tier::Standard => vec![
            RailBlock::new(Color::ORANGE, 3.0, &["ENG 47-2"]),
            RailBlock::new(Color::LAVENDER, 2.5, &["UPTIME", &up_short]),
            RailBlock::new(Color::BLUE, 3.0, &["MEMORY", mem_pct.trim()]).with_fill(mem_fill),
            RailBlock::new(Color::SALMON, 1.5, &["LOAD", &load1]),
            RailBlock::new(Color::ORANGE, 2.0, &["DISK", disk_pct.trim()]).with_fill(disk_fill),
        ],
    };
    let scan = if g.unicode {
        Some(((scene.frame % 64) as f32) / 64.0)
    } else {
        None
    };
    let spec = FrameSpec {
        side: Side::Left,
        rail_w,
        top: &top,
        bottom: &bottom,
        rail: &rail,
        scan,
    };
    let (cx0, cy0, cx1, cy1) = frame::draw(buf, g, &spec);

    if tier == Tier::Tiny {
        draw_tiny(buf, g, scene, status, cx0, cy0, cx1, cy1);
        return;
    }

    // ---- Focal readouts ------------------------------------------------
    let clock_text = scene.clock.time();
    let clock_w = Font::Big.width(&clock_text);
    let clock_x = cx1 - clock_w;
    font::draw(buf, g, clock_x, cy0, &clock_text, Font::Big, Style::fg(Color::AMBER));
    buf.put_str_right(cx1, cy0 + 5, &scene.clock.date(), Style::fg(Color::LAVENDER));

    let dim = Style::fg(Color::DIM);
    let value = Style::fg(Color::IVORY);
    let sx = cx0;
    let status_w = (clock_x - 2 - sx).max(0) as usize;
    buf.put_str(
        sx,
        cy0,
        &fit("SYSTEM STATUS", status_w, g.ellipsis),
        Style::fg(Color::ORANGE),
    );
    let after = frame::pill(buf, g, sx, cy0 + 1, status.label(), status.color());
    widgets::pulse(buf, g, after + 1, cy0 + 1, scene.frame, status.color(), !scene.paused);
    if scene.paused {
        frame::pill(buf, g, after + 10, cy0 + 1, "HOLD", Color::SALMON);
    }
    buf.put_str(sx, cy0 + 3, "UPTIME", dim);
    buf.put_str(
        sx + 8,
        cy0 + 3,
        &t.uptime.map(units::uptime).unwrap_or_else(|| g.unavailable.into()),
        value,
    );
    buf.put_str(sx, cy0 + 4, "LOAD", dim);
    match t.load {
        Some(l) => {
            let mut x = sx + 8;
            for (i, (v, tag)) in l.iter().zip(["1M", "5M", "15M"]).enumerate() {
                let s = units::load(*v);
                buf.put_str(x, cy0 + 4, &s, if i == 0 { Style::fg(Color::AMBER) } else { value });
                buf.put_str(x, cy0 + 5, tag, dim);
                x += (s.len() as i32).max(4) + 2;
            }
        }
        None => {
            buf.put_str(sx + 8, cy0 + 4, g.unavailable, dim);
        }
    }

    // ---- CPU band ------------------------------------------------------
    let mut y = cy0 + 7;
    if y + 4 > cy1 {
        return;
    }
    frame::sweep(buf, g, cx0, y, cx1, "CPU", Color::LAVENDER);
    y += 1;
    let pct_text = match t.cpu {
        Some(c) => format!("{:.0}%", (c * 100.0).round()),
        None => "--%".into(),
    };
    let pct_w = Font::Medium.width("100%");
    let heat = t.cpu.map(Color::heat).unwrap_or(Color::DIM);
    font::draw(buf, g, cx0, y, &pct_text, Font::Medium, Style::fg(heat));
    let trace_x = cx0 + pct_w + 2;
    let right_info_w = if tier == Tier::Standard { 14 } else { 0 };
    let trace_w = cx1 - right_info_w - trace_x;
    let data = t.cpu_history.last_n((trace_w.max(0) * 2) as usize);
    widgets::trace(buf, g, trace_x, y, trace_w, 3, &data, Color::heat);
    if tier == Tier::Standard {
        let ix = cx1 - 12;
        buf.put_str(ix, y, "CORES", dim);
        buf.put_str_right(cx1, y, &format!("{}", scene.info.cores), value);
        buf.put_str(ix, y + 1, "PEAK", dim);
        buf.put_str_right(cx1, y + 1, &units::percent(Some(t.cpu_history.max())), value);
        buf.put_str(ix, y + 2, if g.unicode { "ΔT" } else { "DT" }, dim);
        let dt = t
            .interval
            .map(|d| format!("{:.2}S", d.as_secs_f64()))
            .unwrap_or_else(|| g.unavailable.into());
        buf.put_str_right(cx1, y + 2, &dt, value);
    }
    y += 4;

    // ---- Memory and disk, side by side ---------------------------------
    if y + 3 > cy1 {
        return;
    }
    let half = (cx1 - cx0 - 3) / 2;
    let lx0 = cx0;
    let lx1 = cx0 + half;
    let rx0 = lx1 + 3;
    let rx1 = cx1;
    frame::sweep(buf, g, lx0, y, lx1, "MEMORY", Color::BLUE);
    frame::sweep(buf, g, rx0, y, rx1, "DISK", Color::ORANGE);
    match t.memory {
        Some(m) => {
            let used = m.used as f32 / m.total.max(1) as f32;
            widgets::gauge(buf, g, lx0, y + 1, lx1 - lx0, &[(used, Color::BLUE)], true);
            triple(
                buf,
                lx0,
                lx1,
                y + 2,
                &[
                    ("USED", units::bytes(m.used)),
                    ("AVAIL", units::bytes(m.available)),
                    ("TOTAL", units::bytes(m.total)),
                ],
            );
        }
        None => unavailable(buf, g, lx0, lx1, y + 1, "MEMORY"),
    }
    match &t.disk {
        Some(d) => {
            let used = d.used as f32 / d.total.max(1) as f32;
            widgets::gauge(buf, g, rx0, y + 1, rx1 - rx0, &[(used, Color::ORANGE)], true);
            triple(
                buf,
                rx0,
                rx1,
                y + 2,
                &[
                    ("USED", units::bytes(d.used)),
                    ("FREE", units::bytes(d.available)),
                    ("TOTAL", units::bytes(d.total)),
                ],
            );
        }
        None => unavailable(buf, g, rx0, rx1, y + 1, "DISK"),
    }
    y += 4;

    // ---- Network -------------------------------------------------------
    if y + 2 > cy1 {
        return;
    }
    let trace_h = if tier == Tier::Standard { 3 } else { 1 };
    if tier == Tier::Standard {
        frame::sweep(buf, g, lx0, y, lx1, "NET RX", Color::SALMON);
        frame::sweep(buf, g, rx0, y, rx1, "NET TX", Color::LAVENDER);
    } else {
        frame::sweep(buf, g, cx0, y, cx1, "NET", Color::SALMON);
    }
    y += 1;
    if y + trace_h > cy1 {
        return;
    }
    net_pane(
        buf,
        g,
        lx0,
        lx1,
        y,
        trace_h,
        "RX",
        t.rx_rate,
        &t.rx_history,
        Color::SALMON,
        t.interfaces,
    );
    net_pane(
        buf,
        g,
        rx0,
        rx1,
        y,
        trace_h,
        "TX",
        t.tx_rate,
        &t.tx_history,
        Color::LAVENDER,
        t.interfaces,
    );

    // ---- Sampling readout in the otherwise empty lower-right corner -----
    if tier == Tier::Standard && cy1 - 3 > y + trace_h + 1 {
        let yy = cy1 - 3;
        let st = Style::fg(Color::DIM);
        buf.put_str_right(cx1, yy, &format!("SMP {:06}", t.samples), st);
        buf.put_str_right(cx1, yy + 1, &format!("INTERVAL {}MS", scene.interval_ms), st);
        buf.put_str_right(
            cx1,
            yy + 2,
            &format!(
                "{} {}",
                if g.unicode { "UNICODE" } else { "ASCII" },
                scene.color_mode.label()
            ),
            st,
        );
    }
}

fn triple(buf: &mut Buffer, x0: i32, x1: i32, y: i32, items: &[(&str, String)]) {
    let w = x1 - x0;
    let slot = (w / items.len() as i32).max(1);
    for (i, (label, val)) in items.iter().enumerate() {
        let x = x0 + i as i32 * slot;
        let label = fit(&label.to_uppercase(), 6, "");
        let text = format!("{val} {label}");
        if x + text.len() as i32 <= x1 {
            let end = buf.put_str(x, y, val, Style::fg(Color::IVORY));
            buf.put_str(end + 1, y, &label, Style::fg(Color::DIM));
        }
    }
}

fn unavailable(buf: &mut Buffer, g: &Glyphs, x0: i32, x1: i32, y: i32, what: &str) {
    widgets::gauge(buf, g, x0, y, x1 - x0, &[], false);
    buf.put_str(
        x0,
        y + 1,
        &fit(
            &format!("{what} {} UNAVAILABLE", g.unavailable),
            (x1 - x0).max(0) as usize,
            g.ellipsis,
        ),
        Style::fg(Color::DIM),
    );
}

#[allow(clippy::too_many_arguments)]
fn net_pane(
    buf: &mut Buffer,
    g: &Glyphs,
    x0: i32,
    x1: i32,
    y: i32,
    h: i32,
    tag: &str,
    rate: Option<f64>,
    history: &crate::metrics::rates::History,
    color: Color,
    interfaces: usize,
) {
    let label_w = 9;
    let text_x = x1 - label_w;
    let trace_w = text_x - x0 - 1;
    let peak = history.max().max(1.0);
    let data: Vec<Option<f32>> = history
        .last_n((trace_w.max(0) * 2) as usize)
        .iter()
        .map(|v| v.map(|v| v / peak))
        .collect();
    widgets::trace(buf, g, x0, y, trace_w, h, &data, |_| color);
    match rate {
        Some(r) => {
            buf.put_str_right(x1, y, &units::rate(r), Style::fg(Color::IVORY));
            if h > 1 {
                buf.put_str_right(
                    x1,
                    y + 1,
                    &format!("PK {}", units::rate(peak as f64)),
                    Style::fg(Color::DIM),
                );
                buf.put_str_right(x1, y + 2, &format!("{tag} {interfaces} IF"), Style::fg(Color::DIM));
            }
        }
        None => {
            buf.put_str_right(x1, y, &format!("{tag} {}", g.unavailable), Style::fg(Color::DIM));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_tiny(buf: &mut Buffer, g: &Glyphs, scene: &Scene, status: Status, cx0: i32, cy0: i32, cx1: i32, cy1: i32) {
    let t = scene.telemetry;
    let w = (cx1 - cx0).max(0) as usize;
    let mut y = cy0;
    let line = |buf: &mut Buffer, y: i32, s: &str, st: Style| {
        if y < cy1 {
            buf.put_str(cx0, y, &fit(s, w, g.ellipsis), st);
        }
    };
    line(buf, y, &scene.clock.time(), Style::fg(Color::AMBER).bold());
    y += 1;
    line(buf, y, &scene.clock.date(), Style::fg(Color::LAVENDER));
    y += 1;
    line(buf, y, status.label(), Style::fg(status.color()));
    y += 2;
    line(
        buf,
        y,
        &format!("CPU {}", units::percent(t.cpu).trim()),
        Style::fg(Color::IVORY),
    );
    y += 1;
    if let Some(m) = t.memory {
        line(
            buf,
            y,
            &format!("MEM {}/{}", units::bytes(m.used), units::bytes(m.total)),
            Style::fg(Color::IVORY),
        );
    }
    y += 1;
    if let Some(l) = t.load {
        line(
            buf,
            y,
            &format!("LOAD {} {} {}", units::load(l[0]), units::load(l[1]), units::load(l[2])),
            Style::fg(Color::IVORY),
        );
    }
    y += 2;
    line(buf, y, "ENLARGE TO 60x16", Style::fg(Color::DIM));
}
