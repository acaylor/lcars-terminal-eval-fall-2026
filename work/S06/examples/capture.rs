use lcars_monitor::buffer::{Buffer, Tone};
use lcars_monitor::metrics::Collector;
use lcars_monitor::model::History;
use lcars_monitor::render::{RenderOptions, View, render};
use std::fmt::Write as _;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut collector = Collector::new()?;
    for (view, name) in [(View::Primary, "primary"), (View::Diagnostics, "secondary")] {
        let (snapshot, history) = samples(&mut collector, view);
        let buffer = render(&snapshot, &history, view, 120, 36, RenderOptions::default());
        fs::write(
            format!("captures/{name}-120x36.txt"),
            format!("{}\n", buffer.plain_text()),
        )?;
        fs::write(format!("captures/{name}-120x36.svg"), svg(&buffer))?;
    }
    Ok(())
}

fn samples(collector: &mut Collector, view: View) -> (lcars_monitor::model::Snapshot, History) {
    let mut snapshot = collector.sample(view == View::Diagnostics);
    let mut history = History::default();
    history.push(snapshot.cpu.unwrap_or(0.0));
    for _ in 0..12 {
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        snapshot = collector.sample(view == View::Diagnostics);
        history.push(snapshot.cpu.unwrap_or(0.0));
    }
    (snapshot, history)
}

fn svg(buffer: &Buffer) -> String {
    const CELL_WIDTH: u32 = 9;
    const CELL_HEIGHT: u32 = 18;
    let width = u32::from(buffer.width()) * CELL_WIDTH;
    let height = u32::from(buffer.height()) * CELL_HEIGHT;
    let mut output = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><rect width=\"100%\" height=\"100%\" fill=\"{}\"/><g font-family=\"SFMono-Regular,Consolas,Liberation Mono,monospace\" font-size=\"15\">\n",
        color(Tone::Background)
    );
    for y in 0..buffer.height() {
        for x in 0..buffer.width() {
            let cell = buffer.get(x, y).expect("capture coordinates are valid");
            let px = u32::from(x) * CELL_WIDTH;
            let py = u32::from(y) * CELL_HEIGHT;
            if cell.bg != Tone::Background {
                let _ = write!(
                    output,
                    "<rect x=\"{px}\" y=\"{py}\" width=\"{CELL_WIDTH}\" height=\"{CELL_HEIGHT}\" fill=\"{}\"/>",
                    color(cell.bg)
                );
            }
            if cell.glyph != ' ' && cell.glyph != '\0' {
                let _ = write!(
                    output,
                    "<text x=\"{px}\" y=\"{}\" fill=\"{}\">{}</text>",
                    py + 14,
                    color(cell.fg),
                    escape(cell.glyph)
                );
            }
        }
        output.push('\n');
    }
    output.push_str("</g></svg>\n");
    output
}

fn escape(character: char) -> String {
    match character {
        '&' => "&amp;".into(),
        '<' => "&lt;".into(),
        '>' => "&gt;".into(),
        other => other.to_string(),
    }
}

fn color(tone: Tone) -> &'static str {
    match tone {
        Tone::Background => "#0e0c16",
        Tone::Text => "#f4edde",
        Tone::Dim => "#776d84",
        Tone::Orange => "#ff9948",
        Tone::Amber => "#ffc65c",
        Tone::Salmon => "#ff776f",
        Tone::Lavender => "#b19de0",
        Tone::Blue => "#68b2e2",
        Tone::Mint => "#7cdcb5",
        Tone::Ink => "#19131e",
    }
}
