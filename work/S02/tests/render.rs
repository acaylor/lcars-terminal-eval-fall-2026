//! Terminal-independent rendering tests: every view is drawn into an
//! in-memory canvas from fixture telemetry and inspected directly.

use lcarsmon::ansi::{encode_diff, RenderMode};
use lcarsmon::canvas::{Canvas, WIDE_CONT};
use lcarsmon::clock::LocalTime;
use lcarsmon::color::ColorMode;
use lcarsmon::metrics::history::History;
use lcarsmon::metrics::sampler::{NetRates, ProcSummary, ProcTop, Telemetry};
use lcarsmon::metrics::{Disk, Memory, Missing, Swap, SystemInfo};
use lcarsmon::ui::layout::{self, Tier};
use lcarsmon::ui::{Focus, Scene, Ui, View};
use std::time::Duration;

fn sys() -> SystemInfo {
    SystemInfo { os: "Ubuntu 24.04 LTS".into(), kernel: "Linux 6.8.0".into(), arch: "x86_64".into() }
}

fn telemetry() -> Telemetry {
    Telemetry {
        hostname: Ok("enterprise".into()),
        uptime: Ok(Duration::from_secs(3 * 86_400 + 4_000)),
        load: Ok([1.25, 0.9, 0.7]),
        cpu_pct: Ok(37.0),
        core_pct: Ok(vec![10.0, 80.0, 35.0, 55.0, 5.0, 100.0, 0.0, 42.0]),
        memory: Ok(Memory { total: 16 << 30, available: 9 << 30 }),
        swap: Ok(Swap { total: 4 << 30, used: 1 << 30 }),
        disk: Ok(Disk { mount: "/".into(), total: 500 << 30, used: 200 << 30, available: 300 << 30 }),
        net: Ok(NetRates { rx_bps: 1_250_000.0, tx_bps: 42_000.0, ifaces: 1 }),
        procs: Some(Ok(ProcSummary {
            total: 412,
            unreadable: 3,
            warming: false,
            top: vec![
                ProcTop { pid: 4242, name: "firefox".into(), cpu_pct: 88.5, rss_bytes: 900 << 20, mem_pct: Some(5.5) },
                ProcTop { pid: 7, name: "日本語プロセス名".into(), cpu_pct: 12.0, rss_bytes: 10 << 20, mem_pct: Some(0.1) },
                ProcTop {
                    pid: 99_999,
                    name: "an-extraordinarily-long-process-name-that-never-ends-and-keeps-going".into(),
                    cpu_pct: 3.0,
                    rss_bytes: 1 << 20,
                    mem_pct: None,
                },
                ProcTop { pid: 1, name: "ünïcødé\u{1b}[31m".into(), cpu_pct: 0.5, rss_bytes: 0, mem_pct: Some(0.0) },
            ],
        })),
        samples: 42,
    }
}

fn history() -> History {
    let mut h = History::default();
    for i in 0..300 {
        let t = i as f32 / 10.0;
        h.cpu.push(50.0 + 40.0 * t.sin());
        h.mem.push(40.0 + t.cos());
        h.rx.push(1e6 * (1.0 + t.sin()));
        h.tx.push(3e4 * (1.0 + t.cos()));
        h.load.push(1.0 + 0.5 * t.sin());
    }
    h
}

struct Opts {
    view: View,
    help: bool,
    paused: bool,
    focus: Focus,
}

fn render_with(w: u16, h: u16, tel: &Telemetry, o: Opts) -> Canvas {
    let (hist, sys, time) = (history(), sys(), LocalTime::fixed(13, 37, 42));
    let scene = Scene {
        tel,
        history: &hist,
        sys: &sys,
        time: &time,
        view: o.view,
        focus: o.focus,
        help: o.help,
        paused: o.paused,
        phase: 1.3,
        interval: Duration::from_secs(1),
        mode_label: "TEST",
    };
    let mut c = Canvas::new(w, h);
    Ui::new().render(&mut c, &scene);
    c
}

fn render(w: u16, h: u16, view: View) -> Canvas {
    render_with(w, h, &telemetry(), Opts { view, help: false, paused: false, focus: Focus::Cpu })
}

#[test]
fn every_size_renders_without_panicking() {
    let tel = telemetry();
    let unavailable = Telemetry { procs: Some(Err(Missing::Denied)), ..Telemetry::default() };
    for w in (1..=220).step_by(7) {
        for h in (1..=70).step_by(3) {
            for view in [View::Command, View::Diagnostic] {
                for (t, help) in [(&tel, false), (&unavailable, true)] {
                    let c = render_with(w, h, t, Opts { view, help, paused: help, focus: Focus::Network });
                    assert_eq!(c.size(), (w, h));
                }
            }
        }
    }
}

#[test]
fn reference_sizes_use_the_intended_composition() {
    assert_eq!(layout::tier(80, 24), Tier::Compact);
    assert_eq!(layout::tier(120, 36), Tier::Full);
}

#[test]
fn primary_view_has_at_least_twenty_percent_negative_space() {
    for (w, h) in [(120, 36), (80, 24), (160, 48), (220, 60)] {
        let c = render(w, h, View::Command);
        let blank = c.blank_ratio();
        assert!(blank >= 0.20, "{w}x{h}: only {:.1}% negative space", blank * 100.0);
    }
}

#[test]
fn process_list_only_appears_in_the_diagnostic_view() {
    for (w, h) in [(80, 24), (120, 36), (200, 60)] {
        let primary = render(w, h, View::Command).to_plain_text();
        assert!(!primary.contains("firefox"), "process leaked into primary view at {w}x{h}");
        assert!(!primary.contains("4242"));
        let diag = render(w, h, View::Diagnostic).to_plain_text();
        assert!(diag.contains("firefox"), "diagnostic view at {w}x{h} lacks processes:\n{diag}");
        assert!(diag.contains("4242"));
    }
}

#[test]
fn primary_view_shows_all_core_readouts() {
    let text = render(120, 36, View::Command).to_plain_text();
    for needle in [
        "E N T E R P R I S E",
        "TUESDAY 22 SEP 2026",
        "UPTIME",
        "3D 01:06:40",
        "37%",
        "1.25",
        "0.90",
        "0.70",
        "USED",
        "AVAIL",
        "TOTAL",
        "16.0G",
        "RX",
        "TX",
        "1.19M/s",
        "41.0K/s",
        "DSK",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
    let compact = render(80, 24, View::Command).to_plain_text();
    for needle in ["AVAIL", "TOTAL", "RX", "TX", "DSK", "UPTIME", "? HELP", "Q EXIT"] {
        assert!(compact.contains(needle), "compact view missing {needle:?}:\n{compact}");
    }
}

#[test]
fn diagnostic_view_shows_required_fields() {
    let text = render(120, 36, View::Diagnostic).to_plain_text();
    for needle in ["Ubuntu 24.04 LTS", "x86_64", "CPU CORES · 8", "SWAP", "1.00G", "412", "3 RESTRICTED", "PID", "MEMORY", "88.5%"] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
}

#[test]
fn hostile_names_are_clipped_and_sanitized() {
    let c = render(120, 36, View::Diagnostic);
    let text = c.to_plain_text();
    assert!(!text.contains('\u{1b}'), "escape character reached the canvas");
    assert!(text.contains('…'), "long name should be ellipsized");
    assert!(text.contains("日本語"));
    for line in text.lines() {
        assert!(lcarsmon::canvas::str_width(line) <= 120);
    }
}

#[test]
fn unavailable_metrics_render_tasteful_placeholders() {
    let t = Telemetry { memory: Err(Missing::Denied), disk: Err(Missing::Unsupported), hostname: Err(Missing::Failed), ..Telemetry::default() };
    let text = render_with(120, 36, &t, Opts { view: View::Command, help: false, paused: false, focus: Focus::Load }).to_plain_text();
    assert!(text.contains("DENIED"));
    assert!(text.contains("N/A"));
    assert!(text.contains("SYNC"));
    assert!(!text.contains("NaN") && !text.contains("inf"));
}

#[test]
fn help_overlay_lists_every_required_control() {
    let text = render_with(120, 36, &telemetry(), Opts { view: View::Command, help: true, paused: false, focus: Focus::Cpu }).to_plain_text();
    for needle in ["HELP", "EXIT", "HOLD / RESUME", "DIAGNOSTIC VIEW", "CYCLE SENSOR FOCUS", "SPC", "TAB", "ESC"] {
        assert!(text.contains(needle), "help missing {needle:?}");
    }
}

#[test]
fn focus_changes_the_trace_panel() {
    let t = telemetry();
    let net = render_with(120, 36, &t, Opts { view: View::Command, help: false, paused: false, focus: Focus::Network }).to_plain_text();
    assert!(net.contains("NET TRACE"));
    let mem = render_with(120, 36, &t, Opts { view: View::Command, help: false, paused: false, focus: Focus::Memory }).to_plain_text();
    assert!(mem.contains("MEM TRACE"));
}

#[test]
fn hold_state_is_visible() {
    let t = telemetry();
    let text = render_with(120, 36, &t, Opts { view: View::Command, help: false, paused: true, focus: Focus::Cpu }).to_plain_text();
    assert!(text.contains("TELEMETRY HOLD"));
}

#[test]
fn silhouette_survives_without_text_and_is_asymmetric() {
    // Ignore every text cell: the frame (colored cells and block glyphs)
    // alone must cover a substantial area, and must differ strongly from
    // its own mirror image (an asymmetric composition).
    let c = render(120, 36, View::Command);
    let shape = |x: i32, y: i32| {
        let cell = c.get(x, y).unwrap();
        cell.style.bg != lcarsmon::color::Color::Reset || ('\u{2580}'..='\u{259F}').contains(&cell.ch)
    };
    let (mut ink, mut mismatched) = (0, 0);
    for y in 0..36 {
        for x in 0..120 {
            if shape(x, y) {
                ink += 1;
                if !shape(119 - x, y) {
                    mismatched += 1;
                }
            }
        }
    }
    assert!(ink > 120 * 36 / 5, "frame too faint: {ink} cells");
    let asym = mismatched as f64 / ink as f64;
    assert!(asym > 0.35, "composition too symmetric: only {:.0}% of shape cells lack a mirror twin", asym * 100.0);
}

#[test]
fn ascii_and_mono_modes_stay_ascii_and_colorless() {
    let c = render(120, 36, View::Command);
    let mut out = Vec::new();
    encode_diff(None, &c, RenderMode { color: ColorMode::Mono, ascii: true }, &mut out);
    assert!(out.iter().all(|b| b.is_ascii()), "non-ASCII byte in ASCII mode");
    let s = String::from_utf8(out).unwrap();
    assert!(!s.contains("38;") && !s.contains("48;"));
}

#[test]
fn reduced_color_modes_keep_identical_layout() {
    // The canvas is mode-independent; only the encoder differs. Every mode
    // must position the exact same set of cells.
    let c = render(120, 36, View::Command);
    let mut sizes = Vec::new();
    for color in [ColorMode::TrueColor, ColorMode::Ansi256, ColorMode::Ansi16, ColorMode::Mono] {
        let mut out = Vec::new();
        encode_diff(None, &c, RenderMode { color, ascii: false }, &mut out);
        let text = String::from_utf8(out).unwrap();
        let glyphs: String = strip_ansi(&text);
        sizes.push(glyphs.chars().count());
    }
    assert!(sizes.windows(2).all(|w| w[0] == w[1]), "{sizes:?}");
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn steady_state_frames_are_cheap_to_emit() {
    let a = render(120, 36, View::Command);
    let b = render(120, 36, View::Command);
    let mut full = Vec::new();
    encode_diff(None, &a, RenderMode { color: ColorMode::TrueColor, ascii: false }, &mut full);
    let mut diff = Vec::new();
    encode_diff(Some(&a), &b, RenderMode { color: ColorMode::TrueColor, ascii: false }, &mut diff);
    assert!(diff.is_empty(), "identical scene should emit nothing, got {} bytes", diff.len());
    assert!(full.len() < 120 * 36 * 30, "full paint unexpectedly large: {} bytes", full.len());
}

#[test]
fn clock_change_only_repaints_the_clock_region() {
    let (hist, sys, tel) = (history(), sys(), telemetry());
    let draw = |sec: u8| {
        let time = LocalTime::fixed(13, 37, sec);
        let scene = Scene {
            tel: &tel,
            history: &hist,
            sys: &sys,
            time: &time,
            view: View::Command,
            focus: Focus::Cpu,
            help: false,
            paused: false,
            phase: 0.0,
            interval: Duration::from_secs(1),
            mode_label: "T",
        };
        let mut c = Canvas::new(120, 36);
        Ui::new().render(&mut c, &scene);
        c
    };
    let (a, b) = (draw(41), draw(42));
    let l = layout::primary(120, 36);
    for y in 0..36 {
        for x in 0..120 {
            if a.get(x, y) != b.get(x, y) {
                let inside = x >= l.clock.x && x < l.clock.right() && y >= l.clock.y && y < l.clock.bottom();
                assert!(inside, "cell ({x},{y}) changed outside the clock");
            }
        }
    }
}

#[test]
fn wide_process_names_never_leave_orphan_halves() {
    let c = render(80, 24, View::Diagnostic);
    for y in 0..c.height() {
        let row = c.row(y);
        for x in 0..row.len() {
            if row[x].ch == WIDE_CONT {
                assert!(x > 0 && row[x - 1].ch != WIDE_CONT && row[x - 1].ch != ' ', "orphan continuation at ({x},{y})");
            }
        }
    }
}
