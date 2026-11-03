use lcarsmon::ansi::RenderMode;
use lcarsmon::color::ColorMode;
use lcarsmon::run::{run, snapshot, Options};
use lcarsmon::ui::View;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "\
lcarsmon — LCARS-inspired system monitor

USAGE:
    lcarsmon [OPTIONS]

OPTIONS:
    -i, --interval <MS>     Sampling interval in milliseconds, 250-10000 [default: 1000]
        --color <MODE>      auto | truecolor | 256 | 16 | mono [default: auto]
        --mono              Same as --color mono
        --ascii             ASCII-only glyphs (auto-enabled for non-UTF-8 locales)
        --unicode           Force Unicode glyphs even if the locale is not UTF-8
        --snapshot <VIEW>   Print one frame of VIEW (command | diagnostic) and exit
        --size <COLSxROWS>  Snapshot size [default: 120x36]
        --ansi              Include colors in the snapshot
        --exit-after <MS>   Quit automatically (for startup/CPU measurements)
    -h, --help              Show this help
    -V, --version           Show version

KEYS:
    q / Esc  quit     ?  help     Space  hold/resume     p  diagnostics     Tab  cycle focus

ENVIRONMENT:
    NO_COLOR disables color. COLORTERM=truecolor enables 24-bit color.
";

struct Args {
    interval: Duration,
    color: Option<ColorMode>,
    ascii: Option<bool>,
    snapshot: Option<View>,
    size: (u16, u16),
    ansi: bool,
    exit_after: Option<Duration>,
}

fn parse_size(s: &str) -> Option<(u16, u16)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    let (w, h) = (w.parse::<u16>().ok()?, h.parse::<u16>().ok()?);
    (w > 0 && h > 0 && w <= 1000 && h <= 500).then_some((w, h))
}

fn parse_args() -> Result<Args, String> {
    let mut a =
        Args { interval: Duration::from_millis(1000), color: None, ascii: None, snapshot: None, size: (120, 36), ansi: false, exit_after: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut value = |name: &str| inline.clone().or_else(|| it.next()).ok_or(format!("{name} needs a value; see --help"));
        match flag.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("lcarsmon {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-i" | "--interval" => {
                let v = value("--interval")?;
                let ms: u64 = v.parse().map_err(|_| format!("--interval expects milliseconds, got '{v}'"))?;
                if !(250..=10_000).contains(&ms) {
                    return Err(format!("--interval must be between 250 and 10000 ms, got {ms}"));
                }
                a.interval = Duration::from_millis(ms);
            }
            "--color" => {
                let v = value("--color")?;
                a.color = match v.as_str() {
                    "auto" => None,
                    other => Some(ColorMode::parse(other).ok_or(format!("unknown --color '{other}'; use auto, truecolor, 256, 16 or mono"))?),
                };
            }
            "--mono" => a.color = Some(ColorMode::Mono),
            "--ascii" => a.ascii = Some(true),
            "--unicode" => a.ascii = Some(false),
            "--ansi" => a.ansi = true,
            "--snapshot" => {
                let v = value("--snapshot")?;
                a.snapshot = Some(match v.as_str() {
                    "command" | "primary" => View::Command,
                    "diagnostic" | "diag" | "processes" => View::Diagnostic,
                    other => return Err(format!("unknown view '{other}'; use command or diagnostic")),
                });
            }
            "--size" => {
                let v = value("--size")?;
                a.size = parse_size(&v).ok_or(format!("--size expects COLSxROWS like 120x36, got '{v}'"))?;
            }
            "--exit-after" => {
                let v = value("--exit-after")?;
                a.exit_after = Some(Duration::from_millis(v.parse().map_err(|_| format!("--exit-after expects milliseconds, got '{v}'"))?));
            }
            other => return Err(format!("unknown option '{other}'; see --help")),
        }
    }
    Ok(a)
}

/// Unicode unless the locale explicitly says otherwise (LANG=C, POSIX, ...).
fn locale_is_utf8() -> bool {
    for var in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                let v = v.to_ascii_lowercase();
                return v.contains("utf-8") || v.contains("utf8");
            }
        }
    }
    true
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("lcarsmon: {e}");
            return ExitCode::from(2);
        }
    };
    let color = args.color.unwrap_or_else(|| ColorMode::detect(|k| std::env::var(k).ok()));
    let ascii = args.ascii.unwrap_or(!locale_is_utf8());
    let mode = RenderMode { color, ascii };

    if let Some(view) = args.snapshot {
        print!("{}", snapshot(view, args.size, mode, args.ansi, args.interval.min(Duration::from_millis(1000))));
        return ExitCode::SUCCESS;
    }
    match run(Options { interval: args.interval, mode, exit_after: args.exit_after }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lcarsmon: {e}");
            ExitCode::from(1)
        }
    }
}
