//! lcars: an LCARS-inspired terminal system monitor.

use lcars::app::{App, Options};
use lcars::metrics;
use lcars::term::{self, Buffer, ColorMode, Glyphs};
use lcars::ui::View;
use std::time::{Duration, Instant};

const USAGE: &str = "\
lcars - LCARS-inspired terminal system monitor

USAGE
    lcars [OPTIONS]

OPTIONS
    --interval <MS>      Sampling interval, 500-2000 (default 1000)
    --color <MODE>       auto | true | 256 | mono (default auto-detect)
    --mono               Shortcut for --color mono
    --ascii              Use the 7-bit ASCII glyph set (auto when the locale is not UTF-8)
    --view <NAME>        Start on `command` (default) or `diag`
    --snapshot <WxH>     Render one frame at the given size as plain text to stdout and exit
    --exit-after <MS>    Quit automatically after MS (0 = right after the first frame);
                         prints the measured startup time to stderr
    -h, --help           Show this help
    -V, --version        Show the version

KEYS
    q / Esc  quit      ?  help      Space  hold/resume      p  process diagnostics      Tab  cycle views
";

struct Cli {
    opts: Options,
    snapshot: Option<(usize, usize)>,
}

fn parse_args() -> Result<Cli, String> {
    let env = |k: &str| std::env::var(k).ok();
    let mut interval = Duration::from_millis(1000);
    let mut color = ColorMode::detect(env);
    let mut glyphs = Glyphs::detect(env);
    let mut view = View::Command;
    let mut snapshot = None;
    let mut exit_after = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} requires a value (see --help)"))
        };
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("lcars {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--interval" => {
                let ms: u64 = value("--interval")?
                    .parse()
                    .map_err(|_| "--interval expects milliseconds, e.g. --interval 1000")?;
                if !(500..=2000).contains(&ms) {
                    return Err(format!("--interval {ms} is outside the supported 500-2000 ms range"));
                }
                interval = Duration::from_millis(ms);
            }
            "--color" => {
                color = match value("--color")?.as_str() {
                    "auto" => ColorMode::detect(env),
                    "true" | "truecolor" | "24bit" => ColorMode::TrueColor,
                    "256" => ColorMode::Ansi256,
                    "mono" | "none" => ColorMode::Mono,
                    other => return Err(format!("unknown --color mode '{other}' (use auto, true, 256 or mono)")),
                };
            }
            "--mono" => color = ColorMode::Mono,
            "--ascii" => glyphs = Glyphs::ascii(),
            "--view" => {
                view = match value("--view")?.as_str() {
                    "command" | "cmd" => View::Command,
                    "diag" | "diagnostic" | "proc" => View::Diagnostic,
                    other => return Err(format!("unknown --view '{other}' (use command or diag)")),
                };
            }
            "--snapshot" => {
                let v = value("--snapshot")?;
                let (w, h) = v
                    .split_once('x')
                    .ok_or("--snapshot expects WxH, e.g. --snapshot 120x36")?;
                let w: usize = w.parse().map_err(|_| "--snapshot width must be a number")?;
                let h: usize = h.parse().map_err(|_| "--snapshot height must be a number")?;
                snapshot = Some((w.min(1000), h.min(1000)));
            }
            "--exit-after" => {
                let ms: u64 = value("--exit-after")?
                    .parse()
                    .map_err(|_| "--exit-after expects milliseconds")?;
                exit_after = Some(Duration::from_millis(ms));
            }
            other => return Err(format!("unknown option '{other}' (see --help)")),
        }
    }
    Ok(Cli {
        opts: Options {
            interval,
            color_mode: color,
            glyphs,
            view,
            exit_after,
        },
        snapshot,
    })
}

fn main() {
    let start = Instant::now();
    let cli = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lcars: {e}");
            std::process::exit(2);
        }
    };
    let exit_after = cli.opts.exit_after;
    let mut app = App::new(metrics::platform_collector(), cli.opts);

    if let Some((w, h)) = cli.snapshot {
        // Headless render: two readings so rates and percentages exist.
        app.sample();
        std::thread::sleep(Duration::from_millis(300));
        app.sample();
        let mut buf = Buffer::new(w, h);
        app.draw(&mut buf, &[]);
        let text = buf.rows_text().join("\n");
        println!("{text}");
        return;
    }

    match app.run(start) {
        Ok(first_frame) => {
            if exit_after.is_some() {
                eprintln!(
                    "lcars: first frame rendered {:.1} ms after process start",
                    first_frame.as_secs_f64() * 1000.0
                );
            }
        }
        Err(e) => {
            term::session::restore();
            eprintln!("lcars: {e}");
            std::process::exit(1);
        }
    }
}
