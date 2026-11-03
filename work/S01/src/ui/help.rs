//! Help overlay: a small LCARS card over the current view.

use super::frame;
use super::Scene;
use crate::term::buffer::fit;
use crate::term::{Buffer, Color, Glyphs, Style};

pub fn draw(buf: &mut Buffer, g: &Glyphs, scene: &Scene) {
    let w = buf.width() as i32;
    let h = buf.height() as i32;
    let lines: Vec<(&str, String)> = vec![
        ("Q  ESC", "QUIT".into()),
        ("?", "TOGGLE THIS REFERENCE".into()),
        ("SPACE", "HOLD / RESUME SAMPLING".into()),
        ("P", "OPEN / CLOSE PROCESS DIAGNOSTICS".into()),
        ("TAB", "CYCLE VIEWS".into()),
        ("", String::new()),
        (
            "MODE",
            format!(
                "{} {} {}MS",
                if g.unicode { "UNICODE" } else { "ASCII" },
                scene.color_mode.label(),
                scene.interval_ms
            ),
        ),
    ];
    let mut warn: Vec<String> = scene.warnings.iter().take(3).map(|s| s.to_uppercase()).collect();
    if warn.is_empty() {
        warn.push("ALL COLLECTORS NOMINAL".into());
    }
    let cw = (52).min(w - 2).max(10);
    let ch = (lines.len() as i32 + warn.len() as i32 + 4).min(h - 1).max(4);
    let x0 = (w - cw) / 2;
    let y0 = (h - ch) / 2;
    // Card body: black field so the view behind does not bleed through.
    buf.fill(x0, y0, cw, ch, ' ', Style::default());
    // Title bar with rounded ends and a left rail, the same language as the frame.
    let bar = Style::fg(Color::LAVENDER);
    buf.set(x0, y0, g.q_lr, bar);
    for x in x0 + 1..x0 + cw - 1 {
        buf.set(x, y0, g.lower, bar);
    }
    buf.set(x0 + cw - 1, y0, g.q_ll, bar);
    for y in y0 + 1..y0 + ch - 1 {
        buf.set(x0, y, g.full, bar);
        buf.set(x0 + 1, y, g.full, bar);
    }
    buf.set(x0, y0 + ch - 1, g.q_ur, bar);
    buf.set(x0 + 1, y0 + ch - 1, g.upper, bar);
    for x in x0 + 2..x0 + cw - 1 {
        buf.set(x, y0 + ch - 1, g.upper, Style::fg(Color::ORANGE));
    }
    buf.set(x0 + cw - 1, y0 + ch - 1, g.q_ul, Style::fg(Color::ORANGE));
    frame::pill(buf, g, x0 + 4, y0 + 1, "COMMAND REFERENCE", Color::ORANGE);
    let mut y = y0 + 3;
    let inner = (cw - 6).max(0) as usize;
    for (key, text) in &lines {
        if y >= y0 + ch - 1 {
            break;
        }
        buf.put_str(x0 + 4, y, key, Style::fg(Color::AMBER).bold());
        buf.put_str(
            x0 + 13,
            y,
            &fit(text, inner.saturating_sub(9), g.ellipsis),
            Style::fg(Color::IVORY),
        );
        y += 1;
    }
    for wline in &warn {
        if y >= y0 + ch - 1 {
            break;
        }
        buf.put_str(x0 + 4, y, &fit(wline, inner, g.ellipsis), Style::fg(Color::DIM));
        y += 1;
    }
}
