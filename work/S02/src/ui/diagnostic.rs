//! The diagnostic view: a mirrored, cool-toned frame (sidebar on the right)
//! holding system identity, the per-core comb, swap, and the top processes.

use super::font::Font;
use super::shapes::{cut_right, pill, Align, Corner, Frame};
use super::theme::*;
use super::Scene;
use crate::canvas::{ellipsize, str_width, Canvas, Rect, Style};
use crate::color::Color;
use crate::format;
use crate::metrics::sampler::ProcSummary;
use crate::metrics::Missing;

pub fn render(c: &mut Canvas, s: &Scene, font: &mut Font) {
    let (w, h) = (c.width(), c.height());
    if w < 44 || h < 14 {
        tiny(c, s);
        return;
    }
    let sw = (w * 2 / 15).clamp(9, 20);
    frame(c, s, sw);
    let content = Rect::new(2, 3, w - sw - 5, h - 5);
    let proc_h = 8.min(content.h);
    // Taller terminals get a middle band of large readouts.
    let mid_h = if content.h >= 26 { 6 } else { 0 };
    let sys_h = (content.h - proc_h - mid_h - if mid_h > 0 { 2 } else { 1 }).clamp(0, 12);
    let top = Rect::new(content.x, content.y, content.w, sys_h);
    let procs = Rect::new(content.x, content.bottom() - proc_h, content.w, proc_h);

    let left_w = (top.w * 45 / 100).clamp(30, 52).min(top.w);
    system(c, s, Rect::new(top.x, top.y, left_w, top.h), mid_h == 0);
    cores(c, s, Rect::new(top.x + left_w + 3, top.y, top.w - left_w - 3, top.h));
    if mid_h > 0 {
        let mid = Rect::new(content.x, top.bottom() + 1, content.w, mid_h);
        process_count(c, s, Rect::new(mid.x, mid.y, left_w, mid.h), font);
        swap_plate(c, s, Rect::new(mid.x + left_w + 3, mid.y, mid.w - left_w - 3, mid.h));
    }
    processes(c, s, procs);
}

fn frame(c: &mut Canvas, s: &Scene, sw: i32) {
    let (w, h) = (c.width(), c.height());
    let mut f = Frame::new(w, h);
    let x_side = w - sw;
    let elbow_start = w - sw - (w - sw) * 30 / 100;
    // Top rail from a rounded left cap into the top-right elbow.
    f.rounded_px((0, 0, 2 * (elbow_start - 1), 3), (3, 1), [true, false, false, true], LAVENDER);
    f.elbow_px(2 * elbow_start, 0, 2 * w, 6, 2 * sw, 3, Corner::TopRight, (8, 4), (5, 3), LILAC);
    // Right sidebar blocks.
    let labels = ["SYS", "CPU", "SWP", "PRC"];
    let colors = [BLUE, LAVENDER, ICE, LILAC];
    let (top, bottom) = (3, h - 4);
    let bh = ((bottom - top + 1) / labels.len() as i32).clamp(3, 6);
    let mut y = top;
    let mut placed = Vec::new();
    for (i, col) in colors.iter().enumerate() {
        if y + bh - 1 > bottom {
            break;
        }
        f.rect_px(2 * x_side, 2 * y + 1, 2 * w, 2 * (y + bh), *col);
        placed.push((i, y + bh - 1));
        y += bh;
    }
    if y <= bottom {
        f.rect_px(2 * x_side, 2 * y + 1, 2 * w, 2 * (bottom + 1), TRACK_COOL.lerp(BLUE, 0.5));
    }
    let foot = w - sw - 4;
    f.elbow_px(2 * foot, 2 * (h - 3) + 1, 2 * w, 2 * h, 2 * sw, 2, Corner::BottomRight, (8, 4), (4, 2), LAVENDER);
    f.rounded_px((0, 2 * (h - 1), 2 * (foot - 1), 2 * h), (3, 1), [true, false, false, true], BLUE);
    f.blit(c);

    let host = s.tel.hostname.as_ref().map(|h| h.to_uppercase()).unwrap_or_else(|m| m.short().to_string());
    c.label(2, 0, &ellipsize(&format!("DIAGNOSTIC · {host}"), elbow_start - 4), INK, true);
    cut_right(c, w - 3, 0, "02", true);
    let values = [
        s.sys.arch.to_uppercase(),
        s.tel.core_pct.as_ref().map(|v| v.len().to_string()).unwrap_or_else(|m| m.short().into()),
        match &s.tel.swap {
            Ok(sw) if sw.total > 0 => format::pct(sw.used as f64 / sw.total as f64 * 100.0),
            Ok(_) => "NONE".into(),
            Err(m) => m.short().into(),
        },
        match &s.tel.procs {
            Some(Ok(p)) => p.total.to_string(),
            Some(Err(m)) => m.short().into(),
            None => Missing::Pending.short().into(),
        },
    ];
    for (i, yb) in placed {
        c.label(x_side + 1, yb, labels[i], INK, true);
        cut_right(c, w - 1, yb, &ellipsize(&values[i], sw - 6), true);
        if yb - 1 > yb - (bh - 1) {
            cut_right(c, w - 1, yb - 1, &format!("{:02}", 21 + i), false);
        }
    }
    let keys = " P RETURN · TAB COMMAND · ? HELP · Q EXIT ";
    c.label(2, h - 1, &ellipsize(keys, foot - 4), INK, true);
    if s.paused {
        cut_right(c, w - 1, h - 1, "HOLD", true);
    }
}

fn section(c: &mut Canvas, r: Rect, title: &str, col: Color) -> Rect {
    let pw = (str_width(title) + 4).min(r.w);
    pill(c, r.x, r.y, pw, col, title, Align::Center);
    c.fill(Rect::new(r.x + pw + 1, r.y, (r.w - pw - 1).max(0), 1), '─', Style::fg(TRACK_COOL.lerp(col, 0.35)));
    Rect::new(r.x, r.y + 1, r.w, r.h - 1)
}

fn procs_text(s: &Scene) -> String {
    match &s.tel.procs {
        Some(Ok(p)) if p.unreadable > 0 => format!("{}  ({} RESTRICTED)", p.total, p.unreadable),
        Some(Ok(p)) => p.total.to_string(),
        Some(Err(m)) => m.short().into(),
        None => "ACQUIRING".into(),
    }
}

fn system(c: &mut Canvas, s: &Scene, r: Rect, with_extras: bool) {
    if r.h < 2 {
        return;
    }
    let body = section(c, r, "SYSTEM", BLUE);
    let up = s.tel.uptime.map(format::uptime).unwrap_or_else(|m| m.short().into());
    let host = s.tel.hostname.clone().unwrap_or_else(|m| m.short().into());
    let cpus = s.tel.core_pct.as_ref().map(|v| format!("{} LOGICAL", v.len())).unwrap_or_else(|m| m.short().into());
    let mut rows: Vec<(&str, String)> = vec![
        ("HOST", host),
        ("OS", s.sys.os.clone()),
        ("KERNEL", s.sys.kernel.clone()),
        ("ARCH", s.sys.arch.clone()),
        ("CPUS", cpus),
        ("UPTIME", up),
    ];
    if with_extras {
        rows.push(("PROCS", procs_text(s)));
    }
    let mut y = body.y + 1;
    for (k, v) in rows.iter() {
        if y >= body.bottom() {
            return;
        }
        c.text(body.x + 1, y, k, Style::fg(DIM));
        c.text_max(body.x + 9, y, v, Style::fg(ICE).bold(), body.w - 9);
        y += 1;
    }
    if !with_extras || y >= body.bottom() {
        return;
    }
    c.text(body.x + 1, y, "SWAP", Style::fg(DIM));
    match &s.tel.swap {
        Ok(sw) if sw.total > 0 => {
            let f = sw.used as f64 / sw.total as f64;
            let text = format!("{} / {}", format::bytes(sw.used), format::bytes(sw.total));
            let gw = (body.w - 11 - str_width(&text) - 1).clamp(0, 20);
            rail(c, body.x + 9, y, gw, f, LAVENDER);
            c.text_max(body.x + 10 + gw, y, &text, Style::fg(LAVENDER).bold(), body.w - 10 - gw);
        }
        Ok(_) => {
            c.text(body.x + 9, y, "NOT CONFIGURED", Style::fg(DIM).bold());
        }
        Err(m) => {
            c.text(body.x + 9, y, m.short(), Style::fg(DIM).bold());
        }
    }
}

fn process_count(c: &mut Canvas, s: &Scene, r: Rect, font: &mut Font) {
    let body = section(c, r, "PROCESSES", LILAC);
    match &s.tel.procs {
        Some(Ok(p)) => {
            let digits = p.total.to_string();
            let dw = font.draw(c, (body.x + 1, body.y + 1), &digits, (3, 4), (LILAC, ICE));
            let tx = body.x + dw + 4;
            c.text_max(tx, body.y + 2, "RUNNING TASKS", Style::fg(DIM), body.right() - tx);
            let note = if p.unreadable > 0 { format!("{} RESTRICTED", p.unreadable) } else { "ALL READABLE".into() };
            c.text_max(tx, body.y + 3, &note, Style::fg(LAVENDER).bold(), body.right() - tx);
        }
        Some(Err(m)) => {
            c.text(body.x + 1, body.y + 2, &format!("PROCESS TABLE {}", m.short()), Style::fg(DIM).bold());
        }
        None => {
            c.text(body.x + 1, body.y + 2, "ACQUIRING PROCESS TABLE", Style::fg(DIM).bold());
        }
    }
}

fn swap_plate(c: &mut Canvas, s: &Scene, r: Rect) {
    if r.w < 16 {
        return;
    }
    let body = section(c, r, "SWAP", ICE);
    let gw = (body.w - 2).min(48);
    let (frac, text, col) = match &s.tel.swap {
        Ok(sw) if sw.total > 0 => {
            let f = sw.used as f64 / sw.total as f64;
            (Some(f), format!("USED {}  OF {}  ·  {}", format::bytes(sw.used), format::bytes(sw.total), format::pct(f * 100.0)), ICE)
        }
        Ok(_) => (None, "NOT CONFIGURED ON THIS SYSTEM".to_string(), DIM),
        Err(m) => (None, format!("SWAP {}", m.short()), DIM),
    };
    // A two-row rounded plate: dim track with a bright fill.
    let mut f = Frame::new(gw, 2);
    f.rounded_px((0, 0, 2 * gw, 4), (4, 2), [true, true, true, true], TRACK_COOL);
    if let Some(fr) = frac {
        let fill = ((2 * gw) as f64 * fr.clamp(0.0, 1.0)).round() as i32;
        if fill > 0 {
            f.rounded_px((0, 0, fill.max(4), 4), (4, 2), [true, fill >= 2 * gw - 2, fill >= 2 * gw - 2, true], ICE);
        }
    }
    f.blit_at(c, body.x + 1, body.y + 1);
    c.text_max(body.x + 1, body.y + 4, &text, Style::fg(col).bold(), body.w - 1);
}

/// A thin horizontal gauge from half blocks: filled part bright, track dim.
fn rail(c: &mut Canvas, x: i32, y: i32, w: i32, f: f64, col: Color) {
    if w <= 0 {
        return;
    }
    let filled = (f.clamp(0.0, 1.0) * w as f64 * 2.0).round() as i32; // half-cell resolution
    for i in 0..w {
        let (ch, fg) = match (filled - 2 * i).clamp(0, 2) {
            2 => ('▬', col),
            1 => ('▬', col.lerp(TRACK_COOL, 0.5)),
            _ => ('▬', TRACK_COOL),
        };
        c.put(x + i, y, ch, Style::fg(fg));
    }
}

const EIGHTHS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

fn cores(c: &mut Canvas, s: &Scene, r: Rect) {
    if r.w < 10 || r.h < 3 {
        return;
    }
    let pcts = match &s.tel.core_pct {
        Ok(v) => v.clone(),
        Err(m) => {
            let body = section(c, r, "CPU CORES", LAVENDER);
            c.text(body.x + 1, body.y + 1, &format!("PER-CORE {}", m.short()), Style::fg(DIM).bold());
            return;
        }
    };
    let avg = if pcts.is_empty() { 0.0 } else { pcts.iter().sum::<f64>() / pcts.len() as f64 };
    let body = section(c, r, &format!("CPU CORES · {} · AVG {}", pcts.len(), format::pct(avg)), LAVENDER);
    let body = Rect::new(body.x, body.y + 1, body.w, body.h - 1);
    let n = pcts.len() as i32;
    if n == 0 || body.h < 2 {
        return;
    }
    // Pitch 3 = two-column bar + gap; fall back to thinner bars, then rows.
    let pitch = if n * 3 <= body.w { 3 } else { 2 };
    let per_row = (body.w / pitch).max(1);
    let comb_rows = (n + per_row - 1) / per_row;
    let band = body.h / comb_rows.max(1);
    let bar_h = (band - 1).clamp(1, 8);
    for (i, &p) in pcts.iter().enumerate() {
        let i = i as i32;
        let (row, col) = (i / per_row, i % per_row);
        let x = body.x + col * pitch;
        let base = body.y + row * band + bar_h - 1;
        if base >= body.bottom() {
            break;
        }
        let eighths = (p.clamp(0.0, 100.0) / 100.0 * (bar_h * 8) as f64).round() as i32;
        for k in 0..bar_h {
            let y = base - k;
            let v = (eighths - k * 8).clamp(0, 8) as usize;
            let frac = (k as f32 + 0.5) / bar_h as f32;
            let col = BLUE.lerp(LAVENDER, frac).lerp(ORANGE, (frac - 0.6).max(0.0) * 2.0);
            // Empty slots stay visible as dim columns so idle cores still read.
            let style = match v {
                0 => Style::on(TRACK_COOL, TRACK_COOL),
                8 => Style::on(col, col),
                _ => Style::on(col, TRACK_COOL),
            };
            for dx in 0..(pitch - 1) {
                c.put(x + dx, y, if v == 0 || v == 8 { ' ' } else { EIGHTHS[v] }, style);
            }
        }
        let label_every = if pitch == 3 { 1 } else { 4 };
        if bar_h >= 2 && i % label_every == 0 && base + 1 < body.bottom() + 1 {
            c.text(x, base + 1, &format!("{}", i % 100), Style::fg(DIM));
        }
    }
}

fn processes(c: &mut Canvas, s: &Scene, r: Rect) {
    if r.h < 2 {
        return;
    }
    let body = section(c, r, "TOP PROCESSES · CPU", ORANGE);
    let summary: Result<&ProcSummary, String> = match &s.tel.procs {
        Some(Ok(p)) => Ok(p),
        Some(Err(Missing::Denied)) => Err("PROCESS TABLE ACCESS DENIED".into()),
        Some(Err(m)) => Err(format!("PROCESS TABLE {}", m.short())),
        None => Err("ACQUIRING PROCESS TABLE".into()),
    };
    let p = match summary {
        Ok(p) => p,
        Err(msg) => {
            c.text_max(body.x + 1, body.y + 1, &msg, Style::fg(DIM).bold(), body.w - 1);
            return;
        }
    };
    let name_w = (body.w - 8 - 2 - 18 - 2 - 16).clamp(8, 40);
    let x_name = body.x + 9;
    let x_cpu = x_name + name_w + 2;
    let x_mem = x_cpu + 18 + 2;
    let hdr = Style::fg(DIM);
    c.text(body.x + 2, body.y, "PID", hdr);
    c.text(x_name, body.y, "NAME", hdr);
    c.text(x_cpu, body.y, if p.warming { "CPU  CALIBRATING" } else { "CPU" }, hdr);
    c.text_max(x_mem, body.y, "MEMORY", hdr, body.right() - x_mem);
    if p.top.is_empty() {
        c.text(body.x + 1, body.y + 1, "NO READABLE PROCESSES", Style::fg(DIM).bold());
    }
    let cols = [ORANGE, GOLD, AMBER, PEACH, SALMON];
    for (i, t) in p.top.iter().enumerate() {
        let y = body.y + 1 + i as i32;
        if y >= body.bottom() {
            break;
        }
        let col = cols[i % cols.len()];
        c.fill(Rect::new(body.x, y, 8, 1), ' ', Style::on(INK, col));
        cut_right(c, body.x + 7, y, &t.pid.to_string(), true);
        c.text_max(x_name, y, &ellipsize(&t.name, name_w), Style::fg(PEACH).bold(), name_w);
        let cpu_txt = format!("{:>5.1}%", t.cpu_pct);
        c.text(x_cpu, y, &cpu_txt, Style::fg(col).bold());
        rail(c, x_cpu + 7, y, 11, t.cpu_pct / 100.0, col);
        let mem = match t.mem_pct {
            Some(m) => format!("{:>6}  {:>4.1}%", format::bytes(t.rss_bytes), m),
            None => format::bytes(t.rss_bytes),
        };
        c.text_max(x_mem, y, &mem, Style::fg(LAVENDER), body.right() - x_mem);
    }
}

fn tiny(c: &mut Canvas, s: &Scene) {
    let (w, h) = (c.width(), c.height());
    if w <= 0 || h <= 0 {
        return;
    }
    c.fill(Rect::new(0, 0, w, 1), ' ', Style::on(INK, LILAC));
    c.label(1, 0, &ellipsize("DIAGNOSTIC", w - 2), INK, true);
    let mut y = 1;
    c.text_max(1, y, &format!("{} {}", s.sys.os, s.sys.arch), Style::fg(ICE), w - 1);
    y += 1;
    if let Some(Ok(p)) = &s.tel.procs {
        for t in &p.top {
            if y >= h {
                break;
            }
            c.text_max(1, y, &format!("{:>6} {:>5.1}% {}", t.pid, t.cpu_pct, t.name), Style::fg(PEACH), w - 1);
            y += 1;
        }
    }
}
