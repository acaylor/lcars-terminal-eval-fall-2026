use crate::buffer::Buffer;
use crate::clock::Clock;
use crate::glyphs::{
    arrow, big_text_width, braille_trace, draw_big_text, elbow_capsule, meter, pill,
    segmented_meter, sparkline, tag, thin_meter,
};
use crate::layout::{Density, DiagnosticsLayout, PrimaryLayout, Rect};
use crate::model::{
    format_bytes_compact, format_load, format_pct, format_rate, format_uptime,
    format_uptime_compact, sanitize_ui_text, Metrics,
};
use crate::theme::{Ink, Theme, BOLD, DIM};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Command,
    Diagnostics,
}

pub struct UiState<'a> {
    pub metrics: &'a Metrics,
    pub clock: Option<&'a Clock>,
    pub paused: bool,
    pub view: View,
    pub help: bool,
    pub phase: u64,
    pub cpu_history: &'a [f32],
    pub net_rx_history: &'a [f64],
    pub net_tx_history: &'a [f64],
    pub unicode: bool,
    pub theme: &'a Theme,
    pub interval_secs: f32,
}

pub fn render_frame(state: &UiState<'_>, w: i32, h: i32) -> Buffer {
    let mut b = Buffer::new(w, h, !state.unicode);
    if w <= 0 || h <= 0 {
        return b;
    }
    let layout = PrimaryLayout::new(w, h);
    match state.view {
        View::Command => render_primary(&mut b, state, &layout),
        View::Diagnostics => {
            let dl = DiagnosticsLayout::new(w, h);
            render_diagnostics(&mut b, state, &dl);
        }
    }
    if state.help {
        render_help(&mut b, state, w, h);
    }
    b
}

fn render_primary(b: &mut Buffer, s: &UiState<'_>, l: &PrimaryLayout) {
    match l.density {
        Density::Tiny => render_primary_tiny(b, s, l),
        Density::Compact => render_primary_compact(b, s, l),
        Density::Full => render_primary_full(b, s, l),
    }
}

fn header_specs(
    w: i32,
    diagnostics: bool,
    density: Density,
) -> (Vec<Rect>, Vec<Ink>, Vec<&'static str>) {
    let (weights, gap, margin, labels, colors) = if density == Density::Tiny {
        (vec![], 0, 0, vec![], vec![])
    } else if density == Density::Compact {
        (
            vec![0.40, 0.60],
            2,
            2,
            if diagnostics {
                vec!["LCARS 47", "DIAGNOSTICS"]
            } else {
                vec!["LCARS 47", "ENGINEERING"]
            },
            vec![Ink::Orange, Ink::Lavender],
        )
    } else if diagnostics {
        (
            vec![0.22, 0.28, 0.28, 0.22],
            2,
            4,
            vec!["LCARS 47", "DIAGNOSTICS", "PROCESS MATRIX", "DECK 07"],
            vec![Ink::Orange, Ink::Lavender, Ink::Salmon, Ink::Blue],
        )
    } else {
        (
            vec![0.22, 0.31, 0.25, 0.22],
            2,
            4,
            vec!["LCARS 47", "ENGINEERING", "OPS // 2380", "DECK 07"],
            vec![Ink::Orange, Ink::Lavender, Ink::Salmon, Ink::Blue],
        )
    };
    if w <= 0 {
        return (vec![], vec![], vec![]);
    }
    (segment_rects(w, &weights, gap, margin), colors, labels)
}

fn segment_rects(w: i32, weights: &[f32], gap: i32, margin: i32) -> Vec<Rect> {
    let n = weights.len() as i32;
    if n == 0 {
        return vec![];
    }
    let usable = (w - margin - gap * (n - 1)).max(n * 4);
    let mut rects = Vec::with_capacity(n as usize);
    let mut x = 0;
    let mut used = 0;
    for (i, weight) in weights.iter().enumerate() {
        let width = if i as i32 == n - 1 {
            (usable - used).max(4)
        } else {
            ((usable as f32 * weight).round() as i32).max(4)
        };
        used += width;
        rects.push(Rect::new(x, 0, width, 0));
        x += width + gap;
    }
    rects
}

fn draw_header(b: &mut Buffer, uni: bool, diagnostics: bool, density: Density) {
    let (rects, colors, labels) = header_specs(b.width, diagnostics, density);
    if rects.is_empty() {
        return;
    }
    let header_h = if density == Density::Compact { 2 } else { 3 };
    for i in 0..rects.len() {
        let mut r = rects[i];
        r.y = 0;
        r.h = header_h;
        pill(b, r.x, r.y, r.w, r.h, colors[i], uni);
        if r.w >= labels[i].chars().count() as i32 + 2 {
            let ty = r.y + (r.h - 1) / 2;
            b.text_center_keep_bg(r.x, ty, r.w, labels[i], Ink::Ink, BOLD);
        } else if r.w >= 4 {
            let text = sanitize_ui_text(labels[i], (r.w - 2) as usize);
            let ty = r.y + (r.h - 1) / 2;
            b.text_center_keep_bg(r.x, ty, r.w, &text, Ink::Ink, BOLD);
        }
    }
}

fn draw_spine(b: &mut Buffer, r: Rect, specs: &[(Ink, &str)], uni: bool) {
    if r.is_empty() || specs.is_empty() {
        return;
    }
    let n = specs.len() as i32;
    for i in 0..n {
        let y0 = r.y + (i * r.h) / n;
        let y1 = r.y + ((i + 1) * r.h) / n;
        let sh = (y1 - y0).max(1);
        let color = specs[i as usize].0;
        pill(b, r.x, y0, r.w, sh, color, uni);
        if r.w >= 5 && sh >= 3 {
            b.text_center_keep_bg(
                r.x + 1,
                y0 + sh / 2,
                r.w - 2,
                specs[i as usize].1,
                Ink::Ink,
                BOLD,
            );
        }
    }
}

fn draw_footer(b: &mut Buffer, r: Rect, uni: bool, entries: &[(&str, &str, Ink)], right: &str) {
    if r.is_empty() {
        return;
    }
    let y = r.y;
    let mut x = 4;
    let reserve = if r.w >= 100 { 26 } else { 0 };
    for (key, label, color) in entries {
        let need = key.chars().count() as i32 + label.chars().count() as i32 + 5;
        if x + need > r.w - reserve {
            break;
        }
        let tw = tag(b, x, y, key, Ink::Ink, *color, uni);
        x += tw;
        b.text(x + 1, y, label, Ink::Dim, Ink::Default, 0);
        x += label.chars().count() as i32 + 3;
    }
    if !right.is_empty() && r.w >= 100 {
        b.text_right(0, y, r.w - 3, right, Ink::Dim, Ink::Default, 0);
    }
}

fn status_text(s: &UiState<'_>) -> (&'static str, Ink) {
    if s.metrics.collect_error.is_some() {
        ("SENSOR FAULT", Ink::Salmon)
    } else if s.paused {
        ("HOLD", Ink::Salmon)
    } else if s.metrics.cpu.is_none() {
        ("SYNC", Ink::Amber)
    } else {
        ("LIVE", Ink::Amber)
    }
}

fn draw_status(b: &mut Buffer, x: i32, y: i32, width: i32, s: &UiState<'_>) {
    let (label, color) = status_text(s);
    let mut cx = x;
    if label == "HOLD" {
        b.text(
            cx,
            y,
            if b.ascii { "||" } else { "❚❚" },
            color,
            Ink::Default,
            BOLD,
        );
        cx += 2;
    } else {
        crate::glyphs::pulse(b, cx, y, s.phase, color);
        cx += 1;
    }
    cx = b.text(cx + 1, y, label, color, Ink::Default, BOLD);
    let interval = if s.interval_secs > 0.0 {
        format!("SAMPLE {:.2}s", s.interval_secs)
    } else {
        "SAMPLE --".to_string()
    };
    b.text(cx + 2, y, &interval, Ink::Dim, Ink::Default, 0);

    let mode = match (s.unicode, s.theme.mono) {
        (true, true) => "UTF8/MONO",
        (true, false) => "UTF8/24BIT",
        (false, true) => "ASCII/MONO",
        (false, false) => "ASCII/24BIT",
    };
    let mode = if s.theme.capability == crate::theme::ColorCapability::Ansi256 {
        mode.replace("24BIT", "256")
    } else if s.theme.capability == crate::theme::ColorCapability::Ansi16 {
        mode.replace("24BIT", "16COL")
    } else {
        mode.to_string()
    };
    if width >= 55 {
        b.text_right(x, y, width, &mode, Ink::Dim, Ink::Default, 0);
    }
}

fn clock_text(s: &UiState<'_>) -> String {
    s.clock
        .map(|c| c.hms())
        .unwrap_or_else(|| "--:--:--".to_string())
}

fn host_text(s: &UiState<'_>) -> String {
    sanitize_ui_text(s.metrics.hostname.as_deref().unwrap_or("UNKNOWN"), 28)
}

fn draw_clock(b: &mut Buffer, s: &UiState<'_>, r: Rect, scale: i32) {
    if s.clock.is_none() {
        b.text(r.x, r.y + 3, "TIME N/A", Ink::Amber, Ink::Default, BOLD);
        return;
    }
    let text = clock_text(s);
    draw_big_text(b, r.x, r.y, &text, scale, Ink::Amber, Ink::Default, BOLD);
}

fn draw_sweep(b: &mut Buffer, s: &UiState<'_>, r: Rect) {
    if r.is_empty() {
        return;
    }
    let uni = !b.ascii;
    let track = if uni { '─' } else { '-' };
    let mark = if uni { '━' } else { '=' };
    b.hline(r.x, r.y, r.w, track, Ink::Dim, Ink::Default);
    let sec = s.clock.map(|c| c.second).unwrap_or(0) as f32;
    let len = (r.w - 1).max(1) as f32;
    let pos = r.x + ((sec / 59.0) * len).round() as i32;
    for dx in -1..=1 {
        let x = pos + dx;
        if x >= r.x && x < r.right() {
            b.put(x, r.y, mark, Ink::Amber, Ink::Default, BOLD);
        }
    }
}

fn draw_date_host(b: &mut Buffer, s: &UiState<'_>, l: &PrimaryLayout) {
    let date = s
        .clock
        .map(|c| c.date_upper())
        .unwrap_or_else(|| "DATE N/A".to_string());
    let mut x = b.text(l.clock.x, l.date_y, &date, Ink::White, Ink::Default, BOLD);
    x = b.text(x, l.date_y, "   //   ", Ink::Dim, Ink::Default, 0);
    let up = s
        .metrics
        .uptime_secs
        .map(format_uptime)
        .unwrap_or_else(|| "N/A".to_string());
    b.text(x, l.date_y, "UPTIME ", Ink::Dim, Ink::Default, 0);
    b.text(x + 7, l.date_y, &up, Ink::Lavender, Ink::Default, BOLD);

    let host = host_text(s);
    let hx = b.text(l.clock.x, l.host_y, "HOST // ", Ink::Dim, Ink::Default, 0);
    let _ = b.text(hx, l.host_y, &host, Ink::Blue, Ink::Default, BOLD);
    if l.clock.w >= 45 {
        let status = if s.metrics.collect_error.is_some() {
            "FAULT"
        } else if s.paused {
            "HOLD"
        } else {
            "NOMINAL"
        };
        let text = format!("STATUS {}", status);
        b.text_right(
            l.clock.x,
            l.host_y,
            l.clock.w,
            &text,
            Ink::Dim,
            Ink::Default,
            0,
        );
    }
}

fn draw_memory(b: &mut Buffer, s: &UiState<'_>, r: Rect) {
    if r.is_empty() {
        return;
    }
    let m = s.metrics;
    let used = m
        .mem_used
        .map(format_bytes_compact)
        .unwrap_or_else(|| "N/A".to_string());
    let total = m
        .mem_total
        .map(format_bytes_compact)
        .unwrap_or_else(|| "N/A".to_string());
    let avail = m
        .mem_available
        .map(format_bytes_compact)
        .unwrap_or_else(|| "N/A".to_string());
    let pct = m
        .mem_pct()
        .map(format_pct)
        .unwrap_or_else(|| "N/A".to_string());
    if r.h <= 3 {
        tag(b, r.x, r.y, "MEM", Ink::Ink, Ink::Orange, !b.ascii);
        b.text(
            r.x + 6,
            r.y,
            &format!("{used} / {total}"),
            Ink::White,
            Ink::Default,
            BOLD,
        );
        b.text_right(r.x, r.y, r.w, &pct, Ink::Amber, Ink::Default, BOLD);
        segmented_meter(
            b,
            r.x,
            r.y + 1,
            r.w,
            m.mem_pct().map(|p| p / 100.0),
            Ink::Orange,
        );
        b.text(
            r.x,
            r.y + 2,
            &format!("AVAIL {avail}"),
            Ink::Lavender,
            Ink::Default,
            0,
        );
        return;
    }
    b.put(r.x, r.y, '▌', Ink::Orange, Ink::Default, 0);
    b.text(r.x + 2, r.y, "MEMORY", Ink::Dim, Ink::Default, BOLD);
    b.text_right(r.x, r.y, r.w, "USED / TOTAL", Ink::Dim, Ink::Default, 0);
    let value = format!("{used} / {total}");
    b.text(r.x, r.y + 1, &value, Ink::White, Ink::Default, BOLD);
    b.text_right(
        r.x,
        r.y + 1,
        r.w,
        &format!("AVAIL {avail}"),
        Ink::Lavender,
        Ink::Default,
        0,
    );
    segmented_meter(
        b,
        r.x,
        r.y + 2,
        r.w,
        m.mem_pct().map(|p| p / 100.0),
        Ink::Orange,
    );
    b.text(r.x, r.y + 3, &pct, Ink::Amber, Ink::Default, BOLD);
    b.text(r.x + 6, r.y + 3, "UTILIZED", Ink::Dim, Ink::Default, 0);
    if r.w > 40 {
        b.text_right(
            r.x,
            r.y + 3,
            r.w,
            "PRESSURE NOMINAL",
            Ink::Dim,
            Ink::Default,
            0,
        );
    }
}

fn draw_net_disk(b: &mut Buffer, s: &UiState<'_>, r: Rect, uni: bool) {
    if r.is_empty() {
        return;
    }
    let m = s.metrics;
    let y = r.y;
    tag(b, r.x, y, "NET", Ink::Ink, Ink::Blue, uni);
    b.text(
        r.x + 6,
        y,
        &format!("ALL IF {}", m.net_ifaces),
        Ink::Dim,
        Ink::Default,
        0,
    );
    let down = arrow(b, false);
    let up = arrow(b, true);
    let rx = m
        .net_rx_rate
        .map(format_rate)
        .unwrap_or_else(|| "SYNC".to_string());
    let tx = m
        .net_tx_rate
        .map(format_rate)
        .unwrap_or_else(|| "SYNC".to_string());
    b.put(r.x, y + 1, down, Ink::Blue, Ink::Default, 0);
    b.text(r.x + 2, y + 1, "RX", Ink::Dim, Ink::Default, 0);
    b.text(r.x + 5, y + 1, &rx, Ink::White, Ink::Default, 0);
    b.put(r.x, y + 2, up, Ink::Lavender, Ink::Default, 0);
    b.text(r.x + 2, y + 2, "TX", Ink::Dim, Ink::Default, 0);
    b.text(r.x + 5, y + 2, &tx, Ink::White, Ink::Default, 0);

    let hist = normalized_f64(s.net_rx_history, 32);
    sparkline(b, r.x, y + 3, r.w - 1, &hist, Ink::Blue);

    let disk_pct = m.disk_pct();
    let disk_used = m
        .disk_total
        .zip(m.disk_available)
        .map(|(t, a)| format_bytes_compact(t.saturating_sub(a)))
        .unwrap_or_else(|| "N/A".to_string());
    let disk_total = m
        .disk_total
        .map(format_bytes_compact)
        .unwrap_or_else(|| "N/A".to_string());
    tag(b, r.x, y + 5, "ROOT", Ink::Ink, Ink::Salmon, uni);
    b.text(
        r.x + 7,
        y + 5,
        m.disk_mount.as_deref().unwrap_or("/"),
        Ink::Dim,
        Ink::Default,
        0,
    );
    b.text(
        r.x,
        y + 6,
        &format!("{disk_used} / {disk_total}"),
        Ink::White,
        Ink::Default,
        0,
    );
    b.text_right(
        r.x,
        y + 6,
        r.w,
        &disk_pct
            .map(format_pct)
            .unwrap_or_else(|| "N/A".to_string()),
        Ink::Amber,
        Ink::Default,
        BOLD,
    );
    segmented_meter(b, r.x, y + 7, r.w, disk_pct.map(|p| p / 100.0), Ink::Salmon);
}

fn draw_cpu_capsule(b: &mut Buffer, s: &UiState<'_>, r: Rect, uni: bool) {
    if r.is_empty() {
        return;
    }
    let m = s.metrics;
    elbow_capsule(b, r.x, r.y, r.w, r.h, Ink::Blue, uni);
    let ix = r.x + 2;
    let iw = r.w - 4;
    b.text_keep_bg(ix, r.y + 2, "CORE LOAD", Ink::Ink, BOLD);
    b.text_keep_bg(ix, r.y + 3, "ALL CORES // LIVE", Ink::Ink, DIM);

    match m.cpu_pct() {
        Some(cpu) => {
            let text = format!("{:02.0}", cpu);
            let scale = if text.len() <= 2 && r.w >= 24 { 2 } else { 1 };
            let text_w = big_text_width(&text, scale);
            let tx = ix + ((iw - text_w) / 2).max(0);
            draw_big_text(b, tx, r.y + 5, &text, scale, Ink::Ink, Ink::Default, BOLD);
            b.text_keep_bg(r.right() - 2, r.y + 13, "%", Ink::Ink, BOLD);
            let meter_y = r.y + 15;
            for i in 0..iw {
                let frac = (i as f32 + 0.5) / iw as f32;
                if frac <= cpu / 100.0 {
                    b.put(
                        ix + i,
                        meter_y,
                        if b.ascii { '#' } else { '█' },
                        Ink::White,
                        Ink::Blue,
                        0,
                    );
                } else {
                    b.put(
                        ix + i,
                        meter_y,
                        if b.ascii { '-' } else { '░' },
                        Ink::Ink,
                        Ink::Blue,
                        DIM,
                    );
                }
            }
            b.text_keep_bg(ix, r.y + 17, "CORE STATUS", Ink::Ink, DIM);
            b.text_center_keep_bg(r.x, r.y + 18, r.w, "NOMINAL", Ink::Ink, BOLD);
        }
        None => {
            b.text_keep_bg(ix + 1, r.y + 10, "N/A", Ink::Ink, BOLD);
            b.text_keep_bg(ix, r.y + 12, "AWAITING SAMPLE", Ink::Ink, DIM);
        }
    }
}

fn draw_load_block(b: &mut Buffer, s: &UiState<'_>, l: &PrimaryLayout) {
    if l.clock.w < 30 {
        return;
    }
    let y = l.host_y + 2;
    if y >= l.h - 1 {
        return;
    }
    if let Some(load) = s.metrics.load {
        if l.h >= 36 && l.clock.w >= 60 {
            b.text(
                l.clock.x,
                y - 1,
                "LOAD AVERAGE // 1M",
                Ink::Amber,
                Ink::Default,
                BOLD,
            );
            let text = format_load(load.one);
            let width = big_text_width(&text, 1);
            draw_big_text(b, l.clock.x, y, &text, 1, Ink::Lavender, Ink::Default, BOLD);
            let rx = l.clock.x + width + 5;
            b.text(rx, y, "5M", Ink::Dim, Ink::Default, 0);
            b.text(
                rx + 3,
                y,
                &format_load(load.five),
                Ink::White,
                Ink::Default,
                BOLD,
            );
            b.text(rx, y + 1, "15M", Ink::Dim, Ink::Default, 0);
            b.text(
                rx + 4,
                y + 1,
                &format_load(load.fifteen),
                Ink::White,
                Ink::Default,
                BOLD,
            );
            let tw = (l.clock.right() - rx).max(10);
            b.text(
                rx,
                y + 3,
                "CORE TRACE // 0-100%",
                Ink::Dim,
                Ink::Default,
                DIM,
            );
            let hist = normalized_f32(s.cpu_history, 100.0);
            sparkline(b, rx, y + 4, tw, &hist, Ink::Lavender);
            braille_trace(b, rx, y + 5, tw, s.phase, Ink::Blue);
            return;
        }
    }
    tag(b, l.clock.x, y, "LOAD", Ink::Ink, Ink::Amber, s.unicode);
    b.text(
        l.clock.x + 7,
        y,
        "AVERAGE // 1M  5M  15M",
        Ink::Dim,
        Ink::Default,
        0,
    );
    if let Some(load) = s.metrics.load {
        let mut x = l.clock.x;
        for (label, value) in [("1M", load.one), ("5M", load.five), ("15M", load.fifteen)] {
            x = b.text(x, y + 1, label, Ink::Dim, Ink::Default, 0);
            x = b.text(
                x + 1,
                y + 1,
                &format_load(value),
                Ink::White,
                Ink::Default,
                BOLD,
            );
            x += 4;
        }
    } else {
        b.text(
            l.clock.x,
            y + 1,
            "LOAD DATA UNAVAILABLE",
            Ink::Dim,
            Ink::Default,
            0,
        );
    }
    braille_trace(b, l.clock.x, y + 2, l.clock.w, s.phase, Ink::Blue);
    b.text(
        l.clock.x,
        l.trace.y - 1,
        "CORE TRACE // 0-100%",
        Ink::Dim,
        Ink::Default,
        DIM,
    );
    let hist = normalized_f32(s.cpu_history, 100.0);
    sparkline(b, l.trace.x, l.trace.y, l.trace.w, &hist, Ink::Lavender);
}

fn draw_extended_manifest(b: &mut Buffer, s: &UiState<'_>, l: &PrimaryLayout) {
    if l.h < 44 {
        return;
    }
    let x = l.clock.x;
    let width = (l.cpu.x - x - 4).max(0);
    if width < 30 {
        return;
    }
    let y = (l.memory.bottom() + 3).min(l.h - 5);
    if y <= l.memory.bottom() {
        return;
    }
    let m = s.metrics;
    let mut cx = x;
    cx += tag(b, cx, y, "OS", Ink::Ink, Ink::Lavender, s.unicode);
    let os = {
        let name = m.os_name.as_deref().unwrap_or("N/A");
        let vers = m.os_version.as_deref().unwrap_or("");
        if vers.is_empty() {
            name.to_string()
        } else {
            format!("{name} {vers}")
        }
    };
    cx = b.text(
        cx + 1,
        y,
        &sanitize_ui_text(&os, 20),
        Ink::White,
        Ink::Default,
        0,
    );
    cx += 3;
    if cx + 20 < x + width {
        cx += tag(b, cx, y, "ARCH", Ink::Ink, Ink::Blue, s.unicode);
        b.text(cx + 1, y, &m.arch, Ink::White, Ink::Default, 0);
    }

    let mut cx = x;
    cx += tag(b, cx, y + 1, "KRN", Ink::Ink, Ink::Orange, s.unicode);
    cx = b.text(
        cx + 1,
        y + 1,
        &sanitize_ui_text(m.kernel.as_deref().unwrap_or("N/A"), 24),
        Ink::White,
        Ink::Default,
        0,
    );
    cx += 2;
    if cx + 14 < x + width {
        cx += tag(b, cx, y + 1, "CORES", Ink::Ink, Ink::Lavender, s.unicode);
        b.text(
            cx + 1,
            y + 1,
            &format!("{}C", m.per_core.len()),
            Ink::White,
            Ink::Default,
            0,
        );
        cx += 8;
    }
    if cx + 12 < x + width {
        cx += tag(b, cx, y + 1, "PROC", Ink::Ink, Ink::Amber, s.unicode);
        b.text(
            cx + 1,
            y + 1,
            &m.process_count
                .map(|n| n.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            Ink::White,
            Ink::Default,
            0,
        );
        cx += 7;
    }
    if cx + 14 < x + width {
        tag(b, cx, y + 1, "SWAP", Ink::Ink, Ink::Blue, s.unicode);
        b.text(
            cx + 6,
            y + 1,
            &m.swap_pct()
                .map(format_pct)
                .unwrap_or_else(|| "N/A".to_string()),
            Ink::White,
            Ink::Default,
            0,
        );
    }
}

fn render_primary_full(b: &mut Buffer, s: &UiState<'_>, l: &PrimaryLayout) {
    draw_header(b, s.unicode, false, Density::Full);
    draw_spine(
        b,
        l.spine,
        &[
            (Ink::Orange, "SYS"),
            (Ink::Lavender, "CPU"),
            (Ink::Salmon, "MEM"),
            (Ink::Blue, "NET"),
            (Ink::Orange, "DSK"),
        ],
        s.unicode,
    );
    draw_status(b, l.clock.x, l.status_y, l.cpu.x - l.clock.x - 2, s);
    let label_y = l.clock_label_y;
    b.text(
        l.clock.x,
        label_y,
        "LOCAL TIME",
        Ink::Amber,
        Ink::Default,
        BOLD,
    );
    tag(
        b,
        l.clock.x + 11,
        label_y,
        "24H",
        Ink::Ink,
        Ink::Blue,
        s.unicode,
    );
    if l.clock.w > 50 {
        b.text_right(
            l.clock.x,
            label_y,
            l.clock.w,
            "SECTOR 001 // STANDARD",
            Ink::Dim,
            Ink::Default,
            0,
        );
    }
    draw_clock(b, s, l.clock, l.clock_scale);
    draw_sweep(b, s, l.sweep);
    draw_date_host(b, s, l);
    draw_load_block(b, s, l);

    draw_memory(b, s, l.memory);
    draw_net_disk(b, s, l.net, s.unicode);
    draw_cpu_capsule(b, s, l.cpu, s.unicode);
    draw_extended_manifest(b, s, l);

    draw_footer(
        b,
        l.footer,
        s.unicode,
        &[
            ("Q", "QUIT", Ink::Orange),
            ("?", "HELP", Ink::Lavender),
            ("SPACE", "HOLD", Ink::Blue),
            ("P", "PROCS", Ink::Salmon),
            ("TAB", "VIEW", Ink::Amber),
        ],
        "LCARS 47 // STARFLEET ENGINEERING",
    );
}

fn render_primary_compact(b: &mut Buffer, s: &UiState<'_>, l: &PrimaryLayout) {
    draw_header(b, s.unicode, false, Density::Compact);
    draw_spine(
        b,
        l.spine,
        &[
            (Ink::Orange, ""),
            (Ink::Lavender, ""),
            (Ink::Salmon, ""),
            (Ink::Blue, ""),
            (Ink::Orange, ""),
        ],
        s.unicode,
    );
    b.text(
        l.clock.x,
        l.status_y,
        "LOCAL TIME",
        Ink::Amber,
        Ink::Default,
        BOLD,
    );
    tag(
        b,
        l.clock.x + 11,
        l.status_y,
        "24H",
        Ink::Ink,
        Ink::Blue,
        s.unicode,
    );
    let (label, color) = status_text(s);
    b.text(l.cpu.x - 8, l.status_y, label, color, Ink::Default, BOLD);
    crate::glyphs::pulse(b, l.cpu.x - 10, l.status_y, s.phase, color);

    draw_clock(b, s, l.clock, l.clock_scale);
    draw_sweep(b, s, l.sweep);
    let date = s
        .clock
        .map(|c| c.date_short())
        .unwrap_or_else(|| "DATE N/A".to_string());
    let host = host_text(s);
    b.text(l.clock.x, l.date_y, &date, Ink::White, Ink::Default, BOLD);
    b.text(
        l.clock.x + date.chars().count() as i32 + 2,
        l.date_y,
        &host,
        Ink::Blue,
        Ink::Default,
        BOLD,
    );
    let up = s
        .metrics
        .uptime_secs
        .map(format_uptime_compact)
        .unwrap_or_else(|| "N/A".to_string());
    b.text(l.clock.x, l.host_y, "UPTIME ", Ink::Dim, Ink::Default, 0);
    b.text(l.clock.x + 7, l.host_y, &up, Ink::Lavender, Ink::Default, 0);

    draw_memory(b, s, l.memory);
    draw_disk_compact(b, s, l.disk, s.unicode);
    draw_net_compact(b, s, l.net, s.unicode);

    let hist = normalized_f32(s.cpu_history, 100.0);
    b.text(l.trace.x, l.trace.y, "CORE", Ink::Dim, Ink::Default, 0);
    sparkline(
        b,
        l.trace.x + 6,
        l.trace.y,
        l.trace.w - 6,
        &hist,
        Ink::Lavender,
    );

    draw_footer(
        b,
        l.footer,
        s.unicode,
        &[
            ("Q", "QUIT", Ink::Orange),
            ("?", "HELP", Ink::Lavender),
            ("P", "PROCS", Ink::Salmon),
            ("TAB", "VIEW", Ink::Amber),
        ],
        "LCARS 47",
    );
    draw_cpu_capsule_compact(b, s, l.cpu, s.unicode);
}

fn draw_cpu_capsule_compact(b: &mut Buffer, s: &UiState<'_>, r: Rect, uni: bool) {
    if r.is_empty() {
        return;
    }
    let m = s.metrics;
    pill(b, r.x, r.y, r.w, r.h, Ink::Blue, uni);
    let ix = r.x + 2;
    b.text_keep_bg(ix, r.y + 1, "CORE LOAD", Ink::Ink, BOLD);
    match m.cpu_pct() {
        Some(cpu) => {
            b.text_keep_bg(ix, r.y + 3, &format!("{:02.0}%", cpu), Ink::Ink, BOLD);
            b.text_keep_bg(ix + 6, r.y + 4, "UTILIZED", Ink::Ink, DIM);
            for i in 0..(r.w - 4) {
                let frac = (i as f32 + 0.5) / (r.w - 4) as f32;
                if frac <= cpu / 100.0 {
                    b.put(
                        ix + i,
                        r.y + 6,
                        if b.ascii { '#' } else { '█' },
                        Ink::White,
                        Ink::Blue,
                        0,
                    );
                } else {
                    b.put(
                        ix + i,
                        r.y + 6,
                        if b.ascii { '-' } else { '░' },
                        Ink::Ink,
                        Ink::Blue,
                        DIM,
                    );
                }
            }
        }
        None => {
            b.text_keep_bg(ix, r.y + 3, "N/A", Ink::Ink, BOLD);
        }
    }
    if let Some(load) = m.load {
        b.text_keep_bg(
            ix,
            r.y + 8,
            &format!(
                "1M {}  5M {}",
                format_load(load.one),
                format_load(load.five)
            ),
            Ink::Ink,
            0,
        );
        b.text_keep_bg(
            ix,
            r.y + 9,
            &format!("15M {}", format_load(load.fifteen)),
            Ink::Ink,
            0,
        );
    } else {
        b.text_keep_bg(ix, r.y + 8, "LOAD N/A", Ink::Ink, 0);
    }
}

fn draw_disk_compact(b: &mut Buffer, s: &UiState<'_>, r: Rect, uni: bool) {
    if r.is_empty() {
        return;
    }
    let m = s.metrics;
    tag(b, r.x, r.y, "ROOT", Ink::Ink, Ink::Salmon, uni);
    let used = m
        .disk_total
        .zip(m.disk_available)
        .map(|(t, a)| format_bytes_compact(t.saturating_sub(a)))
        .unwrap_or_else(|| "N/A".to_string());
    let total = m
        .disk_total
        .map(format_bytes_compact)
        .unwrap_or_else(|| "N/A".to_string());
    b.text(
        r.x + 7,
        r.y,
        &format!("{used} / {total}"),
        Ink::White,
        Ink::Default,
        BOLD,
    );
    b.text_right(
        r.x,
        r.y,
        r.w,
        &m.disk_pct()
            .map(format_pct)
            .unwrap_or_else(|| "N/A".to_string()),
        Ink::Amber,
        Ink::Default,
        BOLD,
    );
    segmented_meter(
        b,
        r.x,
        r.y + 1,
        r.w,
        m.disk_pct().map(|p| p / 100.0),
        Ink::Salmon,
    );
    b.text(r.x, r.y + 2, "ROOT FILESYSTEM", Ink::Dim, Ink::Default, 0);
}

fn draw_net_compact(b: &mut Buffer, s: &UiState<'_>, r: Rect, uni: bool) {
    if r.is_empty() {
        return;
    }
    let m = s.metrics;
    tag(b, r.x, r.y, "NET", Ink::Ink, Ink::Blue, uni);
    b.text_right(
        r.x,
        r.y,
        r.w,
        &format!("{} IF", m.net_ifaces),
        Ink::Dim,
        Ink::Default,
        0,
    );
    let down = arrow(b, false);
    let up = arrow(b, true);
    b.put(r.x, r.y + 1, down, Ink::Blue, Ink::Default, 0);
    b.text(r.x + 2, r.y + 1, "RX", Ink::Dim, Ink::Default, 0);
    b.text(
        r.x + 5,
        r.y + 1,
        &m.net_rx_rate
            .map(format_rate)
            .unwrap_or_else(|| "SYNC".to_string()),
        Ink::White,
        Ink::Default,
        0,
    );
    b.put(r.x, r.y + 2, up, Ink::Lavender, Ink::Default, 0);
    b.text(r.x + 2, r.y + 2, "TX", Ink::Dim, Ink::Default, 0);
    b.text(
        r.x + 5,
        r.y + 2,
        &m.net_tx_rate
            .map(format_rate)
            .unwrap_or_else(|| "SYNC".to_string()),
        Ink::White,
        Ink::Default,
        0,
    );
    let hist = normalized_f64(s.net_rx_history, 32);
    sparkline(b, r.x, r.y + 3, r.w - 1, &hist, Ink::Blue);
}

fn render_primary_tiny(b: &mut Buffer, s: &UiState<'_>, l: &PrimaryLayout) {
    let w = b.width;
    let h = b.height;
    if w < 4 || h < 2 {
        return;
    }
    b.fill_bg(0, 0, w.min(10), 1, Ink::Orange);
    b.text(1, 0, "LCARS", Ink::Ink, Ink::Default, BOLD);
    let clock = clock_text(s);
    b.text(1, 1, &clock, Ink::Amber, Ink::Default, BOLD);
    if h < 4 || w < 24 {
        return;
    }
    let lines = [
        format!(
            "DATE {}",
            s.clock
                .map(|c| c.date_short())
                .unwrap_or_else(|| "N/A".to_string())
        ),
        format!("HOST {}", host_text(s)),
        format!(
            "UP   {}",
            s.metrics
                .uptime_secs
                .map(format_uptime_compact)
                .unwrap_or_else(|| "N/A".to_string())
        ),
        format!(
            "CPU  {}  L {}",
            s.metrics
                .cpu_pct()
                .map(format_pct)
                .unwrap_or_else(|| "N/A".to_string()),
            s.metrics
                .load
                .map(|v| format_load(v.one))
                .unwrap_or_else(|| "N/A".to_string())
        ),
        format!(
            "MEM  {} / {}",
            s.metrics
                .mem_used
                .map(format_bytes_compact)
                .unwrap_or_else(|| "N/A".to_string()),
            s.metrics
                .mem_total
                .map(format_bytes_compact)
                .unwrap_or_else(|| "N/A".to_string())
        ),
        format!(
            "DSK  {}",
            s.metrics
                .disk_pct()
                .map(format_pct)
                .unwrap_or_else(|| "N/A".to_string())
        ),
        format!(
            "NET  {} / {}",
            s.metrics
                .net_rx_rate
                .map(format_rate)
                .unwrap_or_else(|| "SYNC".to_string()),
            s.metrics
                .net_tx_rate
                .map(format_rate)
                .unwrap_or_else(|| "SYNC".to_string())
        ),
    ];
    for (y, line) in (2..).zip(lines) {
        if y >= h - 1 {
            break;
        }
        b.text(
            1,
            y,
            &sanitize_ui_text(&line, (w - 2) as usize),
            Ink::White,
            Ink::Default,
            0,
        );
    }
    if l.footer.h > 0 && h >= 3 {
        b.text(1, h - 1, "Q QUIT  ?  P  TAB", Ink::Dim, Ink::Default, 0);
    }
}

fn render_diagnostics(b: &mut Buffer, s: &UiState<'_>, l: &DiagnosticsLayout) {
    if l.density == Density::Tiny {
        render_diagnostics_tiny(b, s, l);
        return;
    }
    draw_header(b, s.unicode, true, l.density);
    draw_spine(
        b,
        l.spine,
        &[
            (Ink::Orange, "SYS"),
            (Ink::Lavender, "CPU"),
            (Ink::Salmon, "MEM"),
            (Ink::Blue, "SWP"),
            (Ink::Orange, "PS"),
        ],
        s.unicode,
    );
    b.text(
        l.left.x,
        l.title_y,
        "DIAGNOSTIC SUBSYSTEM",
        Ink::Amber,
        Ink::Default,
        BOLD,
    );
    if l.density == Density::Full {
        b.text(
            l.left.x + 22,
            l.title_y,
            "// PROCESS MATRIX",
            Ink::Dim,
            Ink::Default,
            0,
        );
        b.text_right(
            0,
            l.title_y,
            l.w - 3,
            "TAB // RETURN TO COMMAND",
            Ink::Dim,
            Ink::Default,
            0,
        );
    }

    if l.density == Density::Compact {
        render_diagnostics_compact(b, s, l);
    } else {
        render_diagnostics_full(b, s, l);
    }

    draw_footer(
        b,
        l.footer,
        s.unicode,
        &[
            ("TAB", "COMMAND", Ink::Amber),
            ("P", "CLOSE", Ink::Salmon),
            ("?", "HELP", Ink::Lavender),
            ("SPACE", "HOLD", Ink::Blue),
            ("Q", "QUIT", Ink::Orange),
        ],
        "LCARS 47 // DIAGNOSTIC NET",
    );
}

fn draw_os_block(b: &mut Buffer, s: &UiState<'_>, x: i32, y: i32, width: i32, uni: bool) {
    let m = s.metrics;
    tag(b, x, y, "OS", Ink::Ink, Ink::Lavender, uni);
    let os = {
        let name = m.os_name.as_deref().unwrap_or("Unknown");
        let vers = m.os_version.as_deref().unwrap_or("");
        if vers.is_empty() {
            name.to_string()
        } else {
            format!("{name} {vers}")
        }
    };
    b.text(
        x + 5,
        y,
        &sanitize_ui_text(&os, (width - 6) as usize),
        Ink::White,
        Ink::Default,
        BOLD,
    );
    let kernel = m.kernel.as_deref().unwrap_or("N/A");
    b.text(
        x,
        y + 1,
        &format!("KRN {kernel}"),
        Ink::Dim,
        Ink::Default,
        0,
    );
    b.text_right(
        x,
        y + 1,
        width,
        &format!("ARCH {}", m.arch),
        Ink::Blue,
        Ink::Default,
        0,
    );
    tag(b, x, y + 2, "HOST", Ink::Ink, Ink::Blue, uni);
    b.text(
        x + 7,
        y + 2,
        &sanitize_ui_text(
            m.hostname.as_deref().unwrap_or("UNKNOWN"),
            (width - 8) as usize,
        ),
        Ink::White,
        Ink::Default,
        0,
    );
}

fn draw_core_matrix(b: &mut Buffer, s: &UiState<'_>, r: Rect, max_rows: i32) {
    if r.is_empty() {
        return;
    }
    let cores = &s.metrics.per_core;
    b.text(r.x, r.y, "CORE MATRIX // %", Ink::Amber, Ink::Default, BOLD);
    b.text_right(
        r.x,
        r.y,
        r.w,
        &format!("{} CORES", cores.len()),
        Ink::Dim,
        Ink::Default,
        0,
    );
    if cores.is_empty() {
        b.text(
            r.x,
            r.y + 2,
            "PER-CORE TELEMETRY UNAVAILABLE",
            Ink::Dim,
            Ink::Default,
            0,
        );
        return;
    }
    let max_rows = max_rows.max(1);
    let needed_cols = ((cores.len() as i32 + max_rows - 1) / max_rows).max(if cores.len() >= 12 {
        3
    } else if cores.len() > 4 {
        2
    } else {
        1
    });
    let by_width = (r.w / 11).max(1);
    let cols = needed_cols.min(by_width).min(6);
    let rows = (((cores.len() as i32 + cols - 1) / cols).max(1)).min(max_rows);
    let capacity = (cols * rows) as usize;
    let entry_w = (r.w / cols).max(10);
    let bar_w = (entry_w - 8).max(2);
    for (i, pct) in cores.iter().take(capacity).enumerate() {
        let col = (i as i32) / rows;
        let row = (i as i32) % rows;
        let ex = r.x + col * entry_w;
        let ey = r.y + 2 + row;
        if ey >= r.bottom() {
            break;
        }
        b.text(ex, ey, &format!("{i:02}"), Ink::Dim, Ink::Default, 0);
        thin_meter(b, ex + 3, ey, bar_w, Some((*pct) / 100.0), Ink::Blue);
        b.text(
            ex + 4 + bar_w,
            ey,
            &format!("{:>3}", *pct as i32),
            Ink::Amber,
            Ink::Default,
            0,
        );
    }
    if cores.len() > capacity {
        let note = format!("+{} CORES NOT SHOWN", cores.len() - capacity);
        b.text_right(r.x, r.bottom() - 1, r.w, &note, Ink::Dim, Ink::Default, 0);
    }
}

fn draw_swap(b: &mut Buffer, s: &UiState<'_>, x: i32, y: i32, width: i32, uni: bool) {
    let m = s.metrics;
    tag(b, x, y, "SWAP", Ink::Ink, Ink::Salmon, uni);
    match (m.swap_total, m.swap_used) {
        (Some(total), Some(used)) if total > 0 => {
            b.text(
                x + 7,
                y,
                &format!(
                    "{} / {}",
                    format_bytes_compact(used),
                    format_bytes_compact(total)
                ),
                Ink::White,
                Ink::Default,
                BOLD,
            );
            b.text_right(
                x,
                y,
                width,
                &m.swap_pct()
                    .map(format_pct)
                    .unwrap_or_else(|| "N/A".to_string()),
                Ink::Amber,
                Ink::Default,
                BOLD,
            );
            meter(
                b,
                x,
                y + 1,
                width,
                m.swap_pct().map(|p| p / 100.0),
                Ink::Salmon,
            );
        }
        _ => {
            b.text(x + 7, y, "NO SWAP", Ink::Dim, Ink::Default, 0);
        }
    }
}

fn render_diagnostics_full(b: &mut Buffer, s: &UiState<'_>, l: &DiagnosticsLayout) {
    draw_os_block(b, s, l.left.x, 6, l.left.w, s.unicode);
    let core_rect = Rect::new(l.left.x, 10, l.left.w, l.left.h - 10);
    let core_rows = (core_rect.h - 2).max(1);
    draw_core_matrix(b, s, core_rect, core_rows);
    draw_swap(b, s, l.left.x, l.left.bottom() - 3, l.left.w, s.unicode);
    let proc = s
        .metrics
        .process_count
        .map(|n| n.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    b.text(
        l.left.x,
        l.left.bottom() - 1,
        "PROCESSES",
        Ink::Dim,
        Ink::Default,
        0,
    );
    b.text(
        l.left.x + 10,
        l.left.bottom() - 1,
        &proc,
        Ink::Amber,
        Ink::Default,
        BOLD,
    );
    if let Some(load) = s.metrics.load {
        b.text_right(
            l.left.x,
            l.left.bottom() - 1,
            l.left.w,
            &format!(
                "LOAD {} / {} / {}",
                format_load(load.one),
                format_load(load.five),
                format_load(load.fifteen)
            ),
            Ink::Dim,
            Ink::Default,
            0,
        );
    }

    let r = l.right;
    b.text(r.x, 4, "PROCESS MANIFEST", Ink::Amber, Ink::Default, BOLD);
    b.text(
        r.x,
        5,
        &format!("TOP CPU // SAMPLE {:.2}s", s.interval_secs),
        Ink::Dim,
        Ink::Default,
        0,
    );
    b.text(r.x + 6, 7, "PID", Ink::Dim, Ink::Default, 0);
    b.text(r.x + 13, 7, "COMMAND", Ink::Dim, Ink::Default, 0);
    b.text_right(r.x, 7, r.w - 14, "CPU%", Ink::Dim, Ink::Default, 0);
    b.text_right(r.x, 7, r.w, "MEM", Ink::Dim, Ink::Default, 0);
    draw_process_list(b, s, r, 9, 5, 3, s.unicode);

    let count = s.metrics.process_count;
    if l.h >= 35 {
        b.text(r.x, 24, "PROCESSES ONLINE", Ink::Dim, Ink::Default, BOLD);
        match count {
            Some(n) => {
                let text = n.to_string();
                let scale = if big_text_width(&text, 2) <= r.w {
                    2
                } else {
                    1
                };
                draw_big_text(b, r.x, 25, &text, scale, Ink::Lavender, Ink::Default, BOLD);
            }
            None => {
                b.text(r.x, 26, "N/A", Ink::Lavender, Ink::Default, BOLD);
            }
        }
    } else {
        b.text(r.x, 24, "PROCESSES ONLINE", Ink::Dim, Ink::Default, BOLD);
        b.text(
            r.x,
            25,
            &count
                .map(|n| n.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            Ink::Lavender,
            Ink::Default,
            BOLD,
        );
    }
}

fn draw_process_list(
    b: &mut Buffer,
    s: &UiState<'_>,
    r: Rect,
    start_y: i32,
    rows: i32,
    height_per: i32,
    uni: bool,
) {
    let colors = [Ink::Orange, Ink::Salmon, Ink::Lavender, Ink::Blue, Ink::Dim];
    if s.metrics.top_processes.is_empty() {
        b.text(
            r.x,
            start_y,
            "PROCESS DATA UNAVAILABLE",
            Ink::Dim,
            Ink::Default,
            0,
        );
        return;
    }
    for (i, proc) in s
        .metrics
        .top_processes
        .iter()
        .take(rows as usize)
        .enumerate()
    {
        let y = start_y + (i as i32) * height_per;
        let color = colors[i % colors.len()];
        let rank = format!("{:02}", i + 1);
        tag(b, r.x, y, &rank, Ink::Ink, color, uni);
        b.text(
            r.x + 5,
            y,
            &format!("{:>6}", proc.pid),
            Ink::Dim,
            Ink::Default,
            0,
        );
        let name = sanitize_ui_text(&proc.name, 21);
        b.text(r.x + 13, y, &name, Ink::White, Ink::Default, BOLD);
        let cpu = if proc.cpu.is_finite() {
            format!("{:.1}", proc.cpu.max(0.0))
        } else {
            "N/A".to_string()
        };
        b.text_right(
            r.x,
            y,
            r.w - 12,
            &format!("{cpu}%"),
            Ink::Amber,
            Ink::Default,
            BOLD,
        );
        b.text_right(
            r.x,
            y,
            r.w,
            &format_bytes_compact(proc.memory),
            Ink::Lavender,
            Ink::Default,
            0,
        );
        let bar_w = (r.w - 23).max(6);
        thin_meter(
            b,
            r.x + 13,
            y + 1,
            bar_w,
            Some((proc.cpu / 100.0).clamp(0.0, 1.0)),
            color,
        );
    }
}

fn draw_process_list_compact(b: &mut Buffer, s: &UiState<'_>, r: Rect, start_y: i32) {
    let colors = [Ink::Orange, Ink::Salmon, Ink::Lavender, Ink::Blue, Ink::Dim];
    if s.metrics.top_processes.is_empty() {
        b.text(
            r.x,
            start_y,
            "PROCESS DATA UNAVAILABLE",
            Ink::Dim,
            Ink::Default,
            0,
        );
        return;
    }
    for (i, proc) in s.metrics.top_processes.iter().take(5).enumerate() {
        let y = start_y + (i as i32) * 2;
        let color = colors[i % colors.len()];
        tag(
            b,
            r.x,
            y,
            &format!("{:02}", i + 1),
            Ink::Ink,
            color,
            s.unicode,
        );
        b.text(
            r.x + 5,
            y,
            &format!("{:>6}", proc.pid),
            Ink::Dim,
            Ink::Default,
            0,
        );
        let name = sanitize_ui_text(&proc.name, (r.w - 13).max(4) as usize);
        b.text(r.x + 13, y, &name, Ink::White, Ink::Default, BOLD);
        b.text(r.x + 2, y + 1, "CPU", Ink::Dim, Ink::Default, 0);
        let cpu = if proc.cpu.is_finite() {
            format!("{:.1}%", proc.cpu.max(0.0))
        } else {
            "N/A".to_string()
        };
        b.text(
            r.x + 6,
            y + 1,
            &sanitize_ui_text(&cpu, 6),
            Ink::Amber,
            Ink::Default,
            BOLD,
        );
        thin_meter(
            b,
            r.x + 14,
            y + 1,
            7,
            Some((proc.cpu / 100.0).clamp(0.0, 1.0)),
            color,
        );
        b.text_right(
            r.x,
            y + 1,
            r.w,
            &format!("MEM {}", format_bytes_compact(proc.memory)),
            Ink::Lavender,
            Ink::Default,
            0,
        );
    }
}

fn render_diagnostics_compact(b: &mut Buffer, s: &UiState<'_>, l: &DiagnosticsLayout) {
    draw_os_block(b, s, l.left.x, 3, l.left.w, s.unicode);
    let core_rect = Rect::new(l.left.x, 6, l.left.w, 11);
    draw_core_matrix(b, s, core_rect, 8);
    draw_swap(b, s, l.left.x, 17, l.left.w, s.unicode);
    if let Some(load) = s.metrics.load {
        b.text(
            l.left.x,
            19,
            &format!(
                "LOAD {} / {} / {}",
                format_load(load.one),
                format_load(load.five),
                format_load(load.fifteen)
            ),
            Ink::Dim,
            Ink::Default,
            0,
        );
    }
    let proc = s
        .metrics
        .process_count
        .map(|n| n.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    b.text(l.left.x, 20, "PROCESSES", Ink::Dim, Ink::Default, 0);
    b.text(l.left.x + 10, 20, &proc, Ink::Amber, Ink::Default, BOLD);

    let r = l.right;
    b.text(r.x, 3, "PROCESS MANIFEST", Ink::Amber, Ink::Default, BOLD);
    b.text(r.x, 4, "TOP 5 // CPU", Ink::Dim, Ink::Default, 0);
    b.text(r.x + 5, 5, "PID", Ink::Dim, Ink::Default, 0);
    b.text(r.x + 13, 5, "COMMAND", Ink::Dim, Ink::Default, 0);
    draw_process_list_compact(b, s, r, 6);
}

fn render_diagnostics_tiny(b: &mut Buffer, s: &UiState<'_>, l: &DiagnosticsLayout) {
    if b.width < 4 || b.height < 2 {
        return;
    }
    b.fill_bg(0, 0, b.width.min(14), 1, Ink::Lavender);
    b.text(1, 0, "DIAGNOSTICS", Ink::Ink, Ink::Default, BOLD);
    let lines = [
        format!(
            "OS {} {}",
            s.metrics.os_name.as_deref().unwrap_or("N/A"),
            s.metrics.os_version.as_deref().unwrap_or("")
        ),
        format!("ARCH {}", s.metrics.arch),
        format!("CORES {}", s.metrics.per_core.len()),
        format!(
            "SWAP {}",
            s.metrics
                .swap_pct()
                .map(format_pct)
                .unwrap_or_else(|| "N/A".to_string())
        ),
        format!(
            "PROCS {}",
            s.metrics
                .process_count
                .map(|n| n.to_string())
                .unwrap_or_else(|| "N/A".to_string())
        ),
    ];
    for (y, line) in (1..).zip(lines) {
        if y >= b.height - 1 {
            break;
        }
        b.text(
            1,
            y,
            &sanitize_ui_text(&line, (b.width - 2) as usize),
            Ink::White,
            Ink::Default,
            0,
        );
    }
    if l.footer.h > 0 {
        b.text(1, b.height - 1, "TAB  Q", Ink::Dim, Ink::Default, 0);
    }
}

fn render_help(b: &mut Buffer, s: &UiState<'_>, w: i32, h: i32) {
    if w < 30 || h < 10 {
        b.fill_bg(0, 0, w, h, Ink::Track);
        b.text_keep_bg(1, 1, "HELP", Ink::Amber, BOLD);
        let lines = ["Q QUIT", "? CLOSE", "SPACE HOLD", "P PROCS", "TAB VIEW"];
        for (y, line) in (2..).zip(lines) {
            if y >= h {
                break;
            }
            b.text_keep_bg(1, y, line, Ink::White, 0);
        }
        return;
    }
    b.clear();
    let pw = w.min(88).min(w - 4);
    let ph = h.min(24).min(h - 2);
    let px = (w - pw) / 2;
    let py = (h - ph) / 2;
    pill(b, px, py, pw, ph, Ink::Track, s.unicode);
    b.fill_bg(px + 2, py + 2, pw - 4, 1, Ink::Lavender);
    b.text_keep_bg(px + 4, py + 3, "COMMAND REFERENCE", Ink::Amber, BOLD);
    b.text_keep_bg(px + pw - 18, py + 3, "LCARS // 47", Ink::Dim, 0);
    let rows: [(&str, &str, Ink); 6] = [
        ("Q / ESC", "QUIT THE DISPLAY", Ink::Orange),
        ("?", "TOGGLE THIS HELP", Ink::Lavender),
        ("SPACE", "HOLD / RESUME SAMPLING", Ink::Blue),
        ("P", "PROCESS DIAGNOSTICS", Ink::Salmon),
        ("TAB", "CYCLE COMMAND VIEWS", Ink::Amber),
        ("CTRL-C", "RESTORE AND EXIT", Ink::Orange),
    ];
    for (y, (key, desc, color)) in (py + 6..).zip(rows) {
        if y >= py + ph - 4 {
            break;
        }
        let kw = key.chars().count() as i32;
        b.fill_bg(px + 4, y, kw + 2, 1, color);
        b.text_keep_bg(px + 5, y, key, Ink::Ink, BOLD);
        b.text_keep_bg(px + 5 + kw + 3, y, desc, Ink::White, 0);
    }
    if pw >= 84 {
        let lx = px + pw - 32;
        let ly = py + 6;
        b.text_keep_bg(lx, ly, "TELEMETRY", Ink::Amber, BOLD);
        let legend: [(&str, &str, Ink); 4] = [
            ("SYS", "TIME / UPTIME", Ink::Orange),
            ("CPU", "LOAD / CORES", Ink::Lavender),
            ("MEM", "RAM / ROOT", Ink::Salmon),
            ("NET", "RX / TX", Ink::Blue),
        ];
        for (i, (key, desc, color)) in legend.iter().enumerate() {
            let yy = ly + 2 + i as i32 * 2;
            tag(b, lx, yy, key, Ink::Ink, *color, s.unicode);
            b.text_keep_bg(lx + 6, yy, desc, Ink::White, 0);
        }
        b.text_keep_bg(lx, ly + 11, "TRUE COLOR IS AUTOMATIC", Ink::Dim, 0);
    }
    b.text_keep_bg(
        px + 4,
        py + ph - 3,
        "LCARS 47 // STARFLEET ENGINEERING // ORIGINAL COMPOSITION",
        Ink::Dim,
        0,
    );
}

fn normalized_f32(values: &[f32], max: f32) -> Vec<f32> {
    values
        .iter()
        .map(|v| {
            if v.is_finite() {
                (*v / max).clamp(0.0, 1.0)
            } else {
                0.0
            }
        })
        .collect()
}

fn normalized_f64(values: &[f64], cap: usize) -> Vec<f32> {
    if values.is_empty() {
        return vec![];
    }
    let start = values.len().saturating_sub(cap);
    let slice = &values[start..];
    let peak = slice
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v > 0.0)
        .fold(0.0f64, f64::max);
    // Headroom keeps a steady stream from becoming a solid bar; the floor
    // gives low-traffic links a visible baseline.
    let max = (peak * 1.5).max(1024.0);
    slice
        .iter()
        .map(|v| {
            if v.is_finite() {
                (*v / max).clamp(0.0, 1.0) as f32
            } else {
                0.0
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::sample_metrics;
    use crate::clock::Clock;
    use crate::theme::{ColorCapability, Theme};

    fn state<'a>(
        m: &'a Metrics,
        c: &'a Clock,
        view: View,
        theme: &'a Theme,
        hist: &'a [f32],
    ) -> UiState<'a> {
        UiState {
            metrics: m,
            clock: Some(c),
            paused: false,
            view,
            help: false,
            phase: 4,
            cpu_history: hist,
            net_rx_history: &[],
            net_tx_history: &[],
            unicode: true,
            theme,
            interval_secs: 1.0,
        }
    }

    fn clock() -> Clock {
        Clock {
            hour: 21,
            minute: 6,
            second: 42,
            year: 2026,
            month: 10,
            day: 27,
            weekday: 2,
            unix: 0,
        }
    }

    #[test]
    fn primary_has_composition_and_no_process_table() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let m = sample_metrics();
        let c = clock();
        let hist = vec![20.0, 50.0, 70.0, 30.0];
        let s = state(&m, &c, View::Command, &theme, &hist);
        let b = render_frame(&s, 120, 36);
        let text = b.plain_text();
        assert!(text.contains("LOCAL TIME"));
        assert!(text.contains("MEMORY"));
        assert!(text.contains("NET"));
        assert!(text.contains("ROOT"));
        assert!(!text.contains("PROCESS MANIFEST"));
        assert!(!text.contains("PID"));
        assert_eq!(b.width, 120);
        assert_eq!(b.height, 36);
    }

    #[test]
    fn diagnostics_contains_required_data() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let m = sample_metrics();
        let c = clock();
        let hist = vec![20.0, 50.0];
        let s = state(&m, &c, View::Diagnostics, &theme, &hist);
        let b = render_frame(&s, 120, 36);
        let text = b.plain_text();
        assert!(text.contains("DIAGNOSTICS"));
        assert!(text.contains("CORE MATRIX"));
        assert!(text.contains("SWAP"));
        assert!(text.contains("PROCESS MANIFEST"));
        assert!(text.contains("rustc"));
        assert!(text.contains("firefox"));
        assert!(text.contains("x86_64"));
        assert!(text.contains("24111"));
    }

    #[test]
    fn every_required_size_renders_without_panic() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let m = sample_metrics();
        let c = clock();
        let hist = vec![10.0];
        for (w, h) in [
            (120, 36),
            (80, 24),
            (100, 30),
            (110, 34),
            (110, 35),
            (112, 28),
            (112, 35),
            (200, 60),
            (60, 16),
            (40, 12),
            (20, 6),
            (8, 3),
            (1, 1),
        ] {
            for view in [View::Command, View::Diagnostics] {
                let s = state(&m, &c, view, &theme, &hist);
                let b = render_frame(&s, w, h);
                assert_eq!(b.width, w);
                assert_eq!(b.height, h);
            }
        }
    }

    #[test]
    fn ascii_mode_is_ascii() {
        let theme = Theme::with_capability(ColorCapability::Ansi16, true);
        let m = sample_metrics();
        let c = clock();
        let hist = vec![10.0];
        let mut s = state(&m, &c, View::Command, &theme, &hist);
        s.unicode = false;
        let b = render_frame(&s, 120, 36);
        assert!(b.plain_text().is_ascii());
    }

    #[test]
    fn unavailable_metrics_are_tasteful() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let m = Metrics::default();
        let c = clock();
        let hist = vec![];
        let s = state(&m, &c, View::Command, &theme, &hist);
        let b = render_frame(&s, 120, 36);
        let text = b.plain_text();
        assert!(text.contains("N/A") || text.contains("SYNC"));
        assert!(text.contains("--:--:--") || b.plain_text().contains('█'));
    }

    #[test]
    fn long_hostname_does_not_break_layout() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let mut m = sample_metrics();
        m.hostname = Some("a-very-long-hostname-that-will-not-fit-anywhere".to_string());
        let c = clock();
        let hist = vec![];
        let s = state(&m, &c, View::Command, &theme, &hist);
        let b = render_frame(&s, 120, 36);
        let text = b.plain_text();
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.iter().all(|l| l.chars().count() == 120));
    }

    #[test]
    fn large_primary_reveals_extended_manifest() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let m = sample_metrics();
        let c = clock();
        let hist = vec![20.0];
        let s = state(&m, &c, View::Command, &theme, &hist);
        let b = render_frame(&s, 200, 60);
        let text = b.plain_text();
        assert!(text.contains("ARCH"));
        assert!(text.contains("CPUS") || text.contains("PROC"));
        assert!(text.contains("SWAP"));
    }

    #[test]
    fn compact_primary_contains_required_fields() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let m = sample_metrics();
        let c = clock();
        let hist = vec![20.0, 50.0];
        let s = state(&m, &c, View::Command, &theme, &hist);
        let b = render_frame(&s, 80, 24);
        let text = b.plain_text();
        for needle in [
            "27 OCT 2026",
            "UPTIME",
            "USS-LINUX",
            "CORE LOAD",
            "LOAD",
            "MEM",
            "ROOT",
            "RX",
            "TX",
            "NET",
        ] {
            assert!(text.contains(needle), "missing {needle}");
        }
        assert!(!text.contains("PROCESS MANIFEST"));
    }

    #[test]
    fn non_ascii_process_names_and_pid_do_not_break_rows() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let mut m = sample_metrics();
        m.top_processes[0].name = "café-日本語-very-long-process-name".to_string();
        m.top_processes[0].pid = 1234567;
        let c = clock();
        let hist = vec![20.0];
        let s = state(&m, &c, View::Diagnostics, &theme, &hist);
        let b = render_frame(&s, 120, 36);
        let text = b.plain_text();
        assert!(text.lines().all(|l| l.chars().count() == 120));
        assert!(text.contains("café"));
        assert!(text.contains("1234567"));
    }

    #[test]
    fn paused_status_is_explicit() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let m = sample_metrics();
        let c = clock();
        let hist = vec![20.0];
        let mut s = state(&m, &c, View::Command, &theme, &hist);
        s.paused = true;
        let b = render_frame(&s, 120, 36);
        assert!(b.row_text(3).contains("HOLD"));
    }

    #[test]
    fn help_overlay_covers_and_renders() {
        let theme = Theme::with_capability(ColorCapability::Truecolor, false);
        let m = sample_metrics();
        let c = clock();
        let hist = vec![];
        let mut s = state(&m, &c, View::Command, &theme, &hist);
        s.help = true;
        let b = render_frame(&s, 120, 36);
        let text = b.plain_text();
        assert!(text.contains("COMMAND REFERENCE"));
        assert!(text.contains("HOLD / RESUME"));
        assert!(!text.contains("LOCAL TIME"));
    }
}
