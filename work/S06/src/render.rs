use crate::buffer::{Buffer, Tone};
use crate::model::{History, Snapshot, format_bytes, format_rate, format_uptime, percent};
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Primary,
    Diagnostics,
}

impl View {
    pub fn toggled(self) -> Self {
        match self {
            Self::Primary => Self::Diagnostics,
            Self::Diagnostics => Self::Primary,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderOptions {
    pub ascii: bool,
    pub paused: bool,
    pub help: bool,
    pub tick: u64,
}

pub fn render(
    snapshot: &Snapshot,
    history: &History,
    view: View,
    width: u16,
    height: u16,
    options: RenderOptions,
) -> Buffer {
    let mut buffer = Buffer::new(width, height);
    if width == 0 || height == 0 {
        return buffer;
    }
    if width < 48 || height < 12 {
        render_minimal(&mut buffer, snapshot, view, options);
    } else if width < 100 || height < 30 {
        match view {
            View::Primary => render_compact_primary(&mut buffer, snapshot, options),
            View::Diagnostics => render_compact_diagnostics(&mut buffer, snapshot, options),
        }
    } else {
        match view {
            View::Primary => render_primary(&mut buffer, snapshot, history, options),
            View::Diagnostics => render_diagnostics(&mut buffer, snapshot, options),
        }
    }
    if options.help {
        render_help(&mut buffer, options);
    }
    if options.ascii {
        buffer.make_ascii_safe();
    }
    buffer
}

fn render_primary(
    buffer: &mut Buffer,
    snapshot: &Snapshot,
    history: &History,
    options: RenderOptions,
) {
    frame(
        buffer,
        "LCARS 47 // ENGINEERING STATUS",
        Tone::Orange,
        options,
    );
    let width = buffer.width();
    let height = buffer.height();

    large_time(buffer, 8, 4, &snapshot.time, Tone::Amber, 2, options.ascii);
    buffer.text(11, 12, &snapshot.date, Tone::Lavender, Tone::Background);
    buffer.text(
        36,
        12,
        &format!("HOST {}", clip(&snapshot.hostname.to_uppercase(), 28)),
        Tone::Text,
        Tone::Background,
    );
    status_pill(buffer, 10, 14, 34, snapshot.cpu.is_some(), options);

    buffer.text(98, 4, "CPU / TOTAL", Tone::Salmon, Tone::Background);
    let cpu = snapshot
        .cpu
        .map_or_else(|| "--".into(), |value| format!("{value:02.0}"));
    large_text(
        buffer,
        98,
        6,
        &format!("{cpu}%"),
        Tone::Salmon,
        1,
        options.ascii,
    );
    buffer.text(96, 14, "LOAD 01/05/15", Tone::Blue, Tone::Background);
    if let Some(load) = snapshot.load {
        buffer.text(
            96,
            16,
            &format!("{:.2}  {:.2}  {:.2}", load[0], load[1], load[2]),
            Tone::Text,
            Tone::Background,
        );
    } else {
        unavailable(buffer, 96, 16, options);
    }
    buffer.text(
        96,
        18,
        &format!("UP {}", format_uptime(snapshot.uptime)),
        Tone::Dim,
        Tone::Background,
    );

    sweep(buffer, 10, 17, 67, Tone::Lavender, options.ascii);
    memory_readout(buffer, 10, 19, snapshot);
    disk_readout(buffer, 10, 22, snapshot);
    network_readout(buffer, 10, 25, snapshot, options);

    if width >= 110 {
        buffer.text(74, 19, "CPU WAVEFORM", Tone::Blue, Tone::Background);
        buffer.text_right(
            108,
            19,
            if options.ascii { "0-100%" } else { "0—100%" },
            Tone::Dim,
            Tone::Background,
        );
        braille_trace(buffer, 74, 20, 34, 8, history, options.ascii);
        buffer.text(74, 29, "T-180 SAMPLES", Tone::Dim, Tone::Background);
        buffer.text(
            100,
            29,
            if options.ascii { "NOW ->" } else { "NOW →" },
            Tone::Dim,
            Tone::Background,
        );
    }

    buffer.text(9, height - 3, "SYS", Tone::Ink, Tone::Salmon);
    buffer.text(17, height - 3, "NOMINAL", Tone::Ink, Tone::Salmon);
    footer(buffer, view_label(View::Primary), options);
}

fn render_compact_primary(buffer: &mut Buffer, snapshot: &Snapshot, options: RenderOptions) {
    compact_frame(buffer, "LCARS // ENGINEERING", Tone::Orange, options);
    let height = buffer.height();
    large_time(buffer, 6, 3, &snapshot.time, Tone::Amber, 1, options.ascii);
    buffer.text(6, 9, &snapshot.date, Tone::Lavender, Tone::Background);
    buffer.text(
        29,
        9,
        &clip(&snapshot.hostname.to_uppercase(), 20),
        Tone::Text,
        Tone::Background,
    );
    status_pill(buffer, 6, 11, 27, snapshot.cpu.is_some(), options);

    buffer.text(58, 3, "CPU", Tone::Salmon, Tone::Background);
    let cpu = snapshot
        .cpu
        .map_or_else(|| "--%".into(), |value| format!("{value:.0}%"));
    large_text(buffer, 58, 4, &cpu, Tone::Salmon, 1, options.ascii);
    buffer.text(57, 10, "LOAD 1/5/15", Tone::Blue, Tone::Background);
    if let Some(load) = snapshot.load {
        buffer.text(
            55,
            11,
            &format!("{:.2} {:.2} {:.2}", load[0], load[1], load[2]),
            Tone::Text,
            Tone::Background,
        );
    } else {
        unavailable(buffer, 55, 11, options);
    }

    memory_readout(buffer, 6, 13, snapshot);
    disk_readout(buffer, 6, 15, snapshot);
    network_readout(buffer, 6, 17, snapshot, options);
    buffer.text(
        42,
        19,
        &format!("UP {}", format_uptime(snapshot.uptime)),
        Tone::Dim,
        Tone::Background,
    );
    buffer.text(7, height - 2, "SYS OK", Tone::Ink, Tone::Salmon);
    footer(buffer, "COMMAND", options);
}

fn render_diagnostics(buffer: &mut Buffer, snapshot: &Snapshot, options: RenderOptions) {
    frame(
        buffer,
        "LCARS 47 // DIAGNOSTIC ARRAY",
        Tone::Lavender,
        options,
    );
    buffer.text(
        10,
        4,
        "SYSTEM DIAGNOSTICS",
        Tone::Lavender,
        Tone::Background,
    );
    buffer.text(
        10,
        6,
        &clip(&format!("{} / {}", snapshot.os, snapshot.architecture), 66),
        Tone::Text,
        Tone::Background,
    );
    buffer.text(
        10,
        8,
        &format!(
            "{}  /  UPTIME {}",
            snapshot.process_count.map_or_else(
                || "PROCESSES N/A".into(),
                |count| format!("{count} PROCESSES")
            ),
            format_uptime(snapshot.uptime)
        ),
        Tone::Dim,
        Tone::Background,
    );

    buffer.text(10, 10, "PER-CORE UTILIZATION", Tone::Blue, Tone::Background);
    core_grid(buffer, 10, 12, 47, 16, &snapshot.cores, options.ascii);
    buffer.text(63, 10, "TOP CPU PROCESSES", Tone::Salmon, Tone::Background);
    process_list(buffer, 63, 12, 51, snapshot, options);

    if let Some(swap) = snapshot.swap {
        buffer.text(
            63,
            21,
            &format!(
                "SWAP {} / {}  {:.0}%",
                format_bytes(swap.used),
                format_bytes(swap.total),
                percent(swap.used, swap.total).unwrap_or(0.0)
            ),
            Tone::Lavender,
            Tone::Background,
        );
    } else {
        buffer.text(63, 21, "SWAP NOT CONFIGURED", Tone::Dim, Tone::Background);
    }
    buffer.text(
        63,
        23,
        "CPU% MAY EXCEED 100 ON",
        Tone::Dim,
        Tone::Background,
    );
    buffer.text(
        63,
        24,
        "MULTI-THREADED PROCESSES",
        Tone::Dim,
        Tone::Background,
    );
    buffer.text(9, buffer.height() - 3, "DIAG", Tone::Ink, Tone::Salmon);
    buffer.text(18, buffer.height() - 3, "TOP 5", Tone::Ink, Tone::Salmon);
    footer(buffer, view_label(View::Diagnostics), options);
}

fn render_compact_diagnostics(buffer: &mut Buffer, snapshot: &Snapshot, options: RenderOptions) {
    compact_frame(buffer, "LCARS // DIAGNOSTICS", Tone::Lavender, options);
    buffer.text(
        6,
        3,
        "SYSTEM / CORE / PROCESS",
        Tone::Lavender,
        Tone::Background,
    );
    buffer.text(
        6,
        5,
        &clip(
            &format!("{} / {}", snapshot.os, snapshot.architecture),
            buffer.width().saturating_sub(12).into(),
        ),
        Tone::Text,
        Tone::Background,
    );
    let swap = snapshot.swap.map_or_else(
        || "SWAP N/A".into(),
        |swap| {
            format!(
                "SWAP {}/{}",
                format_bytes(swap.used),
                format_bytes(swap.total)
            )
        },
    );
    buffer.text(
        6,
        6,
        &format!(
            "{} / {} / {} PROC",
            swap,
            snapshot.architecture,
            snapshot
                .process_count
                .map_or_else(|| "N/A".into(), |count| count.to_string())
        ),
        Tone::Dim,
        Tone::Background,
    );
    buffer.text(6, 8, "CORES", Tone::Blue, Tone::Background);
    core_grid(
        buffer,
        6,
        9,
        buffer.width() - 12,
        4,
        &snapshot.cores,
        options.ascii,
    );
    buffer.text(
        6,
        14,
        "PID     NAME             CPU      MEM",
        Tone::Salmon,
        Tone::Background,
    );
    process_list(buffer, 6, 15, buffer.width() - 12, snapshot, options);
    footer(buffer, "DIAGNOSTIC", options);
}

fn render_minimal(buffer: &mut Buffer, snapshot: &Snapshot, view: View, options: RenderOptions) {
    let width = buffer.width();
    let height = buffer.height();
    let rail = if view == View::Primary {
        Tone::Orange
    } else {
        Tone::Lavender
    };
    buffer.fill(0, 0, width.min(3), height, rail);
    if height > 0 {
        buffer.fill(0, 0, width, 1, rail);
    }
    if width > 5 && height > 2 {
        buffer.text(5, 2, "LCARS", Tone::Text, Tone::Background);
    }
    if height > 4 {
        buffer.text(
            5,
            4,
            if view == View::Primary {
                &snapshot.time
            } else {
                "DIAGNOSTICS"
            },
            Tone::Amber,
            Tone::Background,
        );
    }
    if height > 6 {
        let value = if view == View::Primary {
            snapshot
                .cpu
                .map_or_else(|| "CPU N/A".into(), |cpu| format!("CPU {cpu:.0}%"))
        } else {
            snapshot.process_count.map_or_else(
                || "PROCESSES N/A".into(),
                |count| format!("{count} PROCESSES"),
            )
        };
        buffer.text(5, 6, &value, Tone::Text, Tone::Background);
    }
    if height > 1 && width > 12 {
        buffer.text_right(width - 1, height - 1, "Q QUIT", Tone::Dim, Tone::Background);
    }
    if options.paused && height > 8 {
        buffer.text(5, 8, "PAUSED", Tone::Salmon, Tone::Background);
    }
}

fn frame(buffer: &mut Buffer, title: &str, accent: Tone, options: RenderOptions) {
    let width = buffer.width();
    let height = buffer.height();
    buffer.fill(0, 0, 6, height.saturating_sub(3), accent);
    buffer.fill(0, 0, 55.min(width), 3, accent);
    buffer.fill(0, height.saturating_sub(4), 55.min(width), 3, Tone::Salmon);
    buffer.fill(5, 4, 1, 4, Tone::Lavender);
    buffer.fill(5, 10, 1, 5, Tone::Blue);
    buffer.fill(width.saturating_sub(4), 4, 4, 13, Tone::Lavender);
    buffer.fill(width.saturating_sub(4), 18, 4, 3, Tone::Blue);
    buffer.text(8, 1, title, Tone::Ink, accent);
    buffer.text(1, 5, "47", Tone::Ink, Tone::Lavender);
    buffer.text(1, 11, "ENG", Tone::Ink, Tone::Blue);
    buffer.text(width.saturating_sub(3), 6, "A", Tone::Ink, Tone::Lavender);
    buffer.text(width.saturating_sub(3), 8, "U", Tone::Ink, Tone::Lavender);
    buffer.text(width.saturating_sub(3), 10, "X", Tone::Ink, Tone::Lavender);
    if !options.ascii {
        buffer.set(55, 0, '▞', accent, Tone::Background);
        buffer.set(55, 1, '▐', accent, Tone::Background);
        buffer.set(55, 2, '▟', accent, Tone::Background);
        buffer.set(6, 3, '╰', accent, Tone::Background);
        buffer.set(55, height - 4, '▛', Tone::Salmon, Tone::Background);
        buffer.set(55, height - 3, '▌', Tone::Salmon, Tone::Background);
        buffer.set(55, height - 2, '▙', Tone::Salmon, Tone::Background);
    }
}

fn compact_frame(buffer: &mut Buffer, title: &str, accent: Tone, options: RenderOptions) {
    let width = buffer.width();
    let height = buffer.height();
    buffer.fill(0, 0, 4, height.saturating_sub(2), accent);
    buffer.fill(0, 0, 43.min(width), 2, accent);
    buffer.fill(0, height.saturating_sub(3), 37.min(width), 2, Tone::Salmon);
    buffer.fill(width.saturating_sub(4), 3, 4, 7, Tone::Lavender);
    buffer.text(6, 0, title, Tone::Ink, accent);
    buffer.text(0, 4, "47", Tone::Ink, accent);
    if !options.ascii {
        buffer.set(43, 0, '▞', accent, Tone::Background);
        buffer.set(43, 1, '▟', accent, Tone::Background);
        buffer.set(4, 2, '╰', accent, Tone::Background);
    }
}

fn footer(buffer: &mut Buffer, view: &str, options: RenderOptions) {
    let y = buffer.height().saturating_sub(1);
    let width = buffer.width();
    let state = if options.paused { "PAUSED" } else { "LIVE" };
    buffer.text(7, y, view, Tone::Dim, Tone::Background);
    let controls = if width >= 100 {
        "SPACE PAUSE   P DIAG   TAB VIEW   ? HELP   Q QUIT"
    } else {
        "SPC PAUSE  P DIAG  ? HELP  Q QUIT"
    };
    buffer.text_right(
        width.saturating_sub(1),
        y,
        controls,
        Tone::Dim,
        Tone::Background,
    );
    if width >= 100 {
        buffer.text(
            59,
            y,
            state,
            if options.paused {
                Tone::Salmon
            } else {
                Tone::Mint
            },
            Tone::Background,
        );
    }
}

fn status_pill(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    nominal: bool,
    options: RenderOptions,
) {
    let tone = if options.paused {
        Tone::Salmon
    } else if nominal {
        Tone::Mint
    } else {
        Tone::Amber
    };
    buffer.fill(x, y, width, 2, tone);
    let pulse = if options.ascii {
        "*"
    } else if options.tick.is_multiple_of(2) {
        "◉"
    } else {
        "●"
    };
    let label = if options.paused {
        "UPDATES PAUSED"
    } else if nominal {
        "ALL SYSTEMS NOMINAL"
    } else {
        "TELEMETRY SYNC"
    };
    buffer.text(x + 2, y, &format!("{pulse} {label}"), Tone::Ink, tone);
    buffer.text(x + 2, y + 1, "STARFLEET ENGINEERING", Tone::Ink, tone);
}

fn memory_readout(buffer: &mut Buffer, x: u16, y: u16, snapshot: &Snapshot) {
    pill(buffer, x, y, 13, "MEMORY", Tone::Lavender);
    if let Some(memory) = snapshot.memory {
        buffer.text(
            x + 15,
            y,
            &format!(
                "{} USED  /  {} FREE  /  {} TOTAL",
                format_bytes(memory.used),
                format_bytes(memory.available),
                format_bytes(memory.total)
            ),
            Tone::Text,
            Tone::Background,
        );
        buffer.text(
            x + 15,
            y + 1,
            &format!(
                "{:.1}% ALLOCATED",
                percent(memory.used, memory.total).unwrap_or(0.0)
            ),
            Tone::Dim,
            Tone::Background,
        );
    } else {
        buffer.text(x + 15, y, "UNAVAILABLE", Tone::Dim, Tone::Background);
    }
}

fn disk_readout(buffer: &mut Buffer, x: u16, y: u16, snapshot: &Snapshot) {
    pill(buffer, x, y, 13, "ROOT /", Tone::Amber);
    if let Some(disk) = snapshot.disk {
        buffer.text(
            x + 15,
            y,
            &format!(
                "{} USED  /  {} FREE  /  {} TOTAL",
                format_bytes(disk.used),
                format_bytes(disk.available),
                format_bytes(disk.total)
            ),
            Tone::Text,
            Tone::Background,
        );
        buffer.text(
            x + 15,
            y + 1,
            &format!(
                "{:.1}% CAPACITY",
                percent(disk.used, disk.total).unwrap_or(0.0)
            ),
            Tone::Dim,
            Tone::Background,
        );
    } else {
        buffer.text(x + 15, y, "UNAVAILABLE", Tone::Dim, Tone::Background);
    }
}

fn network_readout(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    snapshot: &Snapshot,
    options: RenderOptions,
) {
    pill(buffer, x, y, 13, "NETWORK", Tone::Blue);
    if let Some(network) = snapshot.network {
        let (down, up) = if options.ascii {
            ("v", "^")
        } else {
            ("▼", "▲")
        };
        buffer.text(
            x + 15,
            y,
            &format!(
                "{down} {} RX  /  {up} {} TX",
                format_rate(network.receive),
                format_rate(network.transmit)
            ),
            Tone::Text,
            Tone::Background,
        );
    } else {
        buffer.text(x + 15, y, "ACQUIRING BASELINE", Tone::Dim, Tone::Background);
    }
}

fn pill(buffer: &mut Buffer, x: u16, y: u16, width: u16, label: &str, tone: Tone) {
    buffer.fill(x, y, width, 1, tone);
    buffer.text(x + 1, y, label, Tone::Ink, tone);
}

fn sweep(buffer: &mut Buffer, x: u16, y: u16, width: u16, tone: Tone, ascii: bool) {
    for offset in 0..width {
        let glyph = if ascii {
            '-'
        } else if offset + 3 >= width {
            '╸'
        } else {
            '━'
        };
        buffer.set(x + offset, y, glyph, tone, Tone::Background);
    }
}

fn core_grid(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    cores: &[f32],
    ascii: bool,
) {
    if cores.is_empty() {
        buffer.text(x, y, "CORE DATA UNAVAILABLE", Tone::Dim, Tone::Background);
        return;
    }
    let columns = cores.len().div_ceil(usize::from(height).max(1)).max(1);
    let rows = cores.len().div_ceil(columns);
    let column_width = (usize::from(width) / columns).max(1);
    for (index, usage) in cores.iter().enumerate() {
        let column = index / rows;
        let row = index % rows;
        let cell_x = x as usize + column * column_width;
        let cell_y = y as usize + row;
        if cell_y >= usize::from(y + height) || cell_x >= usize::from(buffer.width()) {
            continue;
        }
        let label = format!("C{index:02}");
        buffer.text(
            cell_x as u16,
            cell_y as u16,
            &label,
            Tone::Dim,
            Tone::Background,
        );
        if column_width >= 22 {
            let bar_width = column_width - 13;
            let filled = (usage.clamp(0.0, 100.0) / 100.0 * bar_width as f32).round() as usize;
            let mut bar = String::new();
            for position in 0..bar_width {
                bar.push(if position < filled {
                    if ascii { '#' } else { '━' }
                } else if ascii {
                    '-'
                } else {
                    '─'
                });
            }
            buffer.text(
                (cell_x + 4) as u16,
                cell_y as u16,
                &bar,
                Tone::Blue,
                Tone::Background,
            );
            buffer.text(
                (cell_x + column_width - 7) as u16,
                cell_y as u16,
                &format!("{usage:5.1}%"),
                Tone::Text,
                Tone::Background,
            );
        } else {
            buffer.text(
                (cell_x + 4) as u16,
                cell_y as u16,
                &format!("{usage:4.0}%"),
                Tone::Text,
                Tone::Background,
            );
        }
    }
}

fn process_list(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    snapshot: &Snapshot,
    options: RenderOptions,
) {
    if buffer.width() >= 100 {
        buffer.text(x, y, "PID", Tone::Dim, Tone::Background);
        buffer.text(x + 8, y, "PROCESS", Tone::Dim, Tone::Background);
        buffer.text(x + 31, y, "CPU", Tone::Dim, Tone::Background);
        buffer.text(x + 41, y, "MEMORY", Tone::Dim, Tone::Background);
    }
    let start = y + u16::from(buffer.width() >= 100);
    if snapshot.top_processes.is_empty() {
        buffer.text(
            x,
            start + 1,
            "PROCESS DATA UNAVAILABLE",
            Tone::Dim,
            Tone::Background,
        );
        return;
    }
    let name_width = if buffer.width() >= 100 { 20 } else { 15 };
    for (index, process) in snapshot.top_processes.iter().take(5).enumerate() {
        let row = start + 1 + index as u16;
        buffer.text(
            x,
            row,
            &format!("{:>6}", process.pid),
            Tone::Amber,
            Tone::Background,
        );
        buffer.text(
            x + 8,
            row,
            &clip(&process.name, name_width),
            Tone::Text,
            Tone::Background,
        );
        let cpu_x = if buffer.width() >= 100 {
            x + 31
        } else {
            x + 26
        };
        let mem_x = if buffer.width() >= 100 {
            x + 41
        } else {
            x + 35
        };
        if mem_x < x + width {
            buffer.text(
                cpu_x,
                row,
                &format!("{:>6.1}%", process.cpu),
                Tone::Salmon,
                Tone::Background,
            );
            buffer.text(
                mem_x,
                row,
                &format_bytes(process.memory),
                Tone::Blue,
                Tone::Background,
            );
        }
    }
    if !options.ascii && buffer.width() >= 100 {
        sweep(buffer, x, start + 7, width.min(45), Tone::Salmon, false);
    }
}

fn render_help(buffer: &mut Buffer, options: RenderOptions) {
    let width = buffer.width();
    let height = buffer.height();
    if width < 30 || height < 10 {
        buffer.text(0, 0, "Q QUIT / ? CLOSE", Tone::Ink, Tone::Amber);
        return;
    }
    let panel_width = 52.min(width - 2);
    let panel_height = 13.min(height - 2);
    let x = (width - panel_width) / 2;
    let y = (height - panel_height) / 2;
    buffer.fill(x, y, panel_width, panel_height, Tone::Amber);
    buffer.text(
        x + 3,
        y + 1,
        "LCARS // COMMAND REFERENCE",
        Tone::Ink,
        Tone::Amber,
    );
    let entries = [
        ("Q / ESC", "Return terminal to shell"),
        ("SPACE", "Pause or resume sampling"),
        ("P", "Toggle process diagnostics"),
        ("TAB", "Cycle command views"),
        ("?", "Close this reference"),
    ];
    for (index, (key, description)) in entries.iter().enumerate() {
        let row = y + 3 + index as u16;
        if row + 1 >= y + panel_height {
            break;
        }
        buffer.text(x + 3, row, key, Tone::Ink, Tone::Amber);
        buffer.text(x + 14, row, description, Tone::Ink, Tone::Amber);
    }
    if panel_height >= 12 {
        let mode = if options.ascii {
            "ASCII SAFE"
        } else {
            "UNICODE"
        };
        buffer.text(x + 3, y + 10, mode, Tone::Ink, Tone::Amber);
        buffer.text(x + 18, y + 10, "PRESS ? TO RESUME", Tone::Ink, Tone::Amber);
    }
}

fn unavailable(buffer: &mut Buffer, x: u16, y: u16, options: RenderOptions) {
    buffer.text(
        x,
        y,
        if options.ascii { "N/A" } else { "— N/A —" },
        Tone::Dim,
        Tone::Background,
    );
}

fn view_label(view: View) -> &'static str {
    match view {
        View::Primary => "COMMAND",
        View::Diagnostics => "DIAGNOSTIC",
    }
}

fn clip(text: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut width = 0;
    for character in text.chars() {
        if character.is_control() {
            continue;
        }
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width + character_width > max_width {
            break;
        }
        result.push(character);
        width += character_width;
    }
    result
}

const DIGITS: [[&str; 7]; 11] = [
    [
        "01110", "11011", "11011", "11011", "11011", "11011", "01110",
    ],
    [
        "00100", "01100", "00100", "00100", "00100", "00100", "01110",
    ],
    [
        "01110", "11011", "00011", "00110", "01100", "11000", "11111",
    ],
    [
        "11110", "00011", "00011", "01110", "00011", "00011", "11110",
    ],
    [
        "11011", "11011", "11011", "11111", "00011", "00011", "00011",
    ],
    [
        "11111", "11000", "11000", "11110", "00011", "00011", "11110",
    ],
    [
        "01110", "11000", "11000", "11110", "11011", "11011", "01110",
    ],
    [
        "11111", "00011", "00110", "00110", "01100", "01100", "01100",
    ],
    [
        "01110", "11011", "11011", "01110", "11011", "11011", "01110",
    ],
    [
        "01110", "11011", "11011", "01111", "00011", "00011", "01110",
    ],
    [
        "00000", "00000", "00100", "00000", "00100", "00000", "00000",
    ],
];

fn large_time(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    time: &str,
    tone: Tone,
    scale: u16,
    ascii: bool,
) {
    large_text(buffer, x, y, time, tone, scale, ascii);
}

fn large_text(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    text: &str,
    tone: Tone,
    scale: u16,
    ascii: bool,
) {
    let mut cursor = x;
    for character in text.chars() {
        let pattern = match character {
            '0'..='9' => &DIGITS[(character as u8 - b'0') as usize],
            ':' => &DIGITS[10],
            '%' => {
                buffer.text(cursor, y + 3, "%", tone, Tone::Background);
                cursor += 2 * scale;
                continue;
            }
            _ => {
                buffer.text(
                    cursor,
                    y + 3,
                    &character.to_string(),
                    tone,
                    Tone::Background,
                );
                cursor += 3 * scale;
                continue;
            }
        };
        for row in 0..4 {
            for column in 0..5 {
                let top = pattern[row * 2].as_bytes()[column] == b'1';
                let bottom = row * 2 + 1 < 7 && pattern[row * 2 + 1].as_bytes()[column] == b'1';
                let glyph = if ascii {
                    if top || bottom { '#' } else { ' ' }
                } else {
                    match (top, bottom) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    }
                };
                if glyph != ' ' {
                    for repeat in 0..scale {
                        buffer.set(
                            cursor + column as u16 * scale + repeat,
                            y + row as u16,
                            glyph,
                            tone,
                            Tone::Background,
                        );
                    }
                }
            }
        }
        cursor += if character == ':' {
            4 * scale
        } else {
            6 * scale
        };
    }
}

fn braille_trace(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    history: &History,
    ascii: bool,
) {
    if history.is_empty() || width == 0 || height == 0 {
        return;
    }
    let values: Vec<f32> = history.samples().collect();
    if ascii {
        for column in 0..width {
            let index = values.len()
                - 1
                - (usize::from(width - column - 1) * values.len() / usize::from(width))
                    .min(values.len() - 1);
            let row = ((100.0 - values[index]) / 100.0 * (height - 1) as f32).round() as u16;
            buffer.set(x + column, y + row, '*', Tone::Blue, Tone::Background);
        }
        return;
    }
    let dot_columns = usize::from(width) * 2;
    let dot_rows = usize::from(height) * 4;
    for dot_x in 0..dot_columns {
        let from_end = (dot_columns - 1 - dot_x) * values.len() / dot_columns;
        let index = values.len() - 1 - from_end.min(values.len() - 1);
        let value = values[index].clamp(0.0, 100.0);
        let dot_y = ((100.0 - value) / 100.0 * (dot_rows - 1) as f32).round() as usize;
        let cell_x = dot_x / 2;
        let cell_y = dot_y / 4;
        let current = buffer
            .get(x + cell_x as u16, y + cell_y as u16)
            .unwrap_or_default()
            .glyph;
        let bits = if current == ' ' {
            0
        } else {
            current as u32 - 0x2800
        };
        let bit = match (dot_x % 2, dot_y % 4) {
            (0, 0) => 0,
            (0, 1) => 1,
            (0, 2) => 2,
            (1, 0) => 3,
            (1, 1) => 4,
            (1, 2) => 5,
            (0, 3) => 6,
            (1, 3) => 7,
            _ => unreachable!(),
        };
        let glyph = char::from_u32(0x2800 | bits | (1 << bit)).unwrap_or(' ');
        buffer.set(
            x + cell_x as u16,
            y + cell_y as u16,
            glyph,
            Tone::Blue,
            Tone::Background,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_preserves_unicode_width() {
        assert_eq!(clip("ab界cd", 4), "ab界");
        assert_eq!(clip("ab界cd", 3), "ab");
    }
}
