use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use lcars_monitor::buffer::{Buffer, Presenter};
use lcars_monitor::clock::Clock;
use lcars_monitor::collect::{Collector, SysinfoCollector};
use lcars_monitor::model::Metrics;
use lcars_monitor::theme::{rgb, ColorCapability, Ink, Theme, BOLD, REVERSE};
use lcars_monitor::views::{render_frame, UiState, View};

fn main() -> Result<(), String> {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("captures");
    fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    let mut collector: Box<dyn Collector> = Box::new(SysinfoCollector::new());
    let mut cpu_history = Vec::new();
    let mut rx_history = Vec::new();
    let mut tx_history = Vec::new();
    let mut last_metrics: Option<Metrics> = None;

    // A handful of real one-second samples gives the traces something to show
    // without fabricating any data.
    std::thread::sleep(Duration::from_millis(250));
    for i in 0..10 {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(1000));
        }
        let metrics = collector.sample(Duration::from_millis(1000), true);
        if let Some(cpu) = metrics.cpu {
            push_capped(&mut cpu_history, cpu, 120);
        }
        if let Some(rx) = metrics.net_rx_rate {
            push_capped(&mut rx_history, rx, 120);
        }
        if let Some(tx) = metrics.net_tx_rate {
            push_capped(&mut tx_history, tx, 120);
        }
        last_metrics = Some(metrics);
    }

    let metrics = last_metrics.unwrap_or_default();
    let clock = Clock::now_local();
    let theme = Theme::with_capability(ColorCapability::Truecolor, false);
    let unicode = true;

    for (name, view, w, h) in [
        ("primary-120x36", View::Command, 120, 36),
        ("diagnostics-120x36", View::Diagnostics, 120, 36),
        ("primary-80x24", View::Command, 80, 24),
        ("diagnostics-80x24", View::Diagnostics, 80, 24),
        ("help-120x36", View::Command, 120, 36),
    ] {
        let state = UiState {
            metrics: &metrics,
            clock: clock.as_ref(),
            paused: false,
            view,
            help: name.starts_with("help"),
            phase: 6,
            cpu_history: &cpu_history,
            net_rx_history: &rx_history,
            net_tx_history: &tx_history,
            unicode,
            theme: &theme,
            interval_secs: 1.0,
        };
        let frame = render_frame(&state, w, h);
        write_capture(&out_dir, name, &frame, &theme, unicode)?;
    }

    let ascii_state = UiState {
        metrics: &metrics,
        clock: clock.as_ref(),
        paused: false,
        view: View::Command,
        help: false,
        phase: 6,
        cpu_history: &cpu_history,
        net_rx_history: &rx_history,
        net_tx_history: &tx_history,
        unicode: false,
        theme: &theme,
        interval_secs: 1.0,
    };
    let ascii = render_frame(&ascii_state, 120, 36);
    fs::write(out_dir.join("primary-ascii-120x36.txt"), ascii.plain_text())
        .map_err(|e| e.to_string())?;

    println!("wrote captures to {}", out_dir.display());
    Ok(())
}

fn write_capture(
    dir: &Path,
    name: &str,
    frame: &Buffer,
    theme: &Theme,
    unicode: bool,
) -> Result<(), String> {
    fs::write(dir.join(format!("{name}.txt")), frame.plain_text()).map_err(|e| e.to_string())?;
    let mut presenter = Presenter::new();
    let ansi = presenter.present(frame, theme);
    fs::write(dir.join(format!("{name}.ansi")), ansi.as_bytes()).map_err(|e| e.to_string())?;
    let html = to_html(frame, theme, unicode);
    fs::write(dir.join(format!("{name}.html")), html).map_err(|e| e.to_string())?;
    Ok(())
}

fn to_html(frame: &Buffer, theme: &Theme, unicode: bool) -> String {
    let mut out = String::with_capacity(frame.cells.len() * 80);
    out.push_str(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>LCARS 47 capture</title>",
    );
    out.push_str("<style>");
    out.push_str("html,body{margin:0;background:#050508;}");
    out.push_str("body{padding:18px;}");
    out.push_str("pre{margin:0;padding:0;display:block;background:#050508;}");
    out.push_str(".r{height:17px;line-height:17px;white-space:nowrap;margin:0;padding:0;}");
    out.push_str(".c{display:inline-block;width:10.2px;height:17px;line-height:17px;");
    out.push_str("text-align:center;vertical-align:top;overflow:hidden;");
    out.push_str("font-family:\"DejaVu Sans Mono\",\"Liberation Mono\",monospace;font-size:17px;");
    out.push_str("font-variant-ligatures:none;}");
    out.push_str("</style></head><body><pre>");
    for y in 0..frame.height {
        out.push_str("<div class=\"r\">");
        for x in 0..frame.width {
            let cell = frame.cell(x, y).unwrap();
            out.push_str("<span class=\"c\" style=\"");
            out.push_str(&style_for(cell.fg, cell.bg, cell.attrs, theme));
            out.push_str("\">");
            escape(&mut out, cell.ch, unicode);
            out.push_str("</span>");
        }
        out.push_str("</div>");
    }
    out.push_str("</pre></body></html>\n");
    out
}

fn style_for(fg: Ink, bg: Ink, attrs: u8, theme: &Theme) -> String {
    let mut fg_rgb = rgb(fg).unwrap_or((247, 242, 231));
    let mut bg_rgb = rgb(bg).unwrap_or((5, 5, 8));
    if attrs & REVERSE != 0 {
        std::mem::swap(&mut fg_rgb, &mut bg_rgb);
    }
    let mut style = format!(
        "color:rgb({},{},{});background-color:rgb({},{},{})",
        fg_rgb.0, fg_rgb.1, fg_rgb.2, bg_rgb.0, bg_rgb.1, bg_rgb.2
    );
    if theme.mono {
        style.push_str(";filter:grayscale(1)");
    }
    if attrs & BOLD != 0 {
        style.push_str(";font-weight:bold");
    }
    style
}

fn escape(out: &mut String, ch: char, unicode: bool) {
    let ch = if !unicode && !ch.is_ascii() { '?' } else { ch };
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        ' ' => out.push_str("&#160;"),
        _ => out.push(ch),
    }
}

fn push_capped<T>(values: &mut Vec<T>, value: T, cap: usize) {
    if values.len() >= cap {
        values.remove(0);
    }
    values.push(value);
}
