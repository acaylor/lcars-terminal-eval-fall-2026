use lcars::app;
use lcars::metrics::{demo_sample, Collector};
use lcars::render::{self, detect_color, detect_unicode, View, ViewState};
use lcars::term::Term;
use std::env;
use std::io::{self, Write};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage());
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--measure") {
        return measure();
    }
    if let Some(which) = flag_value(&args, "--dump") {
        return dump(&args, &which);
    }
    match Term::enter() {
        Ok(mut term) => match app::run(&mut term) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                drop(term);
                eprintln!("lcars-monitor: {e}");
                ExitCode::from(1)
            }
        },
        Err(e) => {
            eprintln!("lcars-monitor: {e}");
            eprintln!("{}", usage());
            ExitCode::from(2)
        }
    }
}

fn measure() -> ExitCode {
    let t0 = Instant::now();
    let mut collector = Collector::new();
    let sample = collector.sample(false);
    let us = t0.elapsed().as_micros();
    println!("startup_us={us}");
    println!("platform={}", Collector::platform());
    println!("hostname={}", sample.hostname);
    println!("sample_ok=1");
    ExitCode::SUCCESS
}

fn dump(args: &[String], which: &str) -> ExitCode {
    let width = flag_value(args, "--width")
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    let height = flag_value(args, "--height")
        .and_then(|s| s.parse().ok())
        .unwrap_or(36);
    let shape = args.iter().any(|a| a == "--shape");
    let demo = args.iter().any(|a| a == "--demo");
    let mut ui = ViewState::new(detect_color(), detect_unicode());
    if args.iter().any(|a| a == "--ascii") {
        ui.unicode = false;
    }
    if args.iter().any(|a| a == "--mono") {
        ui.color = render::ColorMode::Mono;
    }
    ui.view = if which == "secondary" || which == "diag" {
        View::Diag
    } else {
        View::Primary
    };
    ui.cpu_trace = (0..40).map(|i| 8.0 + ((i * 17) % 70) as f64).collect();
    let sample = if demo {
        demo_sample()
    } else {
        let mut c = Collector::new();
        let _ = c.sample(ui.view == View::Diag);
        thread::sleep(Duration::from_millis(250));
        c.sample(ui.view == View::Diag)
    };
    let buf = render::render(&sample, &ui, width, height);
    let mut out = io::stdout();
    if shape {
        let _ = write!(out, "{}", buf.shape_map());
    } else {
        let blank = render::Buffer::new(1, 1);
        let bytes = render::diff_rows(&blank, &buf, ui.color, true);
        let _ = out.write_all(&bytes);
    }
    let _ = out.flush();
    ExitCode::SUCCESS
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn usage() -> &'static str {
    "lcars-monitor — LCARS engineering display\n\
     Run inside a terminal. Keys: q/esc quit, ? help, space pause, p diagnostics, tab focus.\n\
     LCARS_ASCII=1 disables Unicode. LCARS_MONO=1 or NO_COLOR selects monochrome.\n\
     --measure  print startup time after one sample and exit\n\
     --dump primary|secondary [--width N --height N --shape --demo]"
}
