//! The diagnostic view: a mirrored silhouette (rail on the right) with the
//! process count as its focal readout, a bank of per-core bars, swap, and
//! the five busiest processes as pill rows. Never shown by default.

use super::font::{self, Font};
use super::frame::{self, FrameSpec, RailBlock, Segment, Side};
use super::widgets;
use super::{key_segments, Scene, Tier};
use crate::metrics::units;
use crate::term::buffer::fit;
use crate::term::{Buffer, Color, Glyphs, Style};

pub fn draw(buf: &mut Buffer, g: &Glyphs, scene: &Scene, tier: Tier) {
    let t = scene.telemetry;
    let rail_w = tier.rail_width();
    let host = g.text(&scene.info.hostname).to_uppercase();
    let top = match tier {
        Tier::Tiny => vec![
            Segment::new(Color::LAVENDER, "", rail_w + 2),
            Segment::new(Color::ORANGE, "DIAG", 0),
        ],
        _ => vec![
            Segment::new(Color::LAVENDER, "", rail_w + 12),
            Segment::new(Color::ORANGE, scene.view.code(), 14),
            Segment::new(Color::SALMON, host.as_str(), 0),
        ],
    };
    let bottom = key_segments(scene, tier);
    let swap_fill = t.swap.map(|s| s.used as f32 / s.total.max(1) as f32);
    let swap_pct = units::percent(swap_fill);
    let rail = vec![
        RailBlock::new(Color::LAVENDER, 2.0, &["DIAG"]),
        RailBlock::new(Color::BLUE, 2.0, &["CORES", &scene.info.cores.to_string()]),
        RailBlock::new(Color::SALMON, 1.5, &["SWAP", swap_pct.trim()]).with_fill(swap_fill),
        RailBlock::new(
            Color::ORANGE,
            2.5,
            &[
                "PROC",
                &t.process_count
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| g.unavailable.into()),
            ],
        ),
    ];
    let scan = if g.unicode {
        Some(1.0 - ((scene.frame % 64) as f32) / 64.0)
    } else {
        None
    };
    let spec = FrameSpec {
        side: Side::Right,
        rail_w,
        top: &top,
        bottom: &bottom,
        rail: &rail,
        scan,
    };
    let (cx0, cy0, cx1, cy1) = frame::draw(buf, g, &spec);
    let dim = Style::fg(Color::DIM);
    let value = Style::fg(Color::IVORY);
    let width = (cx1 - cx0).max(0) as usize;

    if tier == Tier::Tiny {
        let mut y = cy0;
        for line in [
            format!("{} {}", scene.info.os, scene.info.arch),
            format!(
                "PROCS {}",
                t.process_count
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| g.unavailable.into())
            ),
        ] {
            if y < cy1 {
                buf.put_str(cx0, y, &fit(&line, width, g.ellipsis), value);
            }
            y += 1;
        }
        for p in t.top_processes.iter().take(5) {
            if y < cy1 {
                buf.put_str(
                    cx0,
                    y,
                    &fit(
                        &format!("{} {}", units::percent(p.cpu_pct.map(|c| c / 100.0)).trim(), p.name),
                        width,
                        g.ellipsis,
                    ),
                    value,
                );
            }
            y += 1;
        }
        return;
    }

    // ---- Identity + focal process count --------------------------------
    let count_text = t.process_count.map(|c| c.to_string()).unwrap_or_else(|| "---".into());
    let count_w = Font::Medium.width(&count_text);
    let count_x = cx1 - count_w;
    font::draw(
        buf,
        g,
        count_x,
        cy0,
        &count_text,
        Font::Medium,
        Style::fg(Color::ORANGE),
    );
    buf.put_str_right(cx1, cy0 + 3, "PROCESSES", dim);
    let id_w = (count_x - 2 - cx0).max(0) as usize;
    // Short terminals get a two-line identity block and a squatter core bank
    // so the process table, the point of this view, stays on screen.
    let short = cy1 - cy0 < 20;
    let mut y = if short {
        buf.put_str(cx0, cy0, &fit(&scene.info.os.to_uppercase(), id_w, g.ellipsis), value);
        buf.put_str(
            cx0,
            cy0 + 1,
            &fit(
                &format!(
                    "{}  {}",
                    scene.info.kernel.to_uppercase(),
                    scene.info.arch.to_uppercase()
                ),
                id_w,
                g.ellipsis,
            ),
            Style::fg(Color::LAVENDER),
        );
        cy0 + 3
    } else {
        buf.put_str(
            cx0,
            cy0,
            &fit("OPERATING SYSTEM", id_w, g.ellipsis),
            Style::fg(Color::ORANGE),
        );
        buf.put_str(
            cx0,
            cy0 + 1,
            &fit(&scene.info.os.to_uppercase(), id_w, g.ellipsis),
            value,
        );
        buf.put_str(
            cx0,
            cy0 + 2,
            &fit(
                &format!(
                    "KERNEL {}  {}",
                    scene.info.kernel.to_uppercase(),
                    scene.info.arch.to_uppercase()
                ),
                id_w,
                g.ellipsis,
            ),
            Style::fg(Color::LAVENDER),
        );
        buf.put_str(cx0, cy0 + 3, &fit(&format!("HOST {}", host), id_w, g.ellipsis), dim);
        cy0 + 5
    };

    // ---- Core bank -----------------------------------------------------
    if y + 2 > cy1 {
        return;
    }
    frame::sweep(buf, g, cx0, y, cx1, "CORES", Color::BLUE);
    y += 1;
    let bank_h = if tier == Tier::Standard {
        4
    } else if short {
        2
    } else {
        3
    };
    let bank_w = cx1 - cx0;
    if t.cores.is_empty() {
        buf.put_str(cx0, y, &format!("PER-CORE {} UNAVAILABLE", g.unavailable), dim);
        y += 2;
    } else {
        // Wide banks get labelled two-column bars; many cores on a narrow
        // screen pack tighter rather than wrapping the list off-screen.
        let pitch = widgets::core_pitch(t.cores.len(), bank_w);
        let shown = widgets::core_bank(buf, g, cx0, y, bank_w, bank_h, &t.cores, pitch, true);
        if pitch < 3 {
            buf.put_str(cx0, y + bank_h, "00", dim);
            buf.put_str_right(
                cx0 + shown as i32 * pitch - (pitch - 1).max(0) + (if pitch == 1 { 1 } else { 0 }),
                y + bank_h,
                &format!("{:02}", shown.saturating_sub(1)),
                dim,
            );
        }
        if shown < t.cores.len() {
            buf.put_str_right(cx1, y + bank_h, &format!("+{} MORE", t.cores.len() - shown), dim);
        }
        y += bank_h + 2;
    }

    // ---- Swap + sampling, side by side (dropped when rows are scarce) ----
    let procs_rows = 2 + t.top_processes.len().clamp(1, 5) as i32;
    if y + 3 > cy1 {
        return;
    }
    if y + 4 + procs_rows > cy1 {
        draw_processes(buf, g, scene, tier, cx0, cx1, y, cy1);
        return;
    }
    let half = (cx1 - cx0 - 3) / 2;
    let (lx0, lx1, rx0, rx1) = (cx0, cx0 + half, cx0 + half + 3, cx1);
    frame::sweep(buf, g, lx0, y, lx1, "SWAP", Color::SALMON);
    frame::sweep(buf, g, rx0, y, rx1, "SAMPLING", Color::LAVENDER);
    match t.swap {
        Some(s) => {
            let used = s.used as f32 / s.total.max(1) as f32;
            widgets::gauge(buf, g, lx0, y + 1, lx1 - lx0, &[(used, Color::SALMON)], true);
            buf.put_str(lx0, y + 2, &format!("{} USED", units::bytes(s.used)), value);
            buf.put_str_right(lx1, y + 2, &format!("{} TOTAL", units::bytes(s.total)), dim);
        }
        None => {
            widgets::gauge(buf, g, lx0, y + 1, lx1 - lx0, &[], false);
            buf.put_str(
                lx0,
                y + 2,
                &fit(
                    &format!("NO SWAP {}", g.unavailable),
                    (lx1 - lx0).max(0) as usize,
                    g.ellipsis,
                ),
                dim,
            );
        }
    }
    let dt = t
        .interval
        .map(|d| format!("{:.3}S", d.as_secs_f64()))
        .unwrap_or_else(|| g.unavailable.into());
    buf.put_str(
        rx0,
        y + 1,
        &fit(
            &format!(
                "{} {dt}   SAMPLES {}   IF {}",
                if g.unicode { "ΔT" } else { "DT" },
                t.samples,
                t.interfaces
            ),
            (rx1 - rx0).max(0) as usize,
            g.ellipsis,
        ),
        value,
    );
    let hold = if scene.paused { "HOLD" } else { "LIVE" };
    let after = buf.put_str(rx0, y + 2, &format!("{} {}MS  ", hold, scene.interval_ms), dim);
    widgets::pulse(buf, g, after, y + 2, scene.frame, Color::LAVENDER, !scene.paused);
    y += 4;
    draw_processes(buf, g, scene, tier, cx0, cx1, y, cy1);
}

#[allow(clippy::too_many_arguments)]
fn draw_processes(buf: &mut Buffer, g: &Glyphs, scene: &Scene, tier: Tier, cx0: i32, cx1: i32, mut y: i32, cy1: i32) {
    let t = scene.telemetry;
    let dim = Style::fg(Color::DIM);
    let value = Style::fg(Color::IVORY);
    let width = (cx1 - cx0).max(0) as usize;
    if y + 1 > cy1 {
        return;
    }
    frame::sweep(buf, g, cx0, y, cx1, "TOP PROCESSES BY CPU", Color::ORANGE);
    y += 1;
    if t.top_processes.is_empty() {
        if y < cy1 {
            buf.put_str(
                cx0,
                y,
                &fit(&format!("{} NO PROCESS DATA", g.unavailable), width, g.ellipsis),
                dim,
            );
        }
        return;
    }
    let pid_w = 8;
    let bar_w = if tier == Tier::Standard { 12 } else { 6 };
    let tail_w = 6 + 1 + 6; // "99.9%" + gap + "1.2G"
    let name_w = (cx1 - cx0 - (pid_w + 2) - bar_w - 2 - tail_w - 2).max(4) as usize;
    for p in t.top_processes.iter().take(5) {
        if y >= cy1 {
            break;
        }
        let pid = format!("{:>6}", p.pid);
        let after = frame::pill(buf, g, cx0, y, &pid, Color::LAVENDER);
        let name = fit(&g.text(&p.name), name_w, g.ellipsis);
        buf.put_str(after + 1, y, &name, value);
        let bar_x = after + 1 + name_w as i32 + 2;
        let frac = p.cpu_pct.map(|c| (c / 100.0).clamp(0.0, 1.0)).unwrap_or(0.0);
        widgets::gauge(buf, g, bar_x, y, bar_w, &[(frac, Color::heat(frac))], false);
        let cpu = match p.cpu_pct {
            Some(c) => format!("{c:>5.1}%"),
            None => format!("{:>5}%", g.unavailable),
        };
        buf.put_str(bar_x + bar_w + 2, y, &cpu, Style::fg(Color::AMBER));
        let rss = units::bytes(p.rss);
        buf.put_str_right(cx1, y, &rss, Style::fg(Color::BLUE));
        y += 1;
    }
}
