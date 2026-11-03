use std::env;
use std::process;

use lcars_mon::app::AppController;
use lcars_mon::bench::run_benchmark;
use lcars_mon::theme::{ColorMode, GlyphMode};
use lcars_mon::ui::ViewMode;

#[derive(Debug, Clone)]
struct CliConfig {
    color_mode: ColorMode,
    glyph_mode: GlyphMode,
    refresh_ms: u64,
    capture_view: Option<(ViewMode, usize, usize)>,
    capture_plain: bool,
    capture_strip_text: bool,
    bench_iters: Option<usize>,
    show_help: bool,
    show_version: bool,
}

fn parse_dims(arg: &str) -> Option<(usize, usize)> {
    let (w_s, h_s) = arg.split_once('x').or_else(|| arg.split_once('X'))?;
    let w: usize = w_s.parse().ok()?;
    let h: usize = h_s.parse().ok()?;
    if w > 0 && h > 0 {
        Some((w, h))
    } else {
        None
    }
}

fn parse_args() -> Result<CliConfig, String> {
    let mut cfg = CliConfig {
        color_mode: ColorMode::detect(),
        glyph_mode: GlyphMode::detect(),
        refresh_ms: 750,
        capture_view: None,
        capture_plain: false,
        capture_strip_text: false,
        bench_iters: None,
        show_help: false,
        show_version: false,
    };

    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => cfg.show_help = true,
            "-V" | "--version" => cfg.show_version = true,
            "--mono" | "--no-color" => cfg.color_mode = ColorMode::Monochrome,
            "--ascii" => cfg.glyph_mode = GlyphMode::Ascii,
            "--unicode" => cfg.glyph_mode = GlyphMode::Unicode,
            "--plain" => cfg.capture_plain = true,
            "--strip-text" => cfg.capture_strip_text = true,
            "-i" | "--interval" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    return Err("Missing value for `--interval <ms>` (expected e.g. 750).".to_string());
                };
                let ms: u64 = val
                    .parse()
                    .map_err(|_| format!("Invalid interval `{}`: expected integer milliseconds.", val))?;
                cfg.refresh_ms = ms.clamp(100, 10_000);
            }
            "--color" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    return Err(
                        "Missing value for `--color <auto|truecolor|256|16|mono>`.".to_string()
                    );
                };
                cfg.color_mode = match val.to_ascii_lowercase().as_str() {
                    "auto" => ColorMode::detect(),
                    "truecolor" | "24bit" | "24" => ColorMode::TrueColor,
                    "256" | "256color" => ColorMode::Ansi256,
                    "16" | "16color" | "ansi" => ColorMode::Ansi16,
                    "mono" | "monochrome" | "none" => ColorMode::Monochrome,
                    other => {
                        return Err(format!(
                            "Unknown color mode `{}`: expected `auto`, `truecolor`, `256`, `16`, or `mono`.",
                            other
                        ))
                    }
                };
            }
            "--capture-primary" => {
                let mut dims = (120, 36);
                if let Some(next) = args.get(i + 1) {
                    if !next.starts_with('-') {
                        if let Some(d) = parse_dims(next) {
                            dims = d;
                            i += 1;
                        }
                    }
                }
                cfg.capture_view = Some((ViewMode::PrimaryCommand, dims.0, dims.1));
            }
            "--capture-secondary" => {
                let mut dims = (120, 36);
                if let Some(next) = args.get(i + 1) {
                    if !next.starts_with('-') {
                        if let Some(d) = parse_dims(next) {
                            dims = d;
                            i += 1;
                        }
                    }
                }
                cfg.capture_view = Some((ViewMode::ProcessDiagnostics, dims.0, dims.1));
            }
            "--bench" => {
                let mut frames = 100usize;
                if let Some(next) = args.get(i + 1) {
                    if !next.starts_with('-') {
                        if let Ok(n) = next.parse::<usize>() {
                            frames = n;
                            i += 1;
                        }
                    }
                }
                cfg.bench_iters = Some(frames);
            }
            other => {
                return Err(format!(
                    "Unrecognized option `{}`. Run `lcars-mon --help` for usage.",
                    other
                ));
            }
        }
        i += 1;
    }

    Ok(cfg)
}

fn print_usage() {
    println!(
        "lcars-mon {} — Starfleet LCARS Terminal System Monitor\n\n\
         USAGE:\n\
         \x20 lcars-mon [OPTIONS]\n\n\
         OPTIONS:\n\
         \x20 -i, --interval <ms>             Refresh interval in ms (default: 750)\n\
         \x20     --color <mode>              Color mode: auto, truecolor, 256, 16, mono\n\
         \x20     --mono, --no-color          Force monochrome rendering\n\
         \x20     --ascii                     Use 7-bit ASCII degraded glyph fallback\n\
         \x20     --unicode                   Use Unicode half-block, quadrant & Braille glyphs\n\
         \x20     --capture-primary [WxH]     Capture Primary Command View (default: 120x36) and exit\n\
         \x20     --capture-secondary [WxH]   Capture Secondary Diagnostic View (default: 120x36) and exit\n\
         \x20     --plain                     Omit ANSI escape codes in capture output\n\
         \x20     --strip-text                Replace alphanumeric labels with blanks in capture\n\
         \x20     --bench [frames]            Measure startup, CPU, RSS memory, and frame latency\n\
         \x20 -h, --help                      Print this help message\n\
         \x20 -V, --version                   Print version information\n\n\
         INTERACTIVE CONTROLS:\n\
         \x20 Tab         Cycle command and engineering diagnostic views\n\
         \x20 p           Open / close Secondary Process Diagnostic View\n\
         \x20 Space       Pause / resume live telemetry updates\n\
         \x20 c           Cycle color mode (TrueColor -> 256 -> 16 -> Monochrome)\n\
         \x20 u           Toggle Unicode / ASCII fallback glyph mode\n\
         \x20 ?           Open / close LCARS command reference overlay\n\
         \x20 q / Esc     Restore terminal state and exit",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() {
    let cfg = match parse_args() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("lcars-mon: {}", err);
            process::exit(2);
        }
    };

    if cfg.show_help {
        print_usage();
        return;
    }

    if cfg.show_version {
        println!("lcars-mon {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if let Some(iters) = cfg.bench_iters {
        print!("{}", run_benchmark(iters));
        return;
    }

    if let Some((view, w, h)) = cfg.capture_view {
        let mut app = AppController::new(cfg.color_mode, cfg.glyph_mode, cfg.refresh_ms);
        let canvas = app.capture_live_canvas(view, w, h);
        if cfg.capture_strip_text {
            println!("{}", canvas.render_plain_text_stripped());
        } else if cfg.capture_plain || cfg.color_mode == ColorMode::Monochrome {
            println!("{}", canvas.render_plain());
        } else {
            println!("{}", canvas.render_ansi_snapshot(cfg.color_mode));
        }
        return;
    }

    let mut app = AppController::new(cfg.color_mode, cfg.glyph_mode, cfg.refresh_ms);
    if let Err(err) = app.run_interactive() {
        eprintln!("lcars-mon: {}", err);
        process::exit(1);
    }
}
