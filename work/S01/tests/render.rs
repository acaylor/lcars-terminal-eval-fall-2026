//! Terminal-independent rendering tests: every view, every tier, many sizes,
//! with a fixed synthetic telemetry stream so the output is deterministic.

use lcars::metrics::rates::CounterWidth;
use lcars::metrics::sampler::{Sampler, Telemetry};
use lcars::metrics::{CpuTicks, Disk, Interface, Memory, ProcessSample, Snapshot, StaticInfo, Swap};
use lcars::term::{Buffer, ColorMode, Glyphs};
use lcars::ui::{self, Clock, Scene, View};
use std::time::{Duration, Instant};

fn synthetic_telemetry() -> Telemetry {
    let base = Instant::now();
    let mut s = Sampler::new(CounterWidth::Bits64);
    for i in 0..30u64 {
        let mut snap = Snapshot::empty(base + Duration::from_millis(i * 1000));
        let busy = i * 37;
        snap.cpu_total = Some(CpuTicks {
            busy,
            idle: i * 100 - busy,
        });
        snap.cpu_cores = (0..8)
            .map(|c| CpuTicks {
                busy: i * (c + 1) * 5,
                idle: i * 100,
            })
            .collect();
        snap.load = Some([1.23, 0.98, 0.75]);
        snap.memory = Some(Memory {
            total: 32 << 30,
            used: 12 << 30,
            available: 20 << 30,
        });
        snap.swap = Some(Swap {
            total: 8 << 30,
            used: 1 << 30,
        });
        snap.disk = Some(Disk {
            mount: "/".into(),
            total: 1 << 40,
            used: 600 << 30,
            available: 400 << 30,
        });
        snap.interfaces = Some(vec![Interface {
            name: "en0".into(),
            rx_bytes: i * 150_000,
            tx_bytes: i * 40_000,
        }]);
        snap.uptime = Some(Duration::from_secs(3 * 86400 + 4 * 3600 + 12 * 60 + 55));
        snap.process_count = Some(412);
        snap.processes = Some(vec![
            ProcessSample {
                pid: 1,
                name: "launchd".into(),
                cpu_time: Duration::from_millis(i * 50),
                rss: 20 << 20,
            },
            ProcessSample {
                pid: 4242,
                name: "a-very-long-process-name-that-will-not-fit-anywhere-at-all-even-on-a-very-wide-terminal-screen"
                    .into(),
                cpu_time: Duration::from_millis(i * 900),
                rss: 2 << 30,
            },
            ProcessSample {
                pid: 7,
                name: "日本語のプロセス".into(),
                cpu_time: Duration::from_millis(i * 10),
                rss: 1 << 20,
            },
            ProcessSample {
                pid: 99999,
                name: "kworker/u16:3".into(),
                cpu_time: Duration::ZERO,
                rss: 0,
            },
            ProcessSample {
                pid: 500,
                name: "firefox".into(),
                cpu_time: Duration::from_millis(i * 300),
                rss: 900 << 20,
            },
            ProcessSample {
                pid: 501,
                name: "sixth".into(),
                cpu_time: Duration::from_millis(i),
                rss: 1,
            },
        ]);
        s.push(snap);
    }
    s.telemetry().clone()
}

fn info(hostname: &str) -> StaticInfo {
    StaticInfo {
        hostname: hostname.into(),
        os: "macOS 26.1".into(),
        kernel: "25.1.0".into(),
        arch: "aarch64".into(),
        cores: 8,
    }
}

fn clock() -> Clock {
    Clock {
        hour: 23,
        minute: 59,
        second: 7,
        weekday: "TUE",
        day: 27,
        month: "OCT",
        year: 2026,
    }
}

fn render(w: usize, h: usize, view: View, g: &Glyphs, help: bool, hostname: &str) -> Buffer {
    let t = synthetic_telemetry();
    let i = info(hostname);
    let c = clock();
    let scene = Scene {
        telemetry: &t,
        info: &i,
        clock: &c,
        view,
        help,
        paused: false,
        frame: 7,
        color_mode: ColorMode::TrueColor,
        interval_ms: 1000,
        warnings: &[],
    };
    let mut b = Buffer::new(w, h);
    ui::draw(&mut b, g, &scene);
    b
}

#[test]
fn primary_view_at_120x36_carries_every_required_metric() {
    let b = render(120, 36, View::Command, &Glyphs::unicode(), false, "enterprise");
    let text = b.rows_text().join("\n");
    assert!(text.contains("TUE 27 OCT 2026"), "date");
    assert!(text.contains("ENTERPRISE"), "hostname in the top rail");
    assert!(text.contains("3D 04:12:55"), "uptime");
    assert!(text.contains("1.23"), "load");
    assert!(
        text.contains("12G USED") && text.contains("20G AVAIL") && text.contains("32G TOTAL"),
        "memory"
    );
    assert!(text.contains("600G USED"), "disk");
    assert!(text.contains("146 KB/S"), "rx rate: {text}");
    assert!(text.contains("39 KB/S"), "tx rate");
    assert!(text.contains("37%"), "cpu percent in the rail or readout");
    // The clock is drawn as block glyphs, so check its footprint instead of text.
    let top_right: String = b.rows_text()[3..8]
        .iter()
        .map(|r| r.chars().skip(78).collect::<String>())
        .collect();
    assert!(top_right.matches('█').count() > 20, "big clock present");
    // No process list on the default screen.
    assert!(!text.contains("PROCESSES"));
    assert!(!text.contains("launchd"));
    assert!(!text.contains("4242"));
}

#[test]
fn negative_space_is_at_least_twenty_percent() {
    let b = render(120, 36, View::Command, &Glyphs::unicode(), false, "enterprise");
    let blank = b
        .rows_text()
        .iter()
        .map(|r| 120 - r.chars().filter(|c| *c != ' ').count())
        .sum::<usize>();
    let ratio = blank as f32 / (120.0 * 36.0);
    assert!(ratio >= 0.20, "negative space {ratio:.2}");
}

#[test]
fn silhouette_is_asymmetric_and_survives_text_removal() {
    let b = render(120, 36, View::Command, &Glyphs::unicode(), false, "enterprise");
    let rows = b.rows_text();
    // Structure glyphs only: count block elements per column half.
    let is_block = |c: char| ('\u{2580}'..='\u{259F}').contains(&c);
    let left: usize = rows
        .iter()
        .map(|r| r.chars().take(12).filter(|c| is_block(*c)).count())
        .sum();
    let right: usize = rows
        .iter()
        .map(|r| r.chars().skip(108).filter(|c| is_block(*c)).count())
        .sum();
    assert!(left > 300 && right * 3 < left, "left edge {left} right edge {right}");
    // Top bar is thicker than the bottom bar.
    assert!(
        rows[1].chars().filter(|c| is_block(*c)).count() > 70,
        "top bar row 1 minus its labels"
    );
    assert!(rows[0].chars().filter(|c| is_block(*c)).count() > 100);
    assert!(
        rows[35].chars().filter(|c| is_block(*c)).count() > 55,
        "bottom bar minus its key labels"
    );
    assert!(rows[34].chars().filter(|c| is_block(*c)).count() < 20);
}

#[test]
fn diagnostic_view_shows_processes_and_cores() {
    let b = render(120, 36, View::Diagnostic, &Glyphs::unicode(), false, "enterprise");
    let text = b.rows_text().join("\n");
    assert!(text.contains("PROCESSES"));
    assert!(text.contains("MACOS 26.1"));
    assert!(text.contains("AARCH64"));
    assert!(text.contains("4242"), "{text}");
    assert!(text.contains("a-very-long-process-name"));
    assert!(text.contains("…"), "long names are truncated with an ellipsis");
    assert!(text.contains("日本語"), "non-ASCII names survive");
    assert!(text.contains("90.0%"), "per-process cpu: {text}");
    assert!(text.contains("2.0G"));
    assert!(text.contains("1.0G USED") && text.contains("8.0G TOTAL"), "swap");
    assert!(text.contains("00 01 02 03 04 05 06 07"), "core labels");
    assert!(!text.contains("kworker"), "only five processes, ranked by cpu");
    // Mirrored silhouette: rail on the right.
    assert!(b.rows_text()[3].ends_with('█'));
    assert!(b.rows_text()[3].starts_with("  "));
}

#[test]
fn help_overlay_lists_every_key() {
    let b = render(120, 36, View::Command, &Glyphs::unicode(), true, "enterprise");
    let text = b.rows_text().join("\n");
    for needle in [
        "COMMAND REFERENCE",
        "Q  ESC",
        "SPACE",
        "TAB",
        "TOGGLE THIS REFERENCE",
        "UNICODE 24-BIT 1000MS",
    ] {
        assert!(text.contains(needle), "{needle}");
    }
}

#[test]
fn compact_view_is_simplified_not_squeezed() {
    let b = render(80, 24, View::Command, &Glyphs::unicode(), false, "enterprise");
    let text = b.rows_text().join("\n");
    assert!(text.contains("12G USED"));
    assert!(text.contains("146 KB/S"));
    assert!(
        !text.contains("PEAK"),
        "the PEAK/CORES column is a large-terminal extra"
    );
    assert!(
        !text.contains("INTERVAL"),
        "the sampling readout is a large-terminal extra"
    );
    assert!(b.rows_text().iter().all(|r| r.chars().count() <= 80));
}

#[test]
fn ascii_mode_emits_only_ascii() {
    for view in [View::Command, View::Diagnostic] {
        for (w, h) in [(120, 36), (80, 24), (40, 10)] {
            let b = render(w, h, view, &Glyphs::ascii(), true, "enterprise");
            for row in b.rows_text() {
                assert!(row.is_ascii(), "{view:?} {w}x{h}: {row:?}");
            }
        }
    }
}

#[test]
fn every_size_renders_without_panicking_or_overflowing() {
    let g = Glyphs::unicode();
    let long_host = "a-really-quite-unreasonably-long-hostname-for-a-starship.example.internal";
    for view in [View::Command, View::Diagnostic] {
        for w in [
            0usize, 1, 2, 5, 10, 19, 20, 39, 40, 59, 60, 79, 80, 99, 100, 119, 120, 160, 250,
        ] {
            for h in [0usize, 1, 2, 3, 5, 8, 12, 15, 16, 23, 24, 29, 30, 36, 50, 80] {
                for help in [false, true] {
                    let b = render(w, h, view, &g, help, long_host);
                    assert_eq!(b.width(), w);
                    assert_eq!(b.height(), h);
                    for row in b.rows_text() {
                        assert!(lcars::term::buffer::str_width(&row) <= w, "{view:?} {w}x{h}: {row:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn unavailable_metrics_render_as_unavailable_states() {
    let s = Sampler::new(CounterWidth::Bits64);
    let t = s.telemetry().clone();
    let i = info("void");
    let c = clock();
    let scene = Scene {
        telemetry: &t,
        info: &i,
        clock: &c,
        view: View::Command,
        help: false,
        paused: true,
        frame: 0,
        color_mode: ColorMode::Mono,
        interval_ms: 500,
        warnings: &["/proc/stat: permission denied".into()],
    };
    let mut b = Buffer::new(120, 36);
    ui::draw(&mut b, &Glyphs::unicode(), &scene);
    let text = b.rows_text().join("\n");
    assert!(text.contains("CALIBRATING"));
    assert!(text.contains("HOLD"));
    assert!(text.contains("MEMORY ── UNAVAILABLE"));
    assert!(text.contains("DISK ── UNAVAILABLE"));
    assert!(text.contains("RX ──"));
    let mut d = Buffer::new(120, 36);
    ui::draw(
        &mut d,
        &Glyphs::unicode(),
        &Scene {
            view: View::Diagnostic,
            ..scene
        },
    );
    let text = d.rows_text().join("\n");
    assert!(text.contains("NO SWAP"));
    assert!(text.contains("NO PROCESS DATA"));
    assert!(text.contains("PER-CORE ── UNAVAILABLE"));
}

#[test]
fn diff_output_is_small_for_a_clock_tick() {
    let a = render(120, 36, View::Command, &Glyphs::unicode(), false, "enterprise");
    let t = synthetic_telemetry();
    let i = info("enterprise");
    let c = Clock { second: 8, ..clock() };
    let scene = Scene {
        telemetry: &t,
        info: &i,
        clock: &c,
        view: View::Command,
        help: false,
        paused: false,
        frame: 7,
        color_mode: ColorMode::TrueColor,
        interval_ms: 1000,
        warnings: &[],
    };
    let mut b = Buffer::new(120, 36);
    ui::draw(&mut b, &Glyphs::unicode(), &scene);
    let mut out = String::new();
    let cells = lcars::term::render::diff(Some(&a), &b, ColorMode::TrueColor, false, &mut out);
    assert!(cells < 60, "a seconds tick rewrote {cells} cells");
    assert!(out.len() < 2000, "{} bytes", out.len());
}
