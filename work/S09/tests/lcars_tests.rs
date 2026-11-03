use std::collections::BTreeMap;
use std::time::Duration;

use lcars_mon::canvas::Canvas;
use lcars_mon::collector::linux::{
    parse_os_release, parse_proc_meminfo, parse_proc_net_dev, parse_proc_pid_stat, parse_proc_stat,
    parse_proc_uptime,
};
use lcars_mon::collector::macos::{
    compute_macos_vm_memory, extract_c_char_slice, parse_mach_cpu_load_info,
};
use lcars_mon::model::{
     truncate_chars, CpuTimes, LocalClock, RawNetBytes, RawProcessSample, RawSystemSample,
    SystemTelemetry,
};
use lcars_mon::sampler::{counter_delta, TelemetrySampler};
use lcars_mon::terminal::{decode_input_bytes, InputEvent};
use lcars_mon::theme::{ColorMode, GlyphMode};
use lcars_mon::ui::{analyze_visual_acceptance, render_ui, UiState, ViewMode};

fn sample_fixture_telemetry() -> SystemTelemetry {
    let mut sampler = TelemetrySampler::new();
    let mut net1 = BTreeMap::new();
    net1.insert(
        "eth0".to_string(),
        RawNetBytes {
            rx_bytes: 1_000_000,
            tx_bytes: 500_000,
        },
    );
    let s1 = RawSystemSample {
        clock: LocalClock {
            year: 2026,
            month: 10,
            day: 20,
            hour: 14,
            minute: 32,
            second: 9,
            day_of_year: 293,
        },
        hostname: Some("uss-enterprise-ncc-1701-d".to_string()),
        os_name: Some("Starfleet OS 47.4".to_string()),
        arch: Some("x86_64".to_string()),
        uptime_secs: Some(93_784),
        cpu_total: Some(CpuTimes {
            active: 1000,
            total: 4000,
        }),
        cpu_cores: vec![
            CpuTimes {
                active: 250,
                total: 1000,
            },
            CpuTimes {
                active: 300,
                total: 1000,
            },
            CpuTimes {
                active: 200,
                total: 1000,
            },
            CpuTimes {
                active: 250,
                total: 1000,
            },
        ],
        load_avg: Some([1.42, 1.18, 0.94]),
        mem_total_bytes: Some(32 * 1024 * 1024 * 1024),
        mem_avail_bytes: Some(20 * 1024 * 1024 * 1024),
        swap_total_bytes: Some(8 * 1024 * 1024 * 1024),
        swap_used_bytes: Some(512 * 1024 * 1024),
        disk_root_total_bytes: Some(512 * 1024 * 1024 * 1024),
        disk_root_used_bytes: Some(210 * 1024 * 1024 * 1024),
        disk_root_avail_bytes: Some(302 * 1024 * 1024 * 1024),
        net_interfaces: net1,
        process_count: Some(342),
        processes: vec![
            RawProcessSample {
                pid: 101,
                name: "warp-field-controller".to_string(),
                cpu_time_ns: 1_000_000_000,
                memory_bytes: 512 * 1024 * 1024,
                start_uptime_secs: Some(10.0),
            },
            RawProcessSample {
                pid: 202,
                name: "subspace-transceiver".to_string(),
                cpu_time_ns: 500_000_000,
                memory_bytes: 256 * 1024 * 1024,
                start_uptime_secs: Some(20.0),
            },
        ],
    };
    let _ = sampler.ingest(s1.clone(), Duration::ZERO);

    let mut net2 = BTreeMap::new();
    net2.insert(
        "eth0".to_string(),
        RawNetBytes {
            rx_bytes: 1_512_000,
            tx_bytes: 628_000,
        },
    );
    let mut s2 = s1;
    s2.cpu_total = Some(CpuTimes {
        active: 1340,
        total: 5000,
    });
    s2.cpu_cores = vec![
        CpuTimes {
            active: 350,
            total: 1250,
        },
        CpuTimes {
            active: 425,
            total: 1250,
        },
        CpuTimes {
            active: 250,
            total: 1250,
        },
        CpuTimes {
            active: 315,
            total: 1250,
        },
    ];
    s2.net_interfaces = net2;
    s2.processes[0].cpu_time_ns = 1_450_000_000;
    s2.processes[1].cpu_time_ns = 620_000_000;

    sampler.ingest(s2, Duration::from_secs(1))
}

#[test]
fn test_linux_proc_stat_parsing_and_cpu_deltas() {
    let raw_stat = "\
cpu  1000 200 300 8000 500 50 50 0 0 0
cpu0 500 100 150 4000 250 25 25 0 0 0
cpu1 500 100 150 4000 250 25 25 0 0 0
intr 123456
";
    let (overall, cores) = parse_proc_stat(raw_stat);
    let ov = overall.expect("overall cpu present");
    // active = 1000 + 200 + 300 + 50 + 50 = 1600; total = 1600 + 8000 + 500 = 10100
    assert_eq!(ov.active, 1600);
    assert_eq!(ov.total, 10100);
    assert_eq!(cores.len(), 2);

    let next_ov = CpuTimes {
        active: 1850,
        total: 11100,
    };
    let pct = next_ov.utilization_delta(&ov).expect("valid delta");
    assert!((pct - 25.0).abs() < 0.01);

    // Counter rollover / reset (`curr < prev`) returns None instead of panicking or fabricating
    let reset_ov = CpuTimes {
        active: 100,
        total: 500,
    };
    assert_eq!(reset_ov.utilization_delta(&ov), None);
}

#[test]
fn test_linux_proc_meminfo_and_fallback_calculation() {
    let meminfo = "\
MemTotal:       16384000 kB
MemFree:         2048000 kB
MemAvailable:    8192000 kB
Buffers:          512000 kB
Cached:          4096000 kB
SwapTotal:       4096000 kB
SwapFree:        3072000 kB
";
    let (tot, avail, sw_tot, sw_used) = parse_proc_meminfo(meminfo);
    assert_eq!(tot, Some(16384000 * 1024));
    assert_eq!(avail, Some(8192000 * 1024));
    assert_eq!(sw_tot, Some(4096000 * 1024));
    assert_eq!(sw_used, Some(1024000 * 1024));

    // Legacy kernel fallback without MemAvailable
    let legacy = "\
MemTotal:       8000000 kB
MemFree:        1000000 kB
Buffers:         500000 kB
Cached:         2000000 kB
SReclaimable:    200000 kB
Shmem:           100000 kB
";
    let (_, legacy_avail, _, _) = parse_proc_meminfo(legacy);
    assert_eq!(legacy_avail, Some(3_600_000 * 1024));
}

#[test]
fn test_linux_proc_pid_stat_with_parens_spaces_and_utf8() {
    let line = "4242 (Web Content (🚀 α-7)) R 1 4242 4242 0 -1 4194304 1200 0 0 0 300 100 0 0 20 0 8 0 5000 123456789 2500 18446744073709551615";
    let parsed = parse_proc_pid_stat(line, 100, 4096).expect("parsed tricky stat line");
    assert_eq!(parsed.pid, 4242);
    assert_eq!(parsed.name, "Web Content (🚀 α-7)");
    // (300 + 100) ticks @ 100 Hz = 4.0 seconds = 4_000_000_000 ns
    assert_eq!(parsed.cpu_time_ns, 4_000_000_000);
    assert_eq!(parsed.memory_bytes, 2500 * 4096);
    assert_eq!(parsed.start_uptime_secs, Some(50.0));
}

#[test]
fn test_linux_uptime_and_os_release_parsers() {
    assert_eq!(parse_proc_uptime("12345.67 98765.43\n"), Some(12345));
    assert_eq!(parse_proc_uptime("invalid"), None);

    let os_rel = "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 26.04.1 LTS\"\n";
    assert_eq!(
        parse_os_release(os_rel),
        Some("Ubuntu 26.04.1 LTS".to_string())
    );

    let net_dev = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 9999999   12345    0    0    0     0          0         0  9999999   12345    0    0    0     0       0          0
  eth0: 1234567    2000    0    0    0     0          0         0  7654321    1500    0    0    0     0       0          0
";
    let parsed_net = parse_proc_net_dev(net_dev);
    assert_eq!(parsed_net.len(), 1);
    assert_eq!(
        parsed_net.get("eth0"),
        Some(&RawNetBytes {
            rx_bytes: 1234567,
            tx_bytes: 7654321
        })
    );
}

#[test]
fn test_macos_collector_parsers_and_vm_math() {
    // 2 cores: [user, sys, idle, nice]
    let mach_ticks = [100i32, 50, 850, 0, 200, 100, 700, 0];
    let (ov, cores) = parse_mach_cpu_load_info(&mach_ticks);
    assert_eq!(cores.len(), 2);
    assert_eq!(cores[0], CpuTimes { active: 150, total: 1000 });
    assert_eq!(cores[1], CpuTimes { active: 300, total: 1000 });
    assert_eq!(ov, Some(CpuTimes { active: 450, total: 2000 }));

    let (tot, avail) = compute_macos_vm_memory(16 * 1024 * 1024 * 1024, 16384, 100_000, 50_000, 10_000);
    assert_eq!(tot, 16 * 1024 * 1024 * 1024);
    assert_eq!(avail, 160_000 * 16384);

    let c_buf = b"WindowServer\0ignored";
    assert_eq!(extract_c_char_slice(c_buf), "WindowServer");
}

#[test]
fn test_network_rates_interface_hotplug_and_rollover() {
    let mut sampler = TelemetrySampler::new();

    let mut s1 = RawSystemSample::default();
    s1.net_interfaces.insert(
        "en0".to_string(),
        RawNetBytes {
            rx_bytes: 10_000,
            tx_bytes: 5_000,
        },
    );
    let t1 = sampler.ingest(s1, Duration::ZERO);
    assert_eq!(t1.network.unwrap().rx_bytes_per_sec, 0.0);

    // 500ms elapsed: +5,000 RX bytes -> 10,000 B/s; +2,500 TX bytes -> 5,000 B/s.
    // Also a new interface `utun0` appears with 999,999,999 bytes (must not spike rate!).
    let mut s2 = RawSystemSample::default();
    s2.net_interfaces.insert(
        "en0".to_string(),
        RawNetBytes {
            rx_bytes: 15_000,
            tx_bytes: 7_500,
        },
    );
    s2.net_interfaces.insert(
        "utun0".to_string(),
        RawNetBytes {
            rx_bytes: 999_999_999,
            tx_bytes: 888_888_888,
        },
    );
    let t2 = sampler.ingest(s2, Duration::from_millis(500));
    let net2 = t2.network.expect("network present");
    assert!((net2.rx_bytes_per_sec - 10_000.0).abs() < 1.0);
    assert!((net2.tx_bytes_per_sec - 5_000.0).abs() < 1.0);
    assert_eq!(net2.active_interfaces, 2);

    // Next sample: `utun0` disappears; `en0` advances by +2,000 RX over 2.0 seconds -> 1,000 B/s
    let mut s3 = RawSystemSample::default();
    s3.net_interfaces.insert(
        "en0".to_string(),
        RawNetBytes {
            rx_bytes: 17_000,
            tx_bytes: 9_500,
        },
    );
    let t3 = sampler.ingest(s3.clone(), Duration::from_secs(2));
    let net3 = t3.network.expect("network present");
    assert!((net3.rx_bytes_per_sec - 1_000.0).abs() < 1.0);
    assert_eq!(net3.active_interfaces, 1);

    // Zero duration or huge duration (> 120s suspend) must not produce NaN or Inf
    let t_zero = sampler.ingest(s3.clone(), Duration::ZERO);
    assert_eq!(t_zero.network.unwrap().rx_bytes_per_sec, 0.0);
    let t_huge = sampler.ingest(s3, Duration::from_secs(600));
    assert_eq!(t_huge.network.unwrap().rx_bytes_per_sec, 0.0);

    // 32-bit rollover check
    let prev32 = (u32::MAX - 999) as u64;
    let curr32 = 500u64;
    assert_eq!(counter_delta(prev32, curr32), 1500);
    // 64-bit reset check
    assert_eq!(counter_delta(10_000_000_000, 100), 0);
}

#[test]
fn test_process_disappearance_and_top5_sorting() {
    let mut sampler = TelemetrySampler::new();
    let mut s1 = RawSystemSample {
        cpu_cores: vec![CpuTimes { active: 10, total: 100 }; 4],
        ..RawSystemSample::default()
    };
    for pid in 1..=8 {
        s1.processes.push(RawProcessSample {
            pid,
            name: format!("proc-{}", pid),
            cpu_time_ns: 1_000_000_000,
            memory_bytes: (pid as u64) * 1024 * 1024,
            start_uptime_secs: Some(1.0),
        });
    }
    let _ = sampler.ingest(s1, Duration::ZERO);

    // In sample 2, PID 8 disappears, and PIDs 1..=7 advance by `pid * 100ms` over 1s
    let mut s2 = RawSystemSample {
        cpu_cores: vec![CpuTimes { active: 20, total: 200 }; 4],
        ..RawSystemSample::default()
    };
    for pid in 1..=7 {
        s2.processes.push(RawProcessSample {
            pid,
            name: format!("proc-{}", pid),
            cpu_time_ns: 1_000_000_000 + (pid as u64) * 100_000_000,
            memory_bytes: (pid as u64) * 1024 * 1024,
            start_uptime_secs: Some(1.0),
        });
    }
    let telem = sampler.ingest(s2, Duration::from_secs(1));
    assert_eq!(telem.top_processes.len(), 5);
    assert_eq!(telem.top_processes[0].pid, 7);
    assert!((telem.top_processes[0].cpu_pct - 70.0).abs() < 0.1);
    assert_eq!(telem.top_processes[4].pid, 3);
}

#[test]
fn test_visual_acceptance_120x36_primary_and_secondary() {
    let telem = sample_fixture_telemetry();
    let mut state = UiState::default();

    // Primary View at 120x36
    let mut primary_canvas = Canvas::new(120, 36, GlyphMode::Unicode);
    render_ui(&mut primary_canvas, &telem, &state);
    let report = analyze_visual_acceptance(&primary_canvas);
    assert!(report.is_asymmetric, "Primary silhouette must be asymmetric");
    assert!(
        report.has_dominant_focal_element,
        "Primary view must have a dominant focal element"
    );
    assert!(
        report.has_sufficient_negative_space,
        "Primary view must have >= 20% negative space (got {:.1}%)",
        report.negative_space_ratio * 100.0
    );
    assert!(
        report.recognizable_without_text,
        "Primary view must remain recognizable when all text is replaced by blanks"
    );
    assert!(
        report.process_list_absent_on_primary,
        "Process list must not appear on primary view"
    );
    assert!(report.all_passed());

    let primary_plain = primary_canvas.render_plain();
    assert!(primary_plain.contains("EARTH 2026.10.20"));
    assert!(primary_plain.contains("uss-enterprise"));
    assert!(primary_plain.contains("MEM USED"));
    assert!(primary_plain.contains("ROOTFS /"));
    assert!(primary_plain.contains("NET RX"));
    assert!(!primary_plain.contains("warp-field-controller"));

    // Secondary View at 120x36
    state.view = ViewMode::ProcessDiagnostics;
    let mut secondary_canvas = Canvas::new(120, 36, GlyphMode::Unicode);
    render_ui(&mut secondary_canvas, &telem, &state);
    let sec_plain = secondary_canvas.render_plain();
    assert!(sec_plain.contains("Starfleet OS 47.4"));
    assert!(sec_plain.contains("x86_64"));
    assert!(sec_plain.contains("342 ACTIVE TASKS"));
    assert!(sec_plain.contains("SWAP"));
    assert!(sec_plain.contains("C00"));
    assert!(sec_plain.contains("warp-field-controller"));
    assert!(sec_plain.contains("subspace-transceiver"));
}

#[test]
fn test_unsupported_metrics_render_graceful_unavailable_states() {
    let empty_telem = SystemTelemetry::default();
    let mut state = UiState::default();

    let mut c1 = Canvas::new(120, 36, GlyphMode::Unicode);
    render_ui(&mut c1, &empty_telem, &state);
    let p1 = c1.render_plain();
    assert!(p1.contains("OFFLINE") || p1.contains("UNAVAILABLE"));

    state.view = ViewMode::ProcessDiagnostics;
    let mut c2 = Canvas::new(120, 36, GlyphMode::Unicode);
    render_ui(&mut c2, &empty_telem, &state);
    let p2 = c2.render_plain();
    assert!(p2.contains("OFFLINE") || p2.contains("UNAVAILABLE"));
}

#[test]
fn test_responsive_layouts_and_extreme_sizes_never_panic() {
    let mut telem = sample_fixture_telemetry();
    telem.hostname = Some("a-very-long-starship-hostname-that-exceeds-any-normal-column-budget-ncc-1701-enterprise".to_string());
    let mut state = UiState::default();

    for &(w, h) in &[(0, 0), (1, 1), (5, 2), (20, 8), (45, 13), (80, 24), (100, 30), (120, 36), (180, 50)] {
        for view in [ViewMode::PrimaryCommand, ViewMode::ProcessDiagnostics] {
            for help in [false, true] {
                state.view = view;
                state.show_help = help;
                let mut canvas = Canvas::new(w, h, GlyphMode::Unicode);
                render_ui(&mut canvas, &telem, &state);
                let _ = canvas.render_plain();
            }
        }
    }

    // Verify 80x24 simplified command display maintains generous negative space
    state.view = ViewMode::PrimaryCommand;
    state.show_help = false;
    let mut c80 = Canvas::new(80, 24, GlyphMode::Unicode);
    render_ui(&mut c80, &telem, &state);
    let rep80 = analyze_visual_acceptance(&c80);
    assert!(rep80.negative_space_ratio >= 0.25);
}

#[test]
fn test_ascii_fallback_and_color_modes() {
    let telem = sample_fixture_telemetry();
    let state = UiState {
        glyph_mode: GlyphMode::Ascii,
        ..UiState::default()
    };

    let mut canvas = Canvas::new(120, 36, GlyphMode::Ascii);
    render_ui(&mut canvas, &telem, &state);
    let plain_ascii = canvas.render_plain();
    assert!(
        plain_ascii.is_ascii(),
        "ASCII fallback mode must emit only 7-bit ASCII characters"
    );

    // Verify all 4 color modes produce valid output
    for mode in [
        ColorMode::TrueColor,
        ColorMode::Ansi256,
        ColorMode::Ansi16,
        ColorMode::Monochrome,
    ] {
        let snap = canvas.render_ansi_snapshot(mode);
        if mode == ColorMode::TrueColor {
            assert!(snap.contains("\x1b[0;38;2;"));
        } else if mode == ColorMode::Ansi256 {
            assert!(snap.contains("\x1b[0;38;5;"));
        } else if mode == ColorMode::Monochrome {
            assert!(!snap.contains(";38;"));
        }
    }
}

#[test]
fn test_cell_diff_renderer_eliminates_redundant_repaint() {
    let telem = sample_fixture_telemetry();
    let state = UiState::default();
    let mut c1 = Canvas::new(120, 36, GlyphMode::Unicode);
    render_ui(&mut c1, &telem, &state);

    let mut out = String::new();
    c1.render_diff(None, ColorMode::TrueColor, &mut out);
    assert!(out.len() > 5000);
    assert!(!out.contains("\x1b[2J"), "Diff renderer must not clear screen");

    // Rendering an identical frame against `c1` should emit only the synchronized update wrapper
    let mut out2 = String::new();
    c1.render_diff(Some(&c1), ColorMode::TrueColor, &mut out2);
    assert_eq!(out2, "\x1b[?2026h\x1b[0m\x1b[?2026l");
}

#[test]
fn test_input_decoding_and_utf8_truncation() {
    assert_eq!(decode_input_bytes(b"q"), InputEvent::Quit);
    assert_eq!(decode_input_bytes(b"\x1b"), InputEvent::Quit);
    assert_eq!(decode_input_bytes(b"\x1b[A"), InputEvent::None);
    assert_eq!(decode_input_bytes(b"?"), InputEvent::ToggleHelp);
    assert_eq!(decode_input_bytes(b" "), InputEvent::TogglePause);
    assert_eq!(decode_input_bytes(b"p"), InputEvent::ToggleProcesses);
    assert_eq!(decode_input_bytes(b"\t"), InputEvent::CycleViewOrFocus);

    let utf8_name = "スターフリート・エンジニアリング";
    let t_uni = truncate_chars(utf8_name, 6, true);
    assert_eq!(t_uni.chars().count(), 6);
    assert!(t_uni.ends_with('…'));

    let t_asc = truncate_chars(utf8_name, 6, false);
    assert_eq!(t_asc.chars().count(), 6);
    assert!(t_asc.ends_with(".."));
}
