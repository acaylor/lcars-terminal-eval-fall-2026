use std::fs;
use std::mem::MaybeUninit;
use std::time::{Duration, Instant};

use crate::canvas::Canvas;
use crate::collector::create_platform_collector;
use crate::model::format_bytes;
use crate::sampler::TelemetrySampler;
use crate::theme::{ColorMode, GlyphMode};
use crate::ui::{analyze_visual_acceptance, render_ui, UiState, ViewMode};

/// Runs a reproducible self-benchmark of startup time, collector overhead,
/// rendering latency, diff output size, CPU time, peak RSS, and binary size.
pub fn run_benchmark(iterations: usize) -> String {
    let iters = iterations.clamp(10, 500);

    // 1. Cold startup + first frame render measurement
    let t_cold_start = Instant::now();
    let mut collector = create_platform_collector();
    let mut sampler = TelemetrySampler::new();
    let state = UiState {
        view: ViewMode::PrimaryCommand,
        show_help: false,
        paused: false,
        focus_index: 0,
        color_mode: ColorMode::TrueColor,
        glyph_mode: GlyphMode::Unicode,
        refresh_ms: 750,
    };
    let mut canvas = Canvas::new(120, 36, GlyphMode::Unicode);
    let mut diff_out = String::with_capacity(16384);

    let raw0 = collector.collect(false);
    let telem0 = sampler.ingest(raw0, Duration::ZERO);
    render_ui(&mut canvas, &telem0, &state);
    canvas.render_diff(None, ColorMode::TrueColor, &mut diff_out);
    let cold_startup_dur = t_cold_start.elapsed();
    let first_frame_bytes = diff_out.len();
    let mut prev_canvas: Option<Canvas> = Some(canvas.clone());

    // Visual acceptance verification on the 120x36 primary frame
    let acceptance = analyze_visual_acceptance(&canvas);

    // 2. Primary view loop benchmark (collector + sampler + UI render + diff)
    let cpu_before = read_process_cpu_time_us();
    let wall_before = Instant::now();
    let mut total_collect_primary = Duration::ZERO;
    let mut total_render_primary = Duration::ZERO;
    let mut total_diff_bytes = 0usize;

    for _ in 0..iters {
        let tc = Instant::now();
        let raw = collector.collect(false);
        let telem = sampler.ingest(raw, Duration::from_millis(750));
        total_collect_primary += tc.elapsed();

        let tr = Instant::now();
        render_ui(&mut canvas, &telem, &state);
        canvas.render_diff(prev_canvas.as_ref(), ColorMode::TrueColor, &mut diff_out);
        total_render_primary += tr.elapsed();

        total_diff_bytes += diff_out.len();
        prev_canvas = Some(canvas.clone());
    }
    let wall_elapsed = wall_before.elapsed();
    let cpu_after = read_process_cpu_time_us();

    // 3. Secondary view (with process table) collection benchmark
    let proc_iters = (iters / 5).clamp(5, 50);
    let t_proc_start = Instant::now();
    for _ in 0..proc_iters {
        let raw = collector.collect(true);
        let _ = sampler.ingest(raw, Duration::from_millis(750));
    }
    let avg_proc_collect = t_proc_start.elapsed() / (proc_iters as u32);

    let avg_collect_primary = total_collect_primary / (iters as u32);
    let avg_render_primary = total_render_primary / (iters as u32);
    let avg_frame_total = avg_collect_primary + avg_render_primary;
    let avg_diff_bytes = total_diff_bytes / iters;

    let cpu_used_us = cpu_after.saturating_sub(cpu_before);
    let cpu_per_frame_us = cpu_used_us as f64 / iters as f64;
    // At default 750ms refresh interval (1.333 Hz), duty cycle = (cpu_per_frame_ms / 750ms) * 100%
    let estimated_idle_cpu_pct = ((cpu_per_frame_us / 1000.0) / 750.0) * 100.0;

    let peak_rss = read_peak_rss_bytes();
    let exe_size = std::env::current_exe()
        .ok()
        .and_then(|p| fs::metadata(p).ok())
        .map(|m| m.len());

    format!(
        "=== LCARS-MON PERFORMANCE & ACCEPTANCE REPORT ({iters} frames @ 120x36) ===\n\
         Cold Startup + First Frame : {:>8.3} ms  (target: < 250.0 ms)\n\
         Primary Collect + Sample   : {:>8.3} ms / frame\n\
         Secondary Collect (w/Procs): {:>8.3} ms / frame\n\
         120x36 UI Render + Diff    : {:>8.3} ms / frame\n\
         Total Frame Time (Primary) : {:>8.3} ms / frame\n\
         Initial Full Frame Output  : {:>8} bytes\n\
         Incremental Diff Output    : {:>8} bytes / frame\n\
         Process CPU Time ({iters} f)   : {:>8.3} ms  (wall: {:.3} ms)\n\
         Est. CPU @ 750ms Refresh   : {:>8.4} % of 1 core\n\
         Peak Memory (RSS)          : {:>12} ({} bytes)\n\
         Executable Binary Size     : {:>12}\n\
         --- Visual Acceptance Checks (120x36 Primary View) ---\n\
         1. Asymmetric Silhouette   : {} (left={} vs right={} structural cells)\n\
         2. Dominant Focal Element  : {}\n\
         3. Negative Space >= 20%   : {} ({:.1}% negative space)\n\
         4. Recognizable No-Text    : {} ({} structural glyphs preserved)\n\
         5. Process List Absent     : {}\n\
         Overall Acceptance Status  : {}\n",
        cold_startup_dur.as_secs_f64() * 1000.0,
        avg_collect_primary.as_secs_f64() * 1000.0,
        avg_proc_collect.as_secs_f64() * 1000.0,
        avg_render_primary.as_secs_f64() * 1000.0,
        avg_frame_total.as_secs_f64() * 1000.0,
        first_frame_bytes,
        avg_diff_bytes,
        cpu_used_us as f64 / 1000.0,
        wall_elapsed.as_secs_f64() * 1000.0,
        estimated_idle_cpu_pct,
        format_bytes(peak_rss),
        peak_rss,
        exe_size
            .map(format_bytes)
            .unwrap_or_else(|| "unknown".to_string()),
        if acceptance.is_asymmetric { "PASS" } else { "FAIL" },
        acceptance.left_structural_cells,
        acceptance.right_structural_cells,
        if acceptance.has_dominant_focal_element {
            "PASS"
        } else {
            "FAIL"
        },
        if acceptance.has_sufficient_negative_space {
            "PASS"
        } else {
            "FAIL"
        },
        acceptance.negative_space_ratio * 100.0,
        if acceptance.recognizable_without_text {
            "PASS"
        } else {
            "FAIL"
        },
        acceptance.structural_cells_after_text_strip,
        if acceptance.process_list_absent_on_primary {
            "PASS"
        } else {
            "FAIL"
        },
        if acceptance.all_passed() {
            "ALL PASSED"
        } else {
            "CHECK FAILED"
        },
    )
}

fn read_process_cpu_time_us() -> u64 {
    unsafe {
        let mut usage = MaybeUninit::<libc::rusage>::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) != 0 {
            return 0;
        }
        let u = usage.assume_init();
        let user_us = (u.ru_utime.tv_sec as u64)
            .saturating_mul(1_000_000)
            .saturating_add(u.ru_utime.tv_usec as u64);
        let sys_us = (u.ru_stime.tv_sec as u64)
            .saturating_mul(1_000_000)
            .saturating_add(u.ru_stime.tv_usec as u64);
        user_us.saturating_add(sys_us)
    }
}

fn read_peak_rss_bytes() -> u64 {
    unsafe {
        let mut usage = MaybeUninit::<libc::rusage>::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) != 0 {
            return 0;
        }
        let u = usage.assume_init();
        let raw = u.ru_maxrss.max(0) as u64;
        // On macOS `ru_maxrss` is reported in bytes; on Linux it is reported in KiB.
        #[cfg(target_os = "macos")]
        {
            raw
        }
        #[cfg(not(target_os = "macos"))]
        {
            raw.saturating_mul(1024)
        }
    }
}
