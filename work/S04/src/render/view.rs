use super::buffer::{
    dash, fit, fit_ascii, fmt_bytes, fmt_load, fmt_pct, fmt_rate, opt_bytes, uptime_label, Buffer,
};
use super::color::*;
use super::draw::{clock, elbow_ul, elbow_ur, label, load_digits, nub, pill, scanner, sparkline};
use super::ViewState;
use crate::metrics::{ProcessRow, Sample};

pub fn paint(buf: &mut Buffer, sample: &Sample, ui: &ViewState) {
    match ui.view {
        super::View::Primary => paint_primary(buf, sample, ui),
        super::View::Diag => paint_diag(buf, sample, ui),
    }
    if ui.help {
        paint_help(buf, ui);
    }
}

fn paint_primary(buf: &mut Buffer, sample: &Sample, ui: &ViewState) {
    let w = buf.width as i32;
    let h = buf.height as i32;
    if w >= 108 && h >= 34 {
        primary_full(buf, sample, ui);
    } else if w >= 72 && h >= 22 {
        primary_standard(buf, sample, ui);
    } else {
        primary_minimal(buf, sample, ui);
    }
}

fn primary_full(buf: &mut Buffer, sample: &Sample, ui: &ViewState) {
    let w = buf.width as i32;
    let h = buf.height as i32;
    let uni = ui.unicode;
    let rail_w = 70.min(w - 36).max(48);
    let rail_h = 3;
    let spine_w = 6;
    let spine_h = 15;
    elbow_ul(
        buf, 0, 0, rail_w, rail_h, spine_w, spine_h, ORANGE, SALMON, uni,
    );
    if !ui.paused {
        scanner(buf, 0, 1, rail_w - 10, ui.frame);
    }
    label(buf, 8, 1, 8, "LCARS", ORANGE, uni);
    label(buf, 18, 1, 6, "04", ORANGE, uni);
    label(buf, 26, 1, 8, "ENG", ORANGE, uni);

    pill(buf, 0, 18, spine_w, 5, LAVENDER, uni);
    pill(buf, 0, 25, spine_w, 3, SALMON, uni);
    nub(buf, spine_w, 6, "01", PEACH, uni);
    nub(buf, spine_w, 11, "02", AMBER, uni);
    nub(buf, spine_w, 18, "03", BLUE, uni);
    nub(buf, spine_w, 25, "04", PEACH, uni);
    lamp(buf, 1, 7, ui);

    let time_x = 12;
    let time_y = 5;
    let colon = if ui.paused {
        DIM
    } else if ui.frame.is_multiple_of(2) {
        SALMON
    } else {
        AMBER
    };
    clock(
        buf,
        time_x,
        time_y,
        &sample.time.hms(),
        AMBER,
        colon,
        true,
        uni,
    );

    let load = sample
        .load
        .map(|l| fmt_load(l[0]))
        .unwrap_or_else(|| if uni { "—.——" } else { "-.--" }.into());
    buf.text(12, 15, "LOAD", CREAM, BLACK);
    load_digits(buf, 12, 16, &load, SALMON, uni);
    if let Some(l) = sample.load {
        let extra = format!("5m {}  15m {}", fmt_load(l[1]), fmt_load(l[2]));
        buf.text(36, 21, &extra, DIM, BLACK);
    } else {
        buf.text(36, 21, "5m --  15m --", DIM, BLACK);
    }

    let (status, sc) = status_of(sample, ui.paused);
    pill(buf, 40, 16, 26, 3, sc, uni);
    label(buf, 40, 17, 26, status, sc, uni);

    let right = w - 1;
    metric_bar(
        buf,
        right,
        4,
        36,
        3,
        ORANGE,
        &cpu_line(sample, uni),
        ui.focus == 0,
        uni,
    );
    metric_bar(
        buf,
        right,
        8,
        42,
        3,
        SALMON,
        &mem_line(sample, uni),
        ui.focus == 1,
        uni,
    );
    metric_bar(
        buf,
        right,
        12,
        30,
        2,
        BLUE,
        &rx_line(sample, uni),
        ui.focus == 2,
        uni,
    );
    metric_bar(
        buf,
        right,
        15,
        26,
        2,
        PEACH,
        &tx_line(sample, uni),
        ui.focus == 3,
        uni,
    );
    metric_bar(
        buf,
        right,
        18,
        22,
        2,
        LAVENDER,
        &dsk_line(sample),
        ui.focus == 4,
        uni,
    );

    sparkline(buf, 12, 23, 48.min(w - 50), &ui.cpu_trace, BLUE, uni);
    buf.text(12, 24, "TRACE", DIM, BLACK);

    let by = h - 3;
    let bx = 18;
    pill(buf, bx, by - 3, 4, 3, BLUE, uni);
    pill(buf, bx, by, w - bx, 3, AMBER, uni);
    pill(buf, bx, by, 16, 3, BLUE, uni);
    pill(buf, w - 14, by, 14, 3, SALMON, uni);
    label(buf, bx, by + 1, 16, &sample.time.date_label(), BLUE, uni);
    let host = if uni {
        fit(&sample.hostname, 18)
    } else {
        fit_ascii(&sample.hostname, 18)
    };
    label(buf, bx + 17, by + 1, 20, &host, AMBER, uni);
    let up = sample
        .uptime
        .map(uptime_label)
        .unwrap_or_else(|| dash(uni).into());
    label(buf, bx + 38, by + 1, 16, &up, AMBER, uni);
    label(buf, w - 42, by + 1, 28, "Q ? P TAB SPC", SALMON, uni);
    if ui.paused {
        pill(buf, 12, 14, 8, 1, SALMON, uni);
        label(buf, 12, 14, 8, "HOLD", SALMON, uni);
    }
}

fn primary_standard(buf: &mut Buffer, sample: &Sample, ui: &ViewState) {
    let w = buf.width as i32;
    let h = buf.height as i32;
    let uni = ui.unicode;
    let rail_w = 42.min(w - 20).max(24);
    elbow_ul(buf, 0, 0, rail_w, 2, 4, 12, ORANGE, SALMON, uni);
    label(buf, 6, 0, 8, "LCARS", ORANGE, uni);
    nub(buf, 4, 5, "01", PEACH, uni);
    nub(buf, 4, 9, "02", AMBER, uni);
    let colon = if ui.frame.is_multiple_of(2) {
        SALMON
    } else {
        AMBER
    };
    clock(buf, 8, 3, &sample.time.hms(), AMBER, colon, false, uni);
    let load = sample
        .load
        .map(|l| fmt_load(l[0]))
        .unwrap_or_else(|| "--".into());
    buf.text(8, 9, &format!("LD {load}"), CREAM, BLACK);
    let (status, sc) = status_of(sample, ui.paused);
    pill(buf, 22, 9, 16.min(w - 24), 2, sc, uni);
    label(buf, 22, 9, 16.min(w - 24), status, sc, uni);
    let right = w - 1;
    metric_bar(
        buf,
        right,
        3,
        22.min(w / 3),
        2,
        ORANGE,
        &cpu_line(sample, uni),
        ui.focus == 0,
        uni,
    );
    metric_bar(
        buf,
        right,
        6,
        18.min(w / 3),
        2,
        SALMON,
        &mem_short(sample),
        ui.focus == 1,
        uni,
    );
    let by = h - 2;
    pill(buf, 10, by, w - 10, 2, AMBER, uni);
    label(buf, 10, by, 14, &sample.time.date_label(), AMBER, uni);
    let host = if uni {
        fit(&sample.hostname, 12)
    } else {
        fit_ascii(&sample.hostname, 12)
    };
    label(buf, 24, by, 14, &host, AMBER, uni);
    label(buf, w - 16, by, 16, "Q ? P TAB", AMBER, uni);
}

fn primary_minimal(buf: &mut Buffer, sample: &Sample, ui: &ViewState) {
    let w = buf.width as i32;
    let h = buf.height as i32;
    let uni = ui.unicode;
    if h >= 1 && w >= 1 {
        buf.fill(0, 0, w, 1, ORANGE);
        label(buf, 0, 0, w, "LCARS", ORANGE, uni);
    }
    if h >= 3 {
        let line = format!("{} {}", sample.time.hms(), cpu_line(sample, uni));
        let shown = if uni {
            fit(&line, w as usize)
        } else {
            fit_ascii(&line, w as usize)
        };
        buf.text(0, 2, &shown, AMBER, BLACK);
    }
    if h >= 5 {
        let (status, _) = status_of(sample, ui.paused);
        buf.text(0, 4, status, CREAM, BLACK);
    }
    if h >= 7 {
        buf.text(0, h - 1, "Q quit", DIM, BLACK);
    }
}

fn paint_diag(buf: &mut Buffer, sample: &Sample, ui: &ViewState) {
    let w = buf.width as i32;
    let h = buf.height as i32;
    let uni = ui.unicode;
    if w < 40 || h < 14 {
        primary_minimal(buf, sample, ui);
        if h >= 6 {
            buf.text(0, 5, "DIAG", SALMON, BLACK);
        }
        return;
    }
    let spine_w = 6.min(w / 8).max(3);
    let rail_w = (w * 45 / 100).clamp(28, w - 4);
    let rail_h = if h >= 30 { 3 } else { 2 };
    elbow_ur(
        buf,
        w,
        0,
        rail_w,
        rail_h,
        spine_w,
        16.min(h - 6),
        SALMON,
        ORANGE,
        uni,
    );
    let head = format!(
        "DIAG {} {}  N {}  SW {}/{}",
        fit(&sample.os_name, 16),
        fit(&sample.arch, 8),
        sample
            .process_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| dash(uni).into()),
        opt_bytes(sample.swap_used),
        opt_bytes(sample.swap_total),
    );
    label(
        buf,
        w - rail_w + 2,
        if rail_h >= 3 { 1 } else { 0 },
        rail_w - spine_w - 4,
        &head,
        SALMON,
        uni,
    );

    paint_cores(buf, 2, rail_h + 1, w - spine_w - 4, sample, uni);

    let bar_y = if h >= 32 { 10 } else { 7 };
    let bar_h = if h >= 32 { 3 } else { 2 };
    let gap = if h >= 32 { 1 } else { 0 };
    let colors = [ORANGE, SALMON, AMBER, BLUE, LAVENDER];
    let widths = [72, 58, 80, 48, 66];
    for i in 0..5 {
        let y = bar_y + i * (bar_h + gap);
        if y + bar_h >= h - 2 {
            break;
        }
        let bw = widths[i as usize].min(w - spine_w - 6).max(16);
        let row = sample.top_cpu.get(i as usize);
        let text = process_line(row, i as usize, uni);
        let color = colors[i as usize];
        pill(buf, 2, y, bw, bar_h, color, uni);
        if ui.focus == i as usize {
            buf.fill(2, y, 3.min(bw), bar_h, SCAN);
        }
        let ty = y + if bar_h >= 3 { 1 } else { 0 };
        label(buf, 2, ty, bw, &text, color, uni);
    }

    let by = h - 2;
    let bw = (w / 2).max(20).min(w);
    pill(buf, 0, by, bw, 2, BLUE, uni);
    label(buf, 0, by, bw, "P CLOSE   TAB FOCUS   Q QUIT", BLUE, uni);
}

fn paint_cores(buf: &mut Buffer, x: i32, y: i32, w: i32, sample: &Sample, uni: bool) {
    if sample.per_core.is_empty() || w < 8 || y + 2 >= buf.height as i32 {
        buf.text(x, y, "CORES --", DIM, BLACK);
        return;
    }
    let n = sample.per_core.len();
    let slot = if n <= w as usize / 4 { 4 } else { 3 };
    let fit_n = ((w as usize) / slot).max(1).min(n);
    for i in 0..fit_n {
        let px = x + (i as i32) * slot as i32;
        let pct = sample.per_core[i].unwrap_or(0.0).clamp(0.0, 100.0);
        let color = if pct >= 85.0 {
            SALMON
        } else if pct >= 50.0 {
            ORANGE
        } else if pct >= 20.0 {
            AMBER
        } else {
            BLUE_DEEP
        };
        pill(buf, px, y, (slot as i32) - 1, 2, color, uni);
        if slot >= 4 {
            let t = sample.per_core[i]
                .map(fmt_pct)
                .unwrap_or_else(|| dash(uni).into());
            label(buf, px, y, (slot as i32) - 1, &t, color, uni);
        }
    }
    if fit_n < n {
        buf.text(
            x + (fit_n as i32) * slot as i32,
            y,
            &format!("+{}", n - fit_n),
            DIM,
            BLACK,
        );
    }
}

fn paint_help(buf: &mut Buffer, ui: &ViewState) {
    let w = buf.width as i32;
    let h = buf.height as i32;
    let uni = ui.unicode;
    let oh = 8.min(h).max(3);
    let y = (h - oh).max(0);
    let x = 4.min(w.saturating_sub(8)).max(0);
    let bw = (w - x - 2).max(8);
    pill(buf, x, y, bw, oh, LAVENDER, uni);
    pill(buf, x, y, 8.min(bw), oh, ORANGE, uni);
    label(buf, x, y + 1, 8.min(bw), "CMD", ORANGE, uni);
    let lines = [
        "Q / ESC  QUIT",
        "?        HELP",
        "SPACE    HOLD / RESUME",
        "P        PROCESS DIAG",
        "TAB      CYCLE FOCUS",
        "LCARS_ASCII=1  LCARS_MONO=1",
    ];
    for (i, line) in lines.iter().enumerate() {
        let yy = y + 1 + i as i32;
        if yy >= y + oh || yy >= h {
            break;
        }
        if i == 0 {
            continue;
        }
        label(buf, x + 9, yy - 1, bw - 10, line, LAVENDER, uni);
    }
    label(buf, x + 9, y, bw - 10, lines[0], LAVENDER, uni);
}

#[allow(clippy::too_many_arguments)]
fn metric_bar(
    buf: &mut Buffer,
    right: i32,
    y: i32,
    w: i32,
    h: i32,
    color: Rgb,
    text: &str,
    focused: bool,
    uni: bool,
) {
    let w = w.min(right).max(8);
    let x = right - w;
    if y + h > buf.height as i32 || x < 0 {
        return;
    }
    pill(buf, x, y, w, h, color, uni);
    if focused {
        buf.fill(x, y, 3.min(w), h, SCAN);
    }
    let ty = y + if h >= 3 { 1 } else { 0 };
    label(buf, x, ty, w, text, if focused { SCAN } else { color }, uni);
}

fn lamp(buf: &mut Buffer, x: i32, y: i32, ui: &ViewState) {
    let c = if ui.paused {
        SALMON
    } else {
        match ui.frame % 3 {
            0 => ORANGE,
            1 => AMBER,
            _ => PEACH,
        }
    };
    buf.fill(x, y, 2, 1, c);
}

fn status_of(sample: &Sample, paused: bool) -> (&'static str, Rgb) {
    if paused {
        return ("HOLD", LAVENDER);
    }
    let load = sample.load.map(|l| l[0]);
    let cores = sample.per_core.len().max(1) as f64;
    let ratio = load.unwrap_or(0.0) / cores;
    if load.is_none() && sample.cpu_pct.is_none() {
        return ("UNAVAIL", DIM);
    }
    if ratio >= 1.5 || sample.cpu_pct.unwrap_or(0.0) >= 90.0 {
        ("ALERT", SALMON)
    } else if ratio >= 0.7 || sample.cpu_pct.unwrap_or(0.0) >= 70.0 {
        ("CAUTION", AMBER)
    } else {
        ("NOMINAL", BLUE)
    }
}

fn cpu_line(sample: &Sample, uni: bool) -> String {
    match sample.cpu_pct {
        Some(v) => format!("CPU {}", fmt_pct(v)),
        None => format!("CPU {}", dash(uni)),
    }
}

fn mem_line(sample: &Sample, uni: bool) -> String {
    format!(
        "MEM {} AVL {} /{}",
        opt_or(sample.mem_used, uni),
        opt_or(sample.mem_available, uni),
        opt_or(sample.mem_total, uni),
    )
}

fn mem_short(sample: &Sample) -> String {
    format!(
        "MEM {}/{}",
        opt_bytes(sample.mem_used),
        opt_bytes(sample.mem_total)
    )
}

fn rx_line(sample: &Sample, uni: bool) -> String {
    match sample.net_rx_bps {
        Some(v) => format!("RX {}", fmt_rate(v)),
        None => format!("RX {}", dash(uni)),
    }
}

fn tx_line(sample: &Sample, uni: bool) -> String {
    match sample.net_tx_bps {
        Some(v) => format!("TX {}", fmt_rate(v)),
        None => format!("TX {}", dash(uni)),
    }
}

fn dsk_line(sample: &Sample) -> String {
    match (sample.disk_used, sample.disk_total) {
        (Some(u), Some(t)) if t > 0 => {
            let pct = (u as f64) * 100.0 / (t as f64);
            format!("DSK {} {}", fmt_pct(pct), fmt_bytes(t))
        }
        _ => "DSK --".into(),
    }
}

fn opt_or(v: Option<u64>, uni: bool) -> String {
    v.map(fmt_bytes).unwrap_or_else(|| dash(uni).into())
}

fn process_line(row: Option<&ProcessRow>, idx: usize, uni: bool) -> String {
    let Some(row) = row else {
        return format!("{:02}  {}", idx + 1, dash(uni));
    };
    let cpu = row.cpu_pct.map(fmt_pct).unwrap_or_else(|| dash(uni).into());
    let mem = row
        .mem_bytes
        .map(fmt_bytes)
        .unwrap_or_else(|| dash(uni).into());
    format!("{}  {}  {}  {}", row.pid, row.name, cpu, mem)
}
