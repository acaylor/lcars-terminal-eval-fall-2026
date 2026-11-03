use lcars_monitor::buffer::{Buffer, DiffRenderer, Tone};
use lcars_monitor::model::{Disk, Memory, NetworkRates, Process, Snapshot, Swap};
use lcars_monitor::render::{RenderOptions, View, render};
use std::time::Duration;

fn fixture() -> Snapshot {
    Snapshot {
        time: "13:37:42".into(),
        date: "2364-10-24 FRI".into(),
        hostname: "USS-ENGINEERING-VERY-LONG-HOSTNAME".into(),
        os: "Linux Test Kernel".into(),
        architecture: "X86_64".into(),
        uptime: Some(Duration::from_secs(4 * 86_400 + 3 * 3_600 + 2 * 60)),
        cpu: Some(37.2),
        cores: (0..24).map(|core| (core * 7 % 101) as f32).collect(),
        load: Some([0.37, 0.42, 0.51]),
        memory: Some(Memory {
            used: 28_500_000_000,
            available: 64_900_000_000,
            total: 93_400_000_000,
        }),
        swap: Some(Swap {
            used: 1_000_000_000,
            total: 8_000_000_000,
        }),
        disk: Some(Disk {
            used: 633_000_000_000,
            available: 283_000_000_000,
            total: 916_000_000_000,
        }),
        network: Some(NetworkRates {
            receive: 1_240_000.0,
            transmit: 84_000.0,
        }),
        process_count: Some(247),
        top_processes: vec![
            Process {
                pid: 1701,
                name: "warp-core-界".into(),
                cpu: 93.4,
                memory: 4_200_000_000,
            },
            Process {
                pid: 47,
                name: "sensor-array".into(),
                cpu: 42.0,
                memory: 900_000_000,
            },
            Process {
                pid: 8,
                name: "shield-control".into(),
                cpu: 12.0,
                memory: 200_000_000,
            },
            Process {
                pid: 9,
                name: "plasma-flow".into(),
                cpu: 8.0,
                memory: 100_000_000,
            },
            Process {
                pid: 10,
                name: "holodeck".into(),
                cpu: 4.0,
                memory: 50_000_000,
            },
        ],
    }
}

fn history() -> lcars_monitor::model::History {
    let mut history = lcars_monitor::model::History::default();
    for value in [5.0, 12.0, 8.0, 30.0, 24.0, 42.0, 37.0, 55.0, 32.0, 18.0] {
        history.push(value);
    }
    history
}

#[test]
fn primary_has_required_metrics_but_no_process_list() {
    let buffer = render(
        &fixture(),
        &history(),
        View::Primary,
        120,
        36,
        RenderOptions::default(),
    );
    let text = buffer.plain_text();
    for required in [
        "2364-10-24",
        "HOST USS-ENGINEERING",
        "CPU / TOTAL",
        "LOAD 01/05/15",
        "MEMORY",
        "USED",
        "FREE",
        "TOTAL",
        "ROOT /",
        "NETWORK",
        "RX",
        "TX",
        "UP 4d 03h 02m",
    ] {
        assert!(text.contains(required), "missing {required:?}\n{text}");
    }
    assert!(!text.contains("TOP CPU PROCESSES"));
    assert!(!text.contains("warp-core"));
}

#[test]
fn primary_composition_is_asymmetric_and_preserves_negative_space() {
    let buffer = render(
        &fixture(),
        &history(),
        View::Primary,
        120,
        36,
        RenderOptions::default(),
    );
    let blank_space = (0..buffer.height())
        .flat_map(|y| (0..buffer.width()).map(move |x| (x, y)))
        .filter(|(x, y)| {
            buffer.get(*x, *y).is_some_and(|cell| {
                cell.bg == Tone::Background && (cell.glyph == ' ' || cell.glyph == '\0')
            })
        })
        .count();
    assert!(
        blank_space * 100 / (120 * 36) >= 20,
        "negative space was only {}%",
        blank_space * 100 / (120 * 36)
    );
    assert!(buffer.count_background(Tone::Orange) > 150);
    assert!(buffer.count_background(Tone::Lavender) > 30);
    let half_blocks = buffer
        .plain_text()
        .chars()
        .filter(|character| matches!(character, '▀' | '▄'))
        .count();
    assert!(
        half_blocks > 20,
        "the focal clock must dominate the composition"
    );
}

#[test]
fn secondary_contains_diagnostics_and_five_processes() {
    let buffer = render(
        &fixture(),
        &history(),
        View::Diagnostics,
        120,
        36,
        RenderOptions::default(),
    );
    let text = buffer.plain_text();
    for required in [
        "SYSTEM DIAGNOSTICS",
        "Linux Test Kernel / X86_64",
        "247 PROCESSES",
        "PER-CORE UTILIZATION",
        "C00",
        "C23",
        "TOP CPU PROCESSES",
        "warp-core-界",
        "1701",
        "93.4%",
        "SWAP",
    ] {
        assert!(text.contains(required), "missing {required:?}\n{text}");
    }
    for name in [
        "warp-core-界",
        "sensor-array",
        "shield-control",
        "plasma-flow",
        "holodeck",
    ] {
        assert!(text.contains(name));
    }
}

#[test]
fn compact_80_by_24_is_deliberately_simplified() {
    let buffer = render(
        &fixture(),
        &history(),
        View::Primary,
        80,
        24,
        RenderOptions::default(),
    );
    let text = buffer.plain_text();
    assert!(text.contains("LCARS // ENGINEERING"));
    assert!(text.contains("MEMORY"));
    assert!(text.contains("ROOT /"));
    assert!(text.contains("NETWORK"));
    assert!(!text.contains("CPU WAVEFORM"));
    assert!(!text.contains("TOP CPU PROCESSES"));
}

#[test]
fn layouts_and_ascii_fallback_survive_extreme_sizes() {
    for width in [0, 1, 5, 20, 47, 48, 79, 80, 99, 100, 120, 200] {
        for height in [0, 1, 4, 11, 12, 23, 24, 29, 30, 36, 60] {
            for view in [View::Primary, View::Diagnostics] {
                for ascii in [false, true] {
                    let buffer = render(
                        &fixture(),
                        &history(),
                        view,
                        width,
                        height,
                        RenderOptions {
                            ascii,
                            ..RenderOptions::default()
                        },
                    );
                    assert_eq!(buffer.width(), width);
                    assert_eq!(buffer.height(), height);
                    if ascii {
                        assert!(
                            buffer.plain_text().is_ascii(),
                            "non-ASCII output at {width}x{height}\n{}",
                            buffer.plain_text()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn help_and_pause_are_visible_without_changing_views() {
    let buffer = render(
        &fixture(),
        &history(),
        View::Primary,
        120,
        36,
        RenderOptions {
            help: true,
            paused: true,
            ..RenderOptions::default()
        },
    );
    let text = buffer.plain_text();
    assert!(text.contains("COMMAND REFERENCE"));
    assert!(text.contains("Pause or resume sampling"));
    assert!(!text.contains("TOP CPU PROCESSES"));
}

#[test]
fn diff_renderer_skips_unchanged_cells() {
    let mut renderer = DiffRenderer::new(lcars_monitor::buffer::ColorMode::Mono);
    let buffer = Buffer::new(12, 3);
    let mut output = Vec::new();
    renderer.draw(&mut output, &buffer).unwrap();
    let initial_size = output.len();
    output.clear();
    renderer.draw(&mut output, &buffer).unwrap();
    assert!(output.len() < initial_size);
    assert!(
        !String::from_utf8(output.clone())
            .unwrap()
            .contains("\u{1b}[1;1H")
    );
    output.clear();
    let mut changed = buffer;
    changed.set(3, 1, 'X', Tone::Amber, Tone::Background);
    renderer.draw(&mut output, &changed).unwrap();
    let ansi = String::from_utf8(output).unwrap();
    assert!(ansi.contains('X'));
    assert!(ansi.contains("\u{1b}[2;4H"));
}
