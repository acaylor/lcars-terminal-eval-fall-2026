//! Help overlay: a small elbow-framed panel listing the controls.

use super::shapes::{pill, Align, Corner, Frame};
use super::theme::*;
use super::Scene;
use crate::canvas::{Canvas, Rect, Style};

const ROWS: [(&str, &str); 6] = [
    ("Q", "EXIT  (ESC CLOSES THIS PANEL FIRST)"),
    ("?", "TOGGLE THIS PANEL"),
    ("SPC", "HOLD / RESUME TELEMETRY"),
    ("P", "OPEN / CLOSE DIAGNOSTIC VIEW"),
    ("TAB", "CYCLE SENSOR FOCUS  (SHIFT-TAB BACK)"),
    ("ESC", "EXIT"),
];

pub fn render(c: &mut Canvas, s: &Scene) {
    let (w, h) = (c.width(), c.height());
    let pw = 52.min(w - 2);
    let ph = 13.min(h - 2);
    if pw < 20 || ph < 5 {
        // No room for the panel: a one-line legend instead.
        let t = "Q QUIT  SPC HOLD  P DIAG  TAB FOCUS  ? CLOSE";
        c.fill(Rect::new(0, h - 1, w, 1), ' ', Style::on(INK, AMBER));
        c.label(0, h - 1, t, INK, true);
        return;
    }
    let r = Rect::new((w - pw) / 2, (h - ph) / 2, pw, ph);
    // Clear a margin around the panel so it reads as a separate layer.
    c.fill(Rect::new(r.x - 1, r.y - 1, r.w + 2, r.h + 2), ' ', Style::default());
    let mut f = Frame::new(w, h);
    f.elbow_px(2 * r.x, 2 * r.y, 2 * (r.x + r.w), 2 * r.bottom(), 8, 2, Corner::TopLeft, (6, 3), (3, 2), AMBER);
    f.rounded_px((2 * r.x, 2 * r.bottom() - 2, 2 * (r.x + r.w), 2 * r.bottom()), (3, 1), [false, true, true, true], LAVENDER);
    f.blit(c);
    c.label(r.x + 6, r.y, "HELP · COMMAND KEYS", INK, true);

    let x = r.x + 6;
    for (y, (key, what)) in (r.y + 2..r.bottom() - 2).zip(ROWS) {
        pill(c, x, y, 7, PEACH, key, Align::Center);
        c.text_max(x + 9, y, what, Style::fg(ICE), r.right() - x - 10);
    }
    let info = format!("{} · INTERVAL {:.1}S", s.mode_label, s.interval.as_secs_f32());
    c.label(r.x + 6, r.bottom() - 1, &crate::canvas::ellipsize(&info, r.w - 8), INK, true);
}
