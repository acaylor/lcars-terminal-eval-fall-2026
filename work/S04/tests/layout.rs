use lcars::metrics::{demo_sample, Collector, ProcessRow};
use lcars::render::{diff_rows, render, Buffer, ColorMode, View, ViewState, BLACK};
use std::time::Duration;

fn ui(view: View) -> ViewState {
    let mut ui = ViewState::new(ColorMode::True, true);
    ui.view = view;
    ui.cpu_trace = vec![10.0, 40.0, 20.0, 80.0, 15.0];
    ui
}

fn primary() -> lcars::render::Buffer {
    render(&demo_sample(), &ui(View::Primary), 120, 36)
}

fn text_of(buf: &Buffer) -> String {
    buf.text_content()
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

fn occupied(cell: lcars::render::Cell) -> bool {
    cell.bg != BLACK || cell.ch != ' '
}

#[test]
fn primary_is_asymmetric_with_negative_space() {
    let buf = primary();
    assert!(buf.empty_ratio() >= 0.20, "empty {}", buf.empty_ratio());
    assert!(occupied(buf.cell(2, 1)), "top rail missing");
    assert!(!occupied(buf.cell(119, 0)), "top-right should be open");
    assert!(!occupied(buf.cell(0, 35)), "bottom-left should be open");
    assert!(occupied(buf.cell(40, 34)), "bottom rail missing");
    let left = (0..36).filter(|y| occupied(buf.cell(1, *y))).count();
    let right = (0..36).filter(|y| occupied(buf.cell(118, *y))).count();
    assert!(left > 0 && right > 0);
    assert_ne!(left, right);
}

#[test]
fn primary_hides_process_list_and_shows_core_metrics() {
    let buf = primary();
    let text = text_of(&buf);
    assert!(!text.contains("warp-core"), "{text}");
    assert!(!text.contains("plasma-injector"));
    assert!(text.contains("21:14:07") || text.contains("LCARS"));
    assert!(text.contains("CPU"));
    assert!(text.contains("MEM"));
    assert!(text.contains("AVL"));
    assert!(text.contains("RX"));
    assert!(text.contains("TX"));
    assert!(text.contains("DSK"));
    assert!(text.contains("LOAD") || text.contains("0.42"));
    assert!(text.contains("SEP"));
    assert!(text.contains("bridge-ops"));
    assert!(text.contains('Q'));
}

#[test]
fn secondary_shows_processes_and_differs() {
    let sample = demo_sample();
    let buf = render(&sample, &ui(View::Diag), 120, 36);
    let text = text_of(&buf);
    assert!(text.contains("warp-core"), "{text}");
    assert!(text.contains("1420"));
    assert!(text.contains("ユニコード") || text.contains("904"));
    assert!(text.contains("DIAG"));
    assert!(text.contains("x86_64"));
    assert!(buf.empty_ratio() >= 0.20);
    assert!(
        !occupied(buf.cell(0, 0)),
        "secondary top-left should be open"
    );
    assert!(occupied(buf.cell(119, 1)), "secondary right rail missing");
}

#[test]
fn standard_and_tiny_layouts_do_not_panic_or_leak_processes() {
    let sample = demo_sample();
    let std = render(&sample, &ui(View::Primary), 80, 24);
    let text = text_of(&std);
    assert!(!text.contains("warp-core"));
    assert!(text.contains("LCARS") || text.contains("CPU"));
    assert!(std.empty_ratio() >= 0.15);
    let tiny = render(&sample, &ui(View::Primary), 10, 5);
    assert_eq!(tiny.width, 10);
    assert_eq!(tiny.height, 5);
    let _ = render(&sample, &ui(View::Diag), 20, 8);
    let _ = render(&sample, &ui(View::Primary), 1, 1);
}

#[test]
fn ascii_and_mono_fallbacks() {
    let mut ui = ui(View::Primary);
    ui.unicode = false;
    let buf = render(&demo_sample(), &ui, 120, 36);
    let text = buf.text_content();
    assert!(
        text.chars().all(|c| c == '\n' || c.is_ascii()),
        "non-ascii in ascii mode"
    );
    ui.unicode = true;
    ui.color = ColorMode::Mono;
    let buf = render(&demo_sample(), &ui, 120, 36);
    let bytes = diff_rows(&Buffer::new(1, 1), &buf, ColorMode::Mono, true);
    let s = String::from_utf8_lossy(&bytes);
    assert!(!s.contains("38;2;"));
}

#[test]
fn long_hostname_is_truncated() {
    let mut sample = demo_sample();
    sample.hostname = "engineering-workstation-with-a-very-long-name.example".into();
    let buf = render(&sample, &ui(View::Primary), 120, 36);
    let text = text_of(&buf);
    assert!(!text.contains("engineering-workstation-with-a-very-long-name.example"));
    assert!(text.contains("engineering"));
}

#[test]
fn unavailable_metrics_are_not_fake_zeros() {
    let mut sample = demo_sample();
    sample.cpu_pct = None;
    sample.net_rx_bps = None;
    sample.net_tx_bps = None;
    sample.load = None;
    sample.disk_total = None;
    sample.disk_used = None;
    let buf = render(&sample, &ui(View::Primary), 120, 36);
    let text = text_of(&buf);
    assert!(text.contains("CPU"));
    assert!(text.contains('—') || text.contains("--"));
    assert!(!text.contains("CPU 0%"));
}

#[test]
fn focus_and_help_change_the_frame() {
    let sample = demo_sample();
    let mut a = ui(View::Primary);
    let mut b = ui(View::Primary);
    b.focus = 2;
    b.help = true;
    let ba = render(&sample, &a, 120, 36);
    let bb = render(&sample, &b, 120, 36);
    assert_ne!(ba.text_content(), bb.text_content());
    assert!(bb.text_content().contains("QUIT"));
    a.paused = true;
    let held = render(&sample, &a, 120, 36);
    assert!(held.text_content().contains("HOLD"));
}

#[test]
fn live_sample_survives_missing_fields() {
    let mut c = Collector::new();
    let first = c.sample(false);
    assert!(!first.hostname.is_empty());
    assert!(first.cpu_pct.is_none(), "first cpu sample has no delta");
    std::thread::sleep(Duration::from_millis(50));
    let second = c.sample(true);
    assert!(second.mem_total.unwrap_or(0) > 0);
    assert!(second.disk_total.unwrap_or(0) > 0);
    assert!(second.load.is_some());
    assert!(second.process_count.unwrap_or(0) > 0);
    assert!(second.top_cpu.len() <= 5);
    for row in &second.top_cpu {
        assert!(!row.name.chars().any(|ch| ch.is_control()));
    }
    let _ = ProcessRow {
        pid: 1,
        name: "x".into(),
        cpu_pct: None,
        mem_bytes: None,
    };
}
