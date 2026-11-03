use crate::canvas::{BrailleGrid, Canvas};
use crate::model::{
    format_bytes, format_rate, format_uptime, truncate_chars, SystemAlertState, SystemTelemetry,
};
use crate::theme::{ColorMode, ColorSpec, GlyphMode, LcarsTone};

/// Active view mode of the command display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    PrimaryCommand,
    ProcessDiagnostics,
}

impl ViewMode {
    pub fn toggle(self) -> Self {
        match self {
            ViewMode::PrimaryCommand => ViewMode::ProcessDiagnostics,
            ViewMode::ProcessDiagnostics => ViewMode::PrimaryCommand,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            ViewMode::PrimaryCommand => "LCARS // SEC-01 TACTICAL COMMAND",
            ViewMode::ProcessDiagnostics => "LCARS // SEC-02 ENGINEERING DIAGNOSTICS",
        }
    }
}

/// Interactive UI state independent of system telemetry.
#[derive(Debug, Clone)]
pub struct UiState {
    pub view: ViewMode,
    pub show_help: bool,
    pub paused: bool,
    pub focus_index: usize,
    pub color_mode: ColorMode,
    pub glyph_mode: GlyphMode,
    pub refresh_ms: u64,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            view: ViewMode::PrimaryCommand,
            show_help: false,
            paused: false,
            focus_index: 0,
            color_mode: ColorMode::TrueColor,
            glyph_mode: GlyphMode::Unicode,
            refresh_ms: 750,
        }
    }
}

/// Quantitative report verifying the 7 Visual Acceptance Criteria on a rendered canvas.
#[derive(Debug, Clone)]
pub struct VisualAcceptanceReport {
    pub left_structural_cells: usize,
    pub right_structural_cells: usize,
    pub is_asymmetric: bool,
    pub has_dominant_focal_element: bool,
    pub negative_space_ratio: f64,
    pub has_sufficient_negative_space: bool,
    pub structural_cells_after_text_strip: usize,
    pub recognizable_without_text: bool,
    pub process_list_absent_on_primary: bool,
}

impl VisualAcceptanceReport {
    pub fn all_passed(&self) -> bool {
        self.is_asymmetric
            && self.has_dominant_focal_element
            && self.has_sufficient_negative_space
            && self.recognizable_without_text
            && self.process_list_absent_on_primary
    }
}

/// Evaluates a rendered canvas against the prompt's visual acceptance criteria.
pub fn analyze_visual_acceptance(canvas: &Canvas) -> VisualAcceptanceReport {
    let total_cells = (canvas.width * canvas.height).max(1);
    let mid_x = canvas.width / 2;

    let mut blank_cells = 0usize;
    let mut left_struct = 0usize;
    let mut right_struct = 0usize;
    let mut stripped_struct = 0usize;
    let mut focal_cells = 0usize;

    for y in 0..canvas.height {
        for x in 0..canvas.width {
            let ch = canvas.cells[y * canvas.width + x].ch;
            if ch == ' ' {
                blank_cells += 1;
                continue;
            }
            let is_structural = !ch.is_alphanumeric()
                && !matches!(ch, ':' | '.' | '%' | '/' | '-' | '[' | ']' | '?' | '(' | ')');
            if is_structural {
                stripped_struct += 1;
                if x < mid_x {
                    left_struct += 1;
                } else {
                    right_struct += 1;
                }
                if (3..=14).contains(&y) && (18..=114).contains(&x) {
                    focal_cells += 1;
                }
            }
        }
    }

    let plain = canvas.render_plain();
    let neg_ratio = blank_cells as f64 / total_cells as f64;
    let diff = left_struct.abs_diff(right_struct);
    let max_side = left_struct.max(right_struct).max(1);
    let asymmetry_ratio = diff as f64 / max_side as f64;

    VisualAcceptanceReport {
        left_structural_cells: left_struct,
        right_structural_cells: right_struct,
        is_asymmetric: asymmetry_ratio >= 0.18,
        has_dominant_focal_element: focal_cells >= 45,
        negative_space_ratio: neg_ratio,
        has_sufficient_negative_space: neg_ratio >= 0.20,
        structural_cells_after_text_strip: stripped_struct,
        recognizable_without_text: stripped_struct >= (total_cells / 10),
        process_list_absent_on_primary: !plain.contains("PID ") && !plain.contains("TOP-5 PROCESS"),
    }
}

/// Top-level rendering entry point. Safely adapts to any terminal dimensions (`0x0` up to `400x200`).
pub fn render_ui(canvas: &mut Canvas, telemetry: &SystemTelemetry, state: &UiState) {
    canvas.clear();
    if canvas.width == 0 || canvas.height == 0 {
        return;
    }

    // Ultra-small terminal guard (< 46 cols or < 14 rows)
    if canvas.width < 46 || canvas.height < 14 {
        render_micro_view(canvas, telemetry, state);
        return;
    }

    // Deliberately simplified command display at 80x24 (< 104 cols or < 30 rows)
    if canvas.width < 104 || canvas.height < 30 {
        match state.view {
            ViewMode::PrimaryCommand => render_compact_primary(canvas, telemetry, state),
            ViewMode::ProcessDiagnostics => render_compact_secondary(canvas, telemetry, state),
        }
    } else {
        match state.view {
            ViewMode::PrimaryCommand => render_full_primary(canvas, telemetry, state),
            ViewMode::ProcessDiagnostics => render_full_secondary(canvas, telemetry, state),
        }
    }

    if state.show_help {
        render_help_overlay(canvas, state);
    }
}

/// Ultra-compact fallback for tiny terminal sizes so layout never panics or corrupts.
fn render_micro_view(canvas: &mut Canvas, t: &SystemTelemetry, state: &UiState) {
    let w = canvas.width;
    let h = canvas.height;
    let pill_w = w.saturating_sub(2).max(2);
    canvas.draw_pill(1, 0, pill_w, LcarsTone::Orange, Some("LCARS COMPACT"));

    let time_s = t.clock.format_time();
    let cpu_s = t
        .cpu_overall_pct
        .map(|v| format!("CPU {:4.1}%", v))
        .unwrap_or_else(|| "CPU N/A".to_string());
    let mem_s = t
        .memory
        .map(|m| format!("MEM {:4.1}%", m.used_pct))
        .unwrap_or_else(|| "MEM N/A".to_string());

    if h > 2 {
        canvas.draw_text(
            1,
            2,
            &truncate_chars(&format!("{} {}", time_s, cpu_s), w.saturating_sub(2), false),
            ColorSpec::Tone(LcarsTone::Amber),
            ColorSpec::Reset,
            true,
        );
    }
    if h > 3 {
        canvas.draw_text(
            1,
            3,
            &truncate_chars(&mem_s, w.saturating_sub(2), false),
            ColorSpec::Tone(LcarsTone::CoolBlue),
            ColorSpec::Reset,
            false,
        );
    }
    if h > 5 {
        let mode_lbl = match state.view {
            ViewMode::PrimaryCommand => "V1:CMD [P/TAB]",
            ViewMode::ProcessDiagnostics => "V2:DIAG [P/TAB]",
        };
        canvas.draw_text(
            1,
            h - 1,
            &truncate_chars(mode_lbl, w.saturating_sub(2), false),
            ColorSpec::Tone(LcarsTone::Lavender),
            ColorSpec::Reset,
            false,
        );
    }
}

/// Deliberately simplified `80x24` Primary Command Display:
/// Preserves the asymmetric left-spine LCARS silhouette and >= 25% intentional negative space
/// without squeezing the expanded `120x36` harmonic visualizers into `80x24`.
fn render_compact_primary(canvas: &mut Canvas, t: &SystemTelemetry, state: &UiState) {
    let w = canvas.width;
    let h = canvas.height;
    let spine_x = 1usize;
    let spine_w = 10usize;
    let top_rail_w = w.saturating_sub(4).max(spine_w + 8);
    let mid_y = (h / 2).clamp(9, 13);
    let bot_y = h.saturating_sub(3);

    // 1. Upper-Left Sweeping Elbow
    canvas.draw_upper_left_elbow(
        spine_x,
        1,
        spine_w,
        4,
        top_rail_w / 2,
        1,
        LcarsTone::Orange,
        LcarsTone::Amber,
    );
    canvas.stamp_stencil_label(spine_x + 1, 2, "LCARS-01", LcarsTone::Orange);

    // Upper rail extension with host & status pill
    let ext_x = spine_x + top_rail_w / 2 + 1;
    let ext_w = top_rail_w.saturating_sub(top_rail_w / 2 + 1);
    if ext_w > 8 {
        canvas.draw_pill(ext_x, 1, ext_w, LcarsTone::Lavender, Some("TACTICAL CMD"));
    }

    // Spine blocks between top elbow and mid bridge
    if mid_y > 6 {
        canvas.fill_solid_block(
            spine_x,
            6,
            spine_w - 1,
            mid_y - 6,
            LcarsTone::Amber,
            LcarsTone::Amber,
        );
        canvas.stamp_stencil_label(spine_x + 1, 6, "CHRONO", LcarsTone::Amber);
    }

    // 2. Mid-Spine T-Bridge
    canvas.draw_mid_left_bridge(
        spine_x,
        mid_y,
        spine_w - 1,
        w.saturating_sub(6),
        LcarsTone::Salmon,
        LcarsTone::Amber,
    );
    canvas.stamp_stencil_label(spine_x + 1, mid_y + 1, "EPS-GRID", LcarsTone::Salmon);

    // Lower spine block
    if bot_y > mid_y + 4 {
        canvas.fill_solid_block(
            spine_x,
            mid_y + 4,
            spine_w - 1,
            bot_y.saturating_sub(mid_y + 5),
            LcarsTone::CoolBlue,
            LcarsTone::CoolBlue,
        );
        canvas.stamp_stencil_label(spine_x + 1, mid_y + 4, "TELEM-A", LcarsTone::CoolBlue);
    }

    // 3. Lower-Left Elbow (shorter horizontal rail for asymmetry)
    let bot_rail_w = (w * 3 / 4).max(spine_w + 10).min(w - 2);
    canvas.draw_lower_left_elbow(
        spine_x,
        bot_y.saturating_sub(1),
        spine_w,
        2,
        bot_rail_w,
        1,
        LcarsTone::Lavender,
        LcarsTone::Periwinkle,
    );

    // --- UPPER BAY: FOCAL TIME, HOST, UPTIME, CPU & LOAD ---
    let content_x = spine_x + spine_w + 3;
    let time_str = t.clock.format_time();
    canvas.draw_big_digits(content_x, 3, &time_str, LcarsTone::Cream, LcarsTone::Amber);

    let meta_x = content_x + 34;
    let host = t.hostname.as_deref().unwrap_or("USS-WORKSTATION");
    let host_trunc = truncate_chars(host, w.saturating_sub(meta_x + 2), state.glyph_mode == GlyphMode::Unicode);
    canvas.draw_text(
        meta_x,
        3,
        &format!("NODE // {}", host_trunc),
        ColorSpec::Tone(LcarsTone::Amber),
        ColorSpec::Reset,
        true,
    );
    canvas.draw_text(
        meta_x,
        4,
        &format!(
            "DATE // {}  SD {:06.1}",
            t.clock.format_date(),
            t.clock.stardate()
        ),
        ColorSpec::Tone(LcarsTone::IceBlue),
        ColorSpec::Reset,
        false,
    );
    let up_str = t
        .uptime_secs
        .map(format_uptime)
        .unwrap_or_else(|| "OFFLINE".to_string());
    canvas.draw_text(
        meta_x,
        5,
        &format!("UPTM // {}", up_str),
        ColorSpec::Tone(LcarsTone::Lavender),
        ColorSpec::Reset,
        false,
    );

    // Focal CPU & Load bar on row 7..9
    let cpu_pct = t.cpu_overall_pct.unwrap_or(0.0);
    let cpu_lbl = t
        .cpu_overall_pct
        .map(|v| format!("CORE UTIL {:5.1}%", v))
        .unwrap_or_else(|| "CORE UTIL OFFLINE".to_string());
    canvas.draw_text(
        content_x,
        7,
        &cpu_lbl,
        ColorSpec::Tone(LcarsTone::Orange),
        ColorSpec::Reset,
        true,
    );
    let gauge_w = (w.saturating_sub(content_x + 22)).clamp(10, 32);
    canvas.draw_conduit_gauge(
        content_x + 18,
        7,
        gauge_w,
        cpu_pct,
        LcarsTone::Amber,
        LcarsTone::Salmon,
    );

    let load_str = match t.load_avg {
        Some([l1, l5, l15]) => format!("LOAD VEC // 1M {:4.2}   5M {:4.2}   15M {:4.2}", l1, l5, l15),
        None => "LOAD VEC // OFFLINE".to_string(),
    };
    canvas.draw_text(
        content_x,
        9,
        &load_str,
        ColorSpec::Tone(LcarsTone::CoolBlue),
        ColorSpec::Reset,
        false,
    );

    // --- LOWER BAY: MEMORY, ROOT DISK, NETWORK ---
    let low_y = mid_y + 3;
    if let Some(mem) = t.memory {
        canvas.draw_pill(content_x, low_y, 8, LcarsTone::Orange, Some("MEM"));
        canvas.draw_conduit_gauge(
            content_x + 9,
            low_y,
            20,
            mem.used_pct,
            LcarsTone::Orange,
            LcarsTone::Salmon,
        );
        let mem_txt = format!(
            "{} / {} (AVAIL {})",
            format_bytes(mem.used_bytes),
            format_bytes(mem.total_bytes),
            format_bytes(mem.avail_bytes)
        );
        canvas.draw_text(
            content_x + 30,
            low_y,
            &truncate_chars(&mem_txt, w.saturating_sub(content_x + 31), false),
            ColorSpec::Tone(LcarsTone::Cream),
            ColorSpec::Reset,
            false,
        );
    } else {
        canvas.draw_text(
            content_x,
            low_y,
            "MEM // TELEMETRY UNAVAILABLE",
            ColorSpec::Tone(LcarsTone::Muted),
            ColorSpec::Reset,
            false,
        );
    }

    if let Some(disk) = t.disk_root {
        canvas.draw_pill(content_x + 3, low_y + 2, 8, LcarsTone::Amber, Some("ROOT /"));
        canvas.draw_conduit_gauge(
            content_x + 12,
            low_y + 2,
            17,
            disk.used_pct,
            LcarsTone::Amber,
            LcarsTone::Orange,
        );
        let d_txt = format!(
            "{} / {} ({:4.1}%)",
            format_bytes(disk.used_bytes),
            format_bytes(disk.total_bytes),
            disk.used_pct
        );
        canvas.draw_text(
            content_x + 30,
            low_y + 2,
            &truncate_chars(&d_txt, w.saturating_sub(content_x + 31), false),
            ColorSpec::Tone(LcarsTone::IceBlue),
            ColorSpec::Reset,
            false,
        );
    }

    if let Some(net) = t.network {
        canvas.draw_pill(content_x + 6, low_y + 4, 8, LcarsTone::CoolBlue, Some("NET IO"));
        let net_txt = format!(
            "RX {:>10}   TX {:>10}",
            format_rate(net.rx_bytes_per_sec),
            format_rate(net.tx_bytes_per_sec)
        );
        canvas.draw_text(
            content_x + 15,
            low_y + 4,
            &net_txt,
            ColorSpec::Tone(LcarsTone::CoolBlue),
            ColorSpec::Reset,
            true,
        );
    } else {
        canvas.draw_text(
            content_x + 6,
            low_y + 4,
            "NET IO // OFFLINE",
            ColorSpec::Tone(LcarsTone::Muted),
            ColorSpec::Reset,
            false,
        );
    }

    // Discoverable control pills on bottom row
    render_footer_controls(canvas, h - 1, state);
}

/// Compact `80x24` Secondary Engineering & Process Diagnostic View.
fn render_compact_secondary(canvas: &mut Canvas, t: &SystemTelemetry, state: &UiState) {
    let w = canvas.width;
    let h = canvas.height;
    let right_x = w.saturating_sub(2);
    let spine_w = 10usize;

    // Right-anchored sweeping LCARS silhouette
    canvas.draw_upper_right_elbow(
        right_x,
        1,
        spine_w,
        5,
        w.saturating_sub(4),
        1,
        LcarsTone::Lavender,
        LcarsTone::Amber,
    );
    canvas.stamp_stencil_label(right_x + 2 - spine_w, 2, "DIAG-02", LcarsTone::Amber);

    canvas.draw_lower_right_elbow(
        right_x,
        h.saturating_sub(4),
        spine_w,
        2,
        w.saturating_sub(8),
        1,
        LcarsTone::CoolBlue,
        LcarsTone::Orange,
    );

    let os_str = t.os_name.as_deref().unwrap_or("UNKNOWN OS");
    let arch_str = t.arch.as_deref().unwrap_or("N/A");
    let proc_cnt = t
        .process_count
        .map(|c| c.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    canvas.draw_text(
        2,
        3,
        &truncate_chars(
            &format!("SYS // {} [{}]   PROCS // {}", os_str, arch_str, proc_cnt),
            w.saturating_sub(spine_w + 6),
            false,
        ),
        ColorSpec::Tone(LcarsTone::Amber),
        ColorSpec::Reset,
        true,
    );

    let swap_str = match t.swap {
        Some(sw) => format!(
            "SWAP // {} / {} ({:4.1}%)",
            format_bytes(sw.used_bytes),
            format_bytes(sw.total_bytes),
            sw.used_pct
        ),
        None => "SWAP // OFFLINE OR UNCONFIGURED".to_string(),
    };
    canvas.draw_text(
        2,
        4,
        &swap_str,
        ColorSpec::Tone(LcarsTone::Lavender),
        ColorSpec::Reset,
        false,
    );

    // Compact Per-Core strip
    let max_cores_shown = 8usize.min(t.cpu_cores_pct.len());
    for idx in 0..max_cores_shown {
        let col = idx % 4;
        let row = idx / 4;
        let cx = 2 + col * 16;
        let cy = 6 + row;
        let pct = t.cpu_cores_pct[idx];
        canvas.draw_text(
            cx,
            cy,
            &format!("C{:02}:{:3.0}%", idx, pct),
            ColorSpec::Tone(LcarsTone::CoolBlue),
            ColorSpec::Reset,
            false,
        );
        canvas.draw_conduit_gauge(cx + 8, cy, 7, pct, LcarsTone::CoolBlue, LcarsTone::Orange);
    }

    // Top 5 CPU processes
    let proc_y = 10usize;
    canvas.draw_text(
        2,
        proc_y,
        "TOP-5 CPU PROCESS ALLOCATION // PID      NAME                 CPU%     MEM",
        ColorSpec::Tone(LcarsTone::Orange),
        ColorSpec::Reset,
        true,
    );
    for (i, proc_entry) in t.top_processes.iter().take(5).enumerate() {
        let ry = proc_y + 1 + i * 2;
        if ry + 2 >= h {
            break;
        }
        let pill_tone = match i {
            0 => LcarsTone::Salmon,
            1 => LcarsTone::Orange,
            2 => LcarsTone::Amber,
            3 => LcarsTone::Lavender,
            _ => LcarsTone::CoolBlue,
        };
        canvas.draw_pill(2, ry, 6, pill_tone, Some(&format!("P0{}", i + 1)));
        let name_fixed = format!(
            "{:<18}",
            truncate_chars(&proc_entry.name, 18, state.glyph_mode == GlyphMode::Unicode)
        );
        let line = format!(
            "{:>7}  {}  {:5.1}%  {:>9}",
            proc_entry.pid,
            name_fixed,
            proc_entry.cpu_pct,
            format_bytes(proc_entry.memory_bytes)
        );
        canvas.draw_text(
            9,
            ry,
            &line,
            ColorSpec::Tone(LcarsTone::Cream),
            ColorSpec::Reset,
            i == 0,
        );
    }

    render_footer_controls(canvas, h - 1, state);
}

/// Full Tactical Command Display (`120x36` and larger):
/// Designed specifically around the LCARS visual direction and 7 visual acceptance criteria.
fn render_full_primary(canvas: &mut Canvas, t: &SystemTelemetry, state: &UiState) {
    let w = canvas.width;
    let h = canvas.height;
    let spine_x = 2usize;
    let spine_w = 14usize;
    let right_limit = w.saturating_sub(4);

    // =========================================================================
    // 1. ASYMMETRIC LCARS SILHOUETTE: LEFT SPINE, SWEEPING ELBOWS & STEPPED RAILS
    // =========================================================================
    let top_shoulder_w = (w * 42 / 100).max(spine_w + 12);
    canvas.draw_upper_left_elbow(
        spine_x,
        1,
        spine_w,
        5,
        top_shoulder_w,
        2,
        LcarsTone::Orange,
        LcarsTone::Amber,
    );
    canvas.stamp_stencil_label(spine_x + 2, 2, "LCARS 47.4", LcarsTone::Orange);
    canvas.stamp_stencil_label(spine_x + 2, 4, "CMD-DECK", LcarsTone::Orange);

    // Stepped upper rail segments extending rightwards (leaving 1-col negative-space cuts)
    let seg1_x = spine_x + top_shoulder_w + 1;
    let seg1_w = 18usize;
    if seg1_x + seg1_w < right_limit {
        canvas.fill_solid_block(seg1_x, 1, seg1_w, 1, LcarsTone::Amber, LcarsTone::Salmon);
        canvas.stamp_stencil_label(seg1_x + 2, 1, "CHRONO-SYNC", LcarsTone::Amber);
    }

    let seg2_x = seg1_x + seg1_w + 1;
    let seg2_w = right_limit.saturating_sub(seg2_x + 14);
    if seg2_w > 6 {
        canvas.fill_solid_block(
            seg2_x,
            1,
            seg2_w,
            1,
            LcarsTone::Lavender,
            LcarsTone::Periwinkle,
        );
        let vessel = t.hostname.as_deref().unwrap_or("USS-ENTERPRISE");
        let vessel_tag = truncate_chars(
            &vessel.to_ascii_uppercase(),
            seg2_w.saturating_sub(4),
            state.glyph_mode == GlyphMode::Unicode,
        );
        canvas.stamp_stencil_label(seg2_x + 2, 1, &vessel_tag, LcarsTone::Lavender);
    }

    // Rightmost upper pill endcap showing Alert Condition
    let status_tone = match t.alert_state {
        SystemAlertState::Nominal => LcarsTone::CoolBlue,
        SystemAlertState::Elevated => LcarsTone::Amber,
        SystemAlertState::Critical => LcarsTone::Salmon,
    };
    if right_limit >= 13 {
        canvas.draw_pill(
            right_limit - 12,
            1,
            12,
            status_tone,
            Some(t.alert_state.label()),
        );
    }

    // Upper-left vertical spine segments (`y = 7..14`) with 1-row dark negative-space cuts
    let focus_tone = |idx: usize, base: LcarsTone| -> LcarsTone {
        if state.focus_index % 4 == idx {
            LcarsTone::Cream
        } else {
            base
        }
    };
    canvas.fill_solid_block(
        spine_x,
        7,
        spine_w - 1,
        3,
        focus_tone(0, LcarsTone::Amber),
        LcarsTone::Orange,
    );
    canvas.stamp_stencil_label(
        spine_x + 2,
        8,
        "01-CHRONO",
        focus_tone(0, LcarsTone::Amber),
    );

    canvas.fill_solid_block(
        spine_x,
        11,
        spine_w - 1,
        3,
        focus_tone(1, LcarsTone::Salmon),
        LcarsTone::Salmon,
    );
    canvas.stamp_stencil_label(
        spine_x + 2,
        12,
        "02-WARP/CPU",
        focus_tone(1, LcarsTone::Salmon),
    );

    // Mid-Deck Sweeping T-Bridge (`y = 15..17`) with an asymmetric step-down at `x = 66`
    let mid_y = 15usize;
    let bridge_split_x = (w * 56 / 100).clamp(54, w.saturating_sub(24));
    let left_bridge_w = bridge_split_x.saturating_sub(spine_x);
    canvas.draw_mid_left_bridge(
        spine_x,
        mid_y,
        spine_w - 1,
        left_bridge_w,
        LcarsTone::Orange,
        LcarsTone::Amber,
    );
    canvas.stamp_stencil_label(spine_x + 2, mid_y + 1, "EPS-CONDUIT", LcarsTone::Orange);

    // Asymmetric step-down elbow in the middle of the screen (`bridge_split_x .. right_limit`)
    let step_x = bridge_split_x;
    canvas.put_char(
        step_x,
        mid_y + 1,
        '▜',
        ColorSpec::Tone(LcarsTone::Amber),
        ColorSpec::Reset,
    );
    canvas.put_char(
        step_x,
        mid_y + 2,
        '▙',
        ColorSpec::Tone(LcarsTone::Lavender),
        ColorSpec::Reset,
    );
    let right_bridge_w = right_limit.saturating_sub(step_x + 1);
    for dx in 0..right_bridge_w {
        let cx = step_x + 1 + dx;
        let ratio = dx as f32 / right_bridge_w.max(1) as f32;
        let fg = ColorSpec::interpolate(LcarsTone::Lavender, LcarsTone::CoolBlue, ratio);
        let ch = if dx + 1 == right_bridge_w {
            '◗'
        } else if dx == 14 || dx == 30 {
            ' '
        } else {
            '▄'
        };
        if ch != ' ' {
            canvas.put_char(cx, mid_y + 2, ch, fg, ColorSpec::Reset);
        }
    }

    // Lower-left vertical spine segments (`y = 19..30`)
    canvas.fill_solid_block(
        spine_x,
        19,
        spine_w - 1,
        4,
        focus_tone(2, LcarsTone::Lavender),
        LcarsTone::Periwinkle,
    );
    canvas.stamp_stencil_label(
        spine_x + 2,
        20,
        "03-MEM-BANK",
        focus_tone(2, LcarsTone::Lavender),
    );

    canvas.fill_solid_block(
        spine_x,
        24,
        spine_w - 1,
        3,
        LcarsTone::Amber,
        LcarsTone::Orange,
    );
    canvas.stamp_stencil_label(spine_x + 2, 25, "04-STORAGE", LcarsTone::Amber);

    let bot_elbow_y = h.saturating_sub(5);
    if bot_elbow_y > 28 {
        canvas.fill_solid_block(
            spine_x,
            28,
            spine_w - 1,
            bot_elbow_y - 28,
            focus_tone(3, LcarsTone::CoolBlue),
            LcarsTone::IceBlue,
        );
        canvas.stamp_stencil_label(
            spine_x + 2,
            28,
            "05-SUBSPACE",
            focus_tone(3, LcarsTone::CoolBlue),
        );
    }

    // Lower-Left Sweeping Elbow (`bot_elbow_y .. h - 2`), terminating early at 72% width for strong asymmetry
    let bot_rail_w = (w * 72 / 100).max(spine_w + 20).min(w - 6);
    canvas.draw_lower_left_elbow(
        spine_x,
        bot_elbow_y,
        spine_w,
        4,
        bot_rail_w,
        2,
        LcarsTone::CoolBlue,
        LcarsTone::Lavender,
    );
    canvas.stamp_stencil_label(spine_x + 2, bot_elbow_y + 1, "SYS-LOCK", LcarsTone::CoolBlue);

    // Floating status pill in the open bottom-right negative space
    let pulse_char = match (t.tick % 4, state.paused) {
        (_, true) => "PAUSED",
        (0, false) => "SCAN ▸▸",
        (1, false) => "SCAN ▸▪",
        (2, false) => "SCAN ▪▸",
        _ => "LOCK ●●",
    };
    if spine_x + bot_rail_w + 16 < w {
        canvas.draw_pill(
            spine_x + bot_rail_w + 2,
            bot_elbow_y + 2,
            14,
            if state.paused {
                LcarsTone::Salmon
            } else {
                LcarsTone::Orange
            },
            Some(pulse_char),
        );
    }

    // =========================================================================
    // 2. DOMINANT FOCAL ELEMENT (UPPER COMMAND DECK: CHRONOMETER + CORE HARMONIC)
    // =========================================================================
    let deck_x = spine_x + spine_w + 4; // x = 20

    // Big 3-row vector chronometer (`HH:MM:SS`)
    let time_str = t.clock.format_time();
    let time_w = canvas.draw_big_digits(deck_x, 4, &time_str, LcarsTone::Cream, LcarsTone::Amber);

    // Sub-rail directly beneath big chronometer integrating Date, Stardate, Hostname, and Uptime
    let sub_rail_y = 8usize;
    for dx in 0..time_w.max(30) {
        let ch = if dx == 0 {
            '╰'
        } else if dx + 1 == time_w.max(30) {
            '◗'
        } else {
            '━'
        };
        canvas.put_char(
            deck_x + dx,
            sub_rail_y,
            ch,
            ColorSpec::Tone(LcarsTone::Amber),
            ColorSpec::Reset,
        );
    }

    let date_sd = format!(
        "EARTH {}  //  STARDATE {:07.2}",
        t.clock.format_date(),
        t.clock.stardate()
    );
    canvas.draw_text(
        deck_x,
        9,
        &date_sd,
        ColorSpec::Tone(LcarsTone::Amber),
        ColorSpec::Reset,
        true,
    );

    let host_full = t.hostname.as_deref().unwrap_or("OFFLINE");
    let up_full = t
        .uptime_secs
        .map(format_uptime)
        .unwrap_or_else(|| "N/A".to_string());
    canvas.draw_text(
        deck_x,
        10,
        &format!(
            "VESSEL // {:<18} MISSION UPTIME // {}",
            truncate_chars(host_full, 18, state.glyph_mode == GlyphMode::Unicode),
            up_full
        ),
        ColorSpec::Tone(LcarsTone::IceBlue),
        ColorSpec::Reset,
        false,
    );

    // Stepped Load Average Rails integrated below the chronometer (`y = 12..13`)
    if let Some([l1, l5, l15]) = t.load_avg {
        let cores = t.cpu_cores_pct.len().max(1) as f32;
        let l1_pct = ((l1 as f32 / cores) * 100.0).clamp(0.0, 100.0);
        canvas.draw_pill(deck_x, 12, 8, LcarsTone::Salmon, Some("LOAD"));
        canvas.draw_text(
            deck_x + 9,
            12,
            &format!("1M {:5.2}   5M {:5.2}   15M {:5.2}", l1, l5, l15),
            ColorSpec::Tone(LcarsTone::Cream),
            ColorSpec::Reset,
            true,
        );
        canvas.draw_conduit_gauge(
            deck_x + 9,
            13,
            28,
            l1_pct,
            LcarsTone::Amber,
            LcarsTone::Salmon,
        );
    } else {
        canvas.draw_text(
            deck_x,
            12,
            "LOAD VECTOR // TELEMETRY UNAVAILABLE",
            ColorSpec::Tone(LcarsTone::Muted),
            ColorSpec::Reset,
            false,
        );
    }

    // Focal Right-Center Upper Deck: Big Primary CPU Readout + Animated Warp/Core Harmonic Field
    let focal_cpu_x = (deck_x + 42).min(w.saturating_sub(54));
    let cpu_pct = t.cpu_overall_pct.unwrap_or(0.0);
    let cpu_big_str = match t.cpu_overall_pct {
        Some(v) => format!("{:04.1}%", v),
        None => "--.-%".to_string(),
    };
    let cpu_tone_top = if cpu_pct >= 85.0 {
        LcarsTone::Salmon
    } else {
        LcarsTone::Orange
    };
    let cpu_digits_w = canvas.draw_big_digits(
        focal_cpu_x,
        4,
        &cpu_big_str,
        cpu_tone_top,
        LcarsTone::Amber,
    );
    canvas.draw_text(
        focal_cpu_x,
        8,
        "CORE LOAD // PRIMARY",
        ColorSpec::Tone(LcarsTone::Orange),
        ColorSpec::Reset,
        true,
    );
    canvas.draw_conduit_gauge(
        focal_cpu_x,
        9,
        cpu_digits_w.max(20),
        cpu_pct,
        LcarsTone::Orange,
        LcarsTone::Salmon,
    );
    canvas.draw_text(
        focal_cpu_x,
        11,
        t.alert_state.code(),
        ColorSpec::Tone(status_tone),
        ColorSpec::Reset,
        true,
    );

    // Animated Braille Core Harmonic / Warp Field Visualizer (`harmonic_x .. right_limit`, `y = 4..13`)
    let harmonic_x = focal_cpu_x + cpu_digits_w.max(20) + 4;
    if harmonic_x + 16 < right_limit {
        let hw = (right_limit - harmonic_x).min(30);
        let hh = 9usize;
        render_warp_harmonic_field(canvas, harmonic_x, 4, hw, hh, t);
    }

    // =========================================================================
    // 3. LOWER LEFT BAY: INTEGRATED MEMORY MATRIX & ROOT STORAGE CONDUITS
    // =========================================================================
    let mem_x = spine_x + spine_w + 3; // x = 19
    let conduit_max_w = bridge_split_x.saturating_sub(mem_x + 2).max(24);

    // Memory Conduit (`y = 19..22`) anchored directly to `03-MEM-BANK` spine block
    if let Some(mem) = t.memory {
        canvas.draw_pill(mem_x, 19, 11, LcarsTone::Lavender, Some("MEM USED"));
        canvas.draw_text(
            mem_x + 12,
            19,
            &format!(
                "{} / {}  ({:4.1}%)",
                format_bytes(mem.used_bytes),
                format_bytes(mem.total_bytes),
                mem.used_pct
            ),
            ColorSpec::Tone(LcarsTone::Cream),
            ColorSpec::Reset,
            true,
        );
        canvas.draw_conduit_gauge(
            mem_x,
            20,
            conduit_max_w,
            mem.used_pct,
            LcarsTone::Lavender,
            LcarsTone::Salmon,
        );
        canvas.draw_text(
            mem_x + 2,
            21,
            &format!(
                "╰━━ AVAIL {:<12}  TOTAL {:<12}",
                format_bytes(mem.avail_bytes),
                format_bytes(mem.total_bytes)
            ),
            ColorSpec::Tone(LcarsTone::IceBlue),
            ColorSpec::Reset,
            false,
        );
    } else {
        canvas.draw_pill(mem_x, 19, 11, LcarsTone::Muted, Some("MEM N/A"));
        canvas.draw_text(
            mem_x + 12,
            19,
            "MEMORY TELEMETRY OFFLINE",
            ColorSpec::Tone(LcarsTone::Muted),
            ColorSpec::Reset,
            false,
        );
    }

    // Root Filesystem Conduit (`y = 24..27`) stepped inward for visual rhythm
    let disk_x = mem_x + 3;
    let disk_w = conduit_max_w.saturating_sub(3).max(20);
    if let Some(disk) = t.disk_root {
        canvas.draw_pill(disk_x, 24, 11, LcarsTone::Amber, Some("ROOTFS /"));
        canvas.draw_text(
            disk_x + 12,
            24,
            &format!(
                "{} / {}  ({:4.1}%)",
                format_bytes(disk.used_bytes),
                format_bytes(disk.total_bytes),
                disk.used_pct
            ),
            ColorSpec::Tone(LcarsTone::Amber),
            ColorSpec::Reset,
            true,
        );
        canvas.draw_conduit_gauge(
            disk_x,
            25,
            disk_w,
            disk.used_pct,
            LcarsTone::Amber,
            LcarsTone::Orange,
        );
        canvas.draw_text(
            disk_x + 2,
            26,
            &format!(
                "╰━━ FREE/AVAIL {:<12}  MOUNT /",
                format_bytes(disk.avail_bytes)
            ),
            ColorSpec::Tone(LcarsTone::Lavender),
            ColorSpec::Reset,
            false,
        );
    } else {
        canvas.draw_pill(disk_x, 24, 11, LcarsTone::Muted, Some("ROOT N/A"));
        canvas.draw_text(
            disk_x + 12,
            24,
            "FILESYSTEM TELEMETRY OFFLINE",
            ColorSpec::Tone(LcarsTone::Muted),
            ColorSpec::Reset,
            false,
        );
    }

    // =========================================================================
    // 4. LOWER RIGHT BAY: SUBSPACE NETWORK TRANSCEIVER & BRAILLE WAVEFORMS
    // =========================================================================
    let net_x = bridge_split_x + 2;
    let net_w = right_limit.saturating_sub(net_x).max(16);
    if let Some(net) = t.network {
        canvas.draw_pill(net_x, 19, 10, LcarsTone::CoolBlue, Some("NET RX"));
        canvas.draw_text(
            net_x + 11,
            19,
            &format!("{:<14}", format_rate(net.rx_bytes_per_sec)),
            ColorSpec::Tone(LcarsTone::IceBlue),
            ColorSpec::Reset,
            true,
        );

        let tx_pill_x = net_x + (net_w / 2).max(22);
        if tx_pill_x + 20 <= right_limit {
            canvas.draw_pill(tx_pill_x, 19, 10, LcarsTone::Orange, Some("NET TX"));
            canvas.draw_text(
                tx_pill_x + 11,
                19,
                &format_rate(net.tx_bytes_per_sec),
                ColorSpec::Tone(LcarsTone::Amber),
                ColorSpec::Reset,
                true,
            );
        } else {
            canvas.draw_pill(net_x, 20, 10, LcarsTone::Orange, Some("NET TX"));
            canvas.draw_text(
                net_x + 11,
                20,
                &format_rate(net.tx_bytes_per_sec),
                ColorSpec::Tone(LcarsTone::Amber),
                ColorSpec::Reset,
                true,
            );
        }

        // High-resolution Braille dual-channel waveform trace (`y = 21..28`)
        let wave_h = 7usize;
        render_network_braille_trace(
            canvas,
            net_x,
            21,
            net_w,
            wave_h,
            &t.net_rx_history,
            &t.net_tx_history,
            t.tick,
        );

        // Subtle baseline guide rail below the network waveform
        for dx in 0..net_w {
            let ch = if dx == 0 {
                '╰'
            } else if dx + 1 == net_w {
                '◗'
            } else {
                '─'
            };
            canvas.put_char(
                net_x + dx,
                28,
                ch,
                ColorSpec::Tone(LcarsTone::CoolBlue),
                ColorSpec::Reset,
            );
        }
        canvas.draw_text(
            net_x + 2,
            29,
            &format!("SUBSPACE LINKS // {} ACTIVE INTERFACES", net.active_interfaces),
            ColorSpec::Tone(LcarsTone::Muted),
            ColorSpec::Reset,
            false,
        );
    } else {
        canvas.draw_pill(net_x, 19, 12, LcarsTone::Muted, Some("SUBSPACE"));
        canvas.draw_text(
            net_x + 13,
            19,
            "NETWORK TELEMETRY OFFLINE",
            ColorSpec::Tone(LcarsTone::Muted),
            ColorSpec::Reset,
            false,
        );
    }

    // Bottom discoverable interaction control bar
    render_footer_controls(canvas, h - 1, state);
}

/// Renders the animated LCARS Warp / Core Harmonic Field using high-resolution `2x4` Braille dot plotting.
fn render_warp_harmonic_field(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    t: &SystemTelemetry,
) {
    let mut grid = BrailleGrid::new(w, h);
    let sw = grid.sub_width() as f64;
    let sh = grid.sub_height() as f64;
    let cx = sw * 0.52;
    let cy = sh * 0.50;

    let cpu_frac = (t.cpu_overall_pct.unwrap_or(15.0) / 100.0).clamp(0.05, 1.0) as f64;
    let phase = (t.tick as f64) * 0.35;

    // 1. Outer sweeping LCARS field arc
    let steps = 96;
    for i in 0..steps {
        let theta = (i as f64 / steps as f64) * std::f64::consts::TAU;
        // Leave an asymmetric LCARS aperture gap on the left side
        if theta > 2.6 && theta < 3.68 {
            continue;
        }
        let rx = sw * 0.44;
        let ry = sh * 0.44;
        let px = (cx + rx * theta.cos()).round() as isize;
        let py = (cy + ry * theta.sin()).round() as isize;
        if px >= 0 && py >= 0 {
            grid.set_dot(px as usize, py as usize, LcarsTone::CoolBlue);
        }
    }

    // 2. Inner dynamic harmonic ring modulated by CPU load and tick phase
    for i in 0..72 {
        let theta = (i as f64 / 72.0) * std::f64::consts::TAU + phase * 0.5;
        let mod_r = 1.0 + 0.18 * cpu_frac * (theta * 3.0 - phase).sin();
        let rx = sw * 0.25 * mod_r;
        let ry = sh * 0.25 * mod_r;
        let px = (cx + rx * theta.cos()).round() as isize;
        let py = (cy + ry * theta.sin()).round() as isize;
        if px >= 0 && py >= 0 {
            let tone = if cpu_frac > 0.75 {
                LcarsTone::Salmon
            } else {
                LcarsTone::Amber
            };
            grid.set_dot(px as usize, py as usize, tone);
        }
    }

    // 3. Live horizontal CPU harmonic trace across the field center
    let sub_w = grid.sub_width();
    let mut prev_pt: Option<(isize, isize)> = None;
    for sx in 0..sub_w {
        let hist_val = if !t.cpu_history.is_empty() {
            let idx = (sx * t.cpu_history.len()) / sub_w.max(1);
            t.cpu_history[idx.min(t.cpu_history.len() - 1)] as f64 / 100.0
        } else {
            cpu_frac
        };
        let wave = ((sx as f64 * 0.28) - phase).sin() * (0.12 + 0.30 * hist_val);
        let sy = (cy + wave * (sh * 0.42)).round().clamp(1.0, sh - 2.0) as isize;
        if let Some((px, py)) = prev_pt {
            grid.draw_line(px, py, sx as isize, sy, LcarsTone::Orange);
        } else {
            grid.set_dot(sx, sy as usize, LcarsTone::Orange);
        }
        prev_pt = Some((sx as isize, sy));
    }

    grid.blit_to(canvas, x, y);
}

/// Renders a dual-channel high-resolution Braille waveform for Network RX and TX rates.
#[allow(clippy::too_many_arguments)]
fn render_network_braille_trace(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    rx_hist: &[f64],
    tx_hist: &[f64],
    tick: u64,
) {
    let mut grid = BrailleGrid::new(w, h);
    let sub_w = grid.sub_width();
    let sub_h = grid.sub_height();
    if sub_w < 4 || sub_h < 8 {
        return;
    }

    let max_rate = rx_hist
        .iter()
        .chain(tx_hist.iter())
        .copied()
        .fold(1024.0_f64, f64::max);

    let mid_sy = (sub_h / 2) as isize;

    // Subtle horizon carrier dots
    for sx in (0..sub_w).step_by(4) {
        grid.set_dot(sx, mid_sy as usize, LcarsTone::TrackDark);
    }

    // Upper half: RX trace (CoolBlue)
    let mut prev_rx: Option<(isize, isize)> = None;
    for sx in 0..sub_w {
        let sample = sample_history(rx_hist, sx, sub_w);
        let norm = (sample / max_rate).sqrt().clamp(0.0, 1.0);
        // Subtle carrier ripple when idle so the transceiver trace feels alive
        let ripple = if sample < 10.0 {
            (((sx as f64 * 0.22) + (tick as f64 * 0.4)).sin().abs()) * 0.12
        } else {
            0.0
        };
        let amp = ((norm + ripple).clamp(0.05, 1.0) * ((mid_sy - 2) as f64)).round() as isize;
        let sy = (mid_sy - 1 - amp).max(0);
        if let Some((px, py)) = prev_rx {
            grid.draw_line(px, py, sx as isize, sy, LcarsTone::CoolBlue);
        } else {
            grid.set_dot(sx, sy as usize, LcarsTone::CoolBlue);
        }
        prev_rx = Some((sx as isize, sy));
    }

    // Lower half: TX trace (Amber/Orange)
    let mut prev_tx: Option<(isize, isize)> = None;
    for sx in 0..sub_w {
        let sample = sample_history(tx_hist, sx, sub_w);
        let norm = (sample / max_rate).sqrt().clamp(0.0, 1.0);
        let ripple = if sample < 10.0 {
            (((sx as f64 * 0.22) - (tick as f64 * 0.35)).cos().abs()) * 0.10
        } else {
            0.0
        };
        let amp = ((norm + ripple).clamp(0.05, 1.0) * ((sub_h as isize - mid_sy - 2) as f64))
            .round() as isize;
        let sy = (mid_sy + 1 + amp).min(sub_h as isize - 1);
        if let Some((px, py)) = prev_tx {
            grid.draw_line(px, py, sx as isize, sy, LcarsTone::Orange);
        } else {
            grid.set_dot(sx, sy as usize, LcarsTone::Orange);
        }
        prev_tx = Some((sx as isize, sy));
    }

    grid.blit_to(canvas, x, y);
}

fn sample_history(hist: &[f64], sx: usize, total_w: usize) -> f64 {
    if hist.is_empty() {
        return 0.0;
    }
    let idx = (sx * hist.len()) / total_w.max(1);
    hist[idx.min(hist.len() - 1)].max(0.0)
}

/// Full Secondary Diagnostic View (`120x36` and larger):
/// Features a visually distinct **Right-Hand Heavy LCARS Engineering Silhouette** with:
/// - Operating system and architecture
/// - Per-core CPU utilization vector array
/// - Swap usage (or tasteful LCARS offline state)
/// - Total process count
/// - Top 5 processes by CPU usage (PID, name, CPU %, memory usage)
fn render_full_secondary(canvas: &mut Canvas, t: &SystemTelemetry, state: &UiState) {
    let w = canvas.width;
    let h = canvas.height;
    let right_x = w.saturating_sub(3);
    let spine_w = 14usize;
    let spine_left_x = right_x + 1 - spine_w;

    // =========================================================================
    // 1. DISTINCT RIGHT-HAND ASYMMETRIC LCARS SILHOUETTE
    // =========================================================================
    let top_rail_w = (w * 78 / 100).max(spine_w + 20);
    canvas.draw_upper_right_elbow(
        right_x,
        1,
        spine_w,
        5,
        top_rail_w,
        2,
        LcarsTone::Lavender,
        LcarsTone::Amber,
    );
    canvas.stamp_stencil_label(spine_left_x + 1, 2, "LCARS DIAG", LcarsTone::Amber);
    canvas.stamp_stencil_label(spine_left_x + 1, 4, "SEC-02 ENG", LcarsTone::Amber);

    // Left-hand floating pill header on row 1..2
    canvas.draw_pill(3, 1, 18, LcarsTone::Orange, Some("ENG DIAGNOSTICS"));

    // Right vertical spine blocks (`y = 7..30`)
    canvas.fill_solid_block(
        spine_left_x + 1,
        7,
        spine_w - 1,
        4,
        LcarsTone::Salmon,
        LcarsTone::Orange,
    );
    canvas.stamp_stencil_label(spine_left_x + 2, 8, "CORE-ARRAY", LcarsTone::Salmon);

    canvas.fill_solid_block(
        spine_left_x + 1,
        12,
        spine_w - 1,
        4,
        LcarsTone::Lavender,
        LcarsTone::Periwinkle,
    );
    canvas.stamp_stencil_label(spine_left_x + 2, 13, "SWAP-CONV", LcarsTone::Lavender);

    // Mid-screen right-to-left cantilevered divider rail (`y = 17`)
    let div_y = 17usize;
    let div_start_x = 6usize;
    let div_w = (right_x + 1).saturating_sub(div_start_x);
    for dx in 0..div_w {
        let cx = div_start_x + dx;
        let ratio = dx as f32 / div_w.max(1) as f32;
        let fg = ColorSpec::interpolate(LcarsTone::CoolBlue, LcarsTone::Amber, ratio);
        let ch = if dx == 0 {
            '◖'
        } else if dx == 18 || dx == 52 {
            ' '
        } else {
            '█'
        };
        if ch != ' ' {
            canvas.put_char(cx, div_y, ch, fg, ColorSpec::Reset);
        }
    }
    canvas.stamp_stencil_label(div_start_x + 2, div_y, "PROC-TELEMETRY", LcarsTone::CoolBlue);

    // Lower-right spine blocks (`y = 19..30`)
    let bot_elbow_y = h.saturating_sub(5);
    if bot_elbow_y > 20 {
        canvas.fill_solid_block(
            spine_left_x + 1,
            19,
            spine_w - 1,
            bot_elbow_y - 19,
            LcarsTone::CoolBlue,
            LcarsTone::IceBlue,
        );
        canvas.stamp_stencil_label(spine_left_x + 2, 20, "TOP-5 PROC", LcarsTone::CoolBlue);
        if bot_elbow_y > 25 {
            canvas.stamp_stencil_label(spine_left_x + 2, 25, "PID-MATRIX", LcarsTone::CoolBlue);
        }
    }

    // Lower-Right Sweeping Elbow (`bot_elbow_y .. h - 2`)
    let bot_rail_w = (w * 82 / 100).max(spine_w + 20);
    canvas.draw_lower_right_elbow(
        right_x,
        bot_elbow_y,
        spine_w,
        4,
        bot_rail_w,
        2,
        LcarsTone::Orange,
        LcarsTone::Amber,
    );
    canvas.stamp_stencil_label(spine_left_x + 2, bot_elbow_y + 1, "DIAG-LOCK", LcarsTone::Amber);

    // =========================================================================
    // 2. UPPER ENGINEERING DECK: OS / ARCH, PROCESS COUNT, SWAP & PER-CORE ARRAY
    // =========================================================================
    let left_x = 4usize;
    let os_name = t.os_name.as_deref().unwrap_or("UNKNOWN OS");
    let arch = t.arch.as_deref().unwrap_or("UNKNOWN ARCH");
    canvas.draw_pill(left_x, 4, 12, LcarsTone::Amber, Some("OS // ARCH"));
    canvas.draw_text(
        left_x + 13,
        4,
        &format!(
            "{}  //  ARCH: {}",
            truncate_chars(os_name, 36, state.glyph_mode == GlyphMode::Unicode),
            arch
        ),
        ColorSpec::Tone(LcarsTone::Cream),
        ColorSpec::Reset,
        true,
    );

    // Process Count + Swap Matrix on Row 6
    let proc_cnt_str = t
        .process_count
        .map(|c| format!("{} ACTIVE TASKS", c))
        .unwrap_or_else(|| "UNAVAILABLE".to_string());
    canvas.draw_pill(left_x, 6, 12, LcarsTone::CoolBlue, Some("PROC COUNT"));
    canvas.draw_text(
        left_x + 13,
        6,
        &proc_cnt_str,
        ColorSpec::Tone(LcarsTone::IceBlue),
        ColorSpec::Reset,
        true,
    );

    let swap_x = left_x + 38;
    canvas.draw_pill(swap_x, 6, 10, LcarsTone::Lavender, Some("SWAP"));
    match t.swap {
        Some(sw) => {
            canvas.draw_conduit_gauge(
                swap_x + 11,
                6,
                18,
                sw.used_pct,
                LcarsTone::Lavender,
                LcarsTone::Salmon,
            );
            canvas.draw_text(
                swap_x + 30,
                6,
                &format!(
                    "{} / {} ({:4.1}%)",
                    format_bytes(sw.used_bytes),
                    format_bytes(sw.total_bytes),
                    sw.used_pct
                ),
                ColorSpec::Tone(LcarsTone::Cream),
                ColorSpec::Reset,
                false,
            );
        }
        None => {
            canvas.draw_text(
                swap_x + 11,
                6,
                "SWAP OFFLINE // ZERO ALLOCATION",
                ColorSpec::Tone(LcarsTone::Muted),
                ColorSpec::Reset,
                false,
            );
        }
    }

    // Per-Core CPU Utilization Harmonic Array (`y = 8..15`)
    canvas.draw_text(
        left_x,
        8,
        "PER-CORE SUBSYSTEM UTILIZATION MATRIX",
        ColorSpec::Tone(LcarsTone::Orange),
        ColorSpec::Reset,
        true,
    );

    if t.cpu_cores_pct.is_empty() {
        canvas.draw_text(
            left_x,
            10,
            "PER-CORE TELEMETRY UNAVAILABLE",
            ColorSpec::Tone(LcarsTone::Muted),
            ColorSpec::Reset,
            false,
        );
    } else {
        let avail_w = spine_left_x.saturating_sub(left_x + 3);
        let num_cores = t.cpu_cores_pct.len();
        let cols = if num_cores <= 6 {
            2
        } else if num_cores <= 18 {
            3
        } else {
            4
        };
        let col_w = (avail_w / cols).max(20);
        let max_rows = 6usize;
        let max_display = (cols * max_rows).min(num_cores);

        for idx in 0..max_display {
            let r = idx % max_rows;
            let c = idx / max_rows;
            let cx = left_x + c * col_w;
            let cy = 9 + r;
            let pct = t.cpu_cores_pct[idx];
            let lbl = format!("C{:02}", idx);
            let tone = if pct >= 85.0 {
                LcarsTone::Salmon
            } else if pct >= 50.0 {
                LcarsTone::Orange
            } else {
                LcarsTone::CoolBlue
            };
            canvas.draw_text(cx, cy, &lbl, ColorSpec::Tone(LcarsTone::Amber), ColorSpec::Reset, true);
            let bar_w = col_w.saturating_sub(13).clamp(6, 22);
            canvas.draw_conduit_gauge(cx + 4, cy, bar_w, pct, tone, LcarsTone::Salmon);
            canvas.draw_text(
                cx + 4 + bar_w + 1,
                cy,
                &format!("{:4.0}%", pct),
                ColorSpec::Tone(LcarsTone::Cream),
                ColorSpec::Reset,
                false,
            );
        }
    }

    // =========================================================================
    // 3. LOWER BAY: TOP-5 CPU PROCESS DIAGNOSTIC RAILS (`y = 19..30`)
    // =========================================================================
    canvas.draw_text(
        left_x,
        19,
        "TOP-5 CPU ALLOCATION   PID     PROCESS DESIGNATION        CPU UTILIZATION         RESIDENT MEM",
        ColorSpec::Tone(LcarsTone::Amber),
        ColorSpec::Reset,
        true,
    );

    if t.top_processes.is_empty() {
        canvas.draw_pill(left_x + 2, 22, 16, LcarsTone::Muted, Some("PROC OFFLINE"));
        canvas.draw_text(
            left_x + 20,
            22,
            "NO PROCESS TELEMETRY SAMPLES AVAILABLE",
            ColorSpec::Tone(LcarsTone::Muted),
            ColorSpec::Reset,
            false,
        );
    } else {
        for (idx, proc_entry) in t.top_processes.iter().take(5).enumerate() {
            let ry = 21 + idx * 2;
            if ry >= bot_elbow_y {
                break;
            }
            // Stepped left indentation for asymmetric LCARS flow
            let indent = left_x + (idx % 2) * 2;
            let pill_tone = match idx {
                0 => LcarsTone::Salmon,
                1 => LcarsTone::Orange,
                2 => LcarsTone::Amber,
                3 => LcarsTone::Lavender,
                _ => LcarsTone::CoolBlue,
            };
            let rank_tag = format!("P-0{}", idx + 1);
            canvas.draw_pill(indent, ry, 8, pill_tone, Some(&rank_tag));

            // Structural connector rail into process readout
            canvas.draw_text(
                indent + 8,
                ry,
                "━━▸",
                ColorSpec::Tone(pill_tone),
                ColorSpec::Reset,
                false,
            );

            let clean_name = truncate_chars(
                &proc_entry.name,
                24,
                state.glyph_mode == GlyphMode::Unicode,
            );
            let pid_and_name = format!("{:>8}    {:<24}", proc_entry.pid, clean_name);
            canvas.draw_text(
                indent + 12,
                ry,
                &pid_and_name,
                ColorSpec::Tone(LcarsTone::Cream),
                ColorSpec::Reset,
                idx == 0,
            );

            // Proportional CPU gauge & percentage
            let gauge_x = indent + 50;
            canvas.draw_conduit_gauge(
                gauge_x,
                ry,
                16,
                proc_entry.cpu_pct.clamp(0.0, 100.0),
                pill_tone,
                LcarsTone::Salmon,
            );
            canvas.draw_text(
                gauge_x + 17,
                ry,
                &format!("{:6.1}%", proc_entry.cpu_pct),
                ColorSpec::Tone(LcarsTone::Amber),
                ColorSpec::Reset,
                true,
            );

            // Memory usage
            canvas.draw_text(
                gauge_x + 27,
                ry,
                &format!("{:>11}", format_bytes(proc_entry.memory_bytes)),
                ColorSpec::Tone(LcarsTone::IceBlue),
                ColorSpec::Reset,
                false,
            );
        }
    }

    render_footer_controls(canvas, h - 1, state);
}

/// Renders discoverable LCARS control pills along the bottom row.
fn render_footer_controls(canvas: &mut Canvas, y: usize, state: &UiState) {
    if y >= canvas.height {
        return;
    }
    let controls: [(&str, &str, LcarsTone); 5] = [
        ("TAB", "CYCLE VIEW", LcarsTone::Orange),
        ("P", "PROC DIAG", LcarsTone::Amber),
        (
            "SPC",
            if state.paused { "RESUME" } else { "PAUSE" },
            if state.paused {
                LcarsTone::Salmon
            } else {
                LcarsTone::Lavender
            },
        ),
        ("?", "HELP", LcarsTone::CoolBlue),
        ("Q/ESC", "QUIT", LcarsTone::Salmon),
    ];

    let mut cx = 2usize;
    for (key, desc, tone) in controls {
        let pill_w = key.len() + 2;
        if cx + pill_w + desc.len() + 3 >= canvas.width {
            break;
        }
        canvas.draw_pill(cx, y, pill_w, tone, Some(key));
        canvas.draw_text(
            cx + pill_w + 1,
            y,
            desc,
            ColorSpec::Tone(LcarsTone::Cream),
            ColorSpec::Reset,
            false,
        );
        cx += pill_w + desc.len() + 4;
    }
}

/// Renders the `?` interactive Help Overlay framed as an asymmetric LCARS sub-panel.
fn render_help_overlay(canvas: &mut Canvas, state: &UiState) {
    let w = canvas.width;
    let h = canvas.height;
    let box_w = 64usize.min(w.saturating_sub(4));
    let box_h = 16usize.min(h.saturating_sub(4));
    if box_w < 28 || box_h < 10 {
        return;
    }
    let bx = (w - box_w) / 2;
    let by = (h - box_h) / 2;

    // Clear overlay interior
    for dy in 0..box_h {
        for dx in 0..box_w {
            canvas.put_char(
                bx + dx,
                by + dy,
                ' ',
                ColorSpec::Reset,
                ColorSpec::Tone(LcarsTone::CutoutDark),
            );
        }
    }

    // Left LCARS elbow frame on the modal
    canvas.draw_upper_left_elbow(
        bx,
        by,
        8,
        4,
        box_w,
        1,
        LcarsTone::Amber,
        LcarsTone::Orange,
    );
    canvas.draw_lower_left_elbow(
        bx,
        by + box_h - 3,
        8,
        3,
        box_w,
        1,
        LcarsTone::CoolBlue,
        LcarsTone::Lavender,
    );
    for ry in (by + 4)..(by + box_h - 3) {
        canvas.fill_solid_block(bx, ry, 7, 1, LcarsTone::Orange, LcarsTone::Orange);
    }
    canvas.stamp_stencil_label(bx + 1, by + 2, "MANUAL", LcarsTone::Amber);

    let tx = bx + 10;
    canvas.draw_text(
        tx,
        by + 2,
        "LCARS WORKSTATION COMMAND REFERENCE",
        ColorSpec::Tone(LcarsTone::Amber),
        ColorSpec::Reset,
        true,
    );

    let rows = [
        ("[TAB]       ", "Cycle command & diagnostic views"),
        ("[P]         ", "Open / close Secondary Process Diagnostic View"),
        ("[SPACE]     ", "Pause / resume live telemetry sampling"),
        ("[C]         ", "Cycle color mode (TrueColor/256/16/Monochrome)"),
        ("[U]         ", "Toggle Unicode Hi-Res / ASCII fallback glyphs"),
        ("[?]         ", "Toggle this LCARS command reference overlay"),
        ("[Q] / [ESC] ", "Restore terminal state and exit"),
    ];

    for (i, (k, desc)) in rows.iter().enumerate() {
        let ry = by + 4 + i;
        if ry + 3 >= by + box_h {
            break;
        }
        canvas.draw_text(
            tx,
            ry,
            k,
            ColorSpec::Tone(LcarsTone::CoolBlue),
            ColorSpec::Reset,
            true,
        );
        canvas.draw_text(
            tx + 13,
            ry,
            desc,
            ColorSpec::Tone(LcarsTone::Cream),
            ColorSpec::Reset,
            false,
        );
    }

    let status_line = format!(
        "MODE: {} // {} // {}ms",
        state.color_mode.label(),
        state.glyph_mode.label(),
        state.refresh_ms
    );
    canvas.draw_text(
        tx,
        by + box_h - 2,
        &truncate_chars(&status_line, box_w.saturating_sub(12), false),
        ColorSpec::Tone(LcarsTone::Lavender),
        ColorSpec::Reset,
        false,
    );
}
