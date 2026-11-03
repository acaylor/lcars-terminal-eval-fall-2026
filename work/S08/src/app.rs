use std::env;
use std::io::{self, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

use crate::buffer::Presenter;
use crate::clock::Clock;
use crate::collect::{Collector, SysinfoCollector};
use crate::model::{Metrics, SAMPLE_INTERVAL};
use crate::term::{self, KeyAction, Terminal};
use crate::theme::{ColorCapability, Theme};
use crate::views::{render_frame, UiState, View};

pub const WIDTH: i32 = 120;
pub const HEIGHT: i32 = 36;

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub unicode: Option<bool>,
    pub mono: bool,
    pub capability: Option<ColorCapability>,
    pub snapshot: Option<View>,
    pub plain: bool,
    pub width: i32,
    pub height: i32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            unicode: None,
            mono: false,
            capability: None,
            snapshot: None,
            plain: false,
            width: WIDTH,
            height: HEIGHT,
        }
    }
}

impl Options {
    fn theme(&self) -> Theme {
        match self.capability {
            Some(cap) => Theme::with_capability(cap, self.mono),
            None => Theme::detect(self.mono),
        }
    }

    fn unicode(&self) -> bool {
        self.unicode.unwrap_or_else(crate::theme::unicode_locale)
    }
}

pub struct App {
    collector: Box<dyn Collector>,
    metrics: Metrics,
    clock: Option<Clock>,
    paused: bool,
    help: bool,
    view: View,
    phase: u64,
    cpu_history: Vec<f32>,
    net_rx_history: Vec<f64>,
    net_tx_history: Vec<f64>,
    unicode: bool,
    theme: Theme,
    interval_secs: f32,
    last_sample: Instant,
    next_sample: Instant,
    sample_failures: u32,
}

impl App {
    pub fn new(collector: Box<dyn Collector>, options: &Options) -> Self {
        let metrics = collector.initial_metrics();
        let now = Instant::now();
        Self {
            collector,
            metrics,
            clock: Clock::now_local(),
            paused: false,
            help: false,
            view: View::Command,
            phase: 0,
            cpu_history: Vec::with_capacity(120),
            net_rx_history: Vec::with_capacity(120),
            net_tx_history: Vec::with_capacity(120),
            unicode: options.unicode(),
            theme: options.theme(),
            interval_secs: 0.0,
            last_sample: now,
            next_sample: now + Duration::from_millis(250),
            sample_failures: 0,
        }
    }

    fn apply_sample(&mut self, mut metrics: Metrics, elapsed: Duration) {
        if let Some(cpu) = metrics.cpu {
            push_capped(&mut self.cpu_history, cpu.clamp(0.0, 100.0), 120);
        }
        if let Some(rx) = metrics.net_rx_rate {
            push_capped(&mut self.net_rx_history, rx, 120);
        }
        if let Some(tx) = metrics.net_tx_rate {
            push_capped(&mut self.net_tx_history, tx, 120);
        }
        if metrics.collect_error.is_none() {
            self.sample_failures = 0;
        }
        metrics.collect_error = if self.sample_failures > 0 {
            Some(format!(
                "{} consecutive collector faults",
                self.sample_failures
            ))
        } else {
            None
        };
        self.metrics = metrics;
        self.interval_secs = elapsed.as_secs_f32().clamp(0.0, 30.0);
    }

    fn sample_if_due(&mut self) {
        if self.paused {
            return;
        }
        let now = Instant::now();
        if now < self.next_sample {
            return;
        }
        let elapsed = now.saturating_duration_since(self.last_sample);
        let want_processes = self.view == View::Diagnostics;
        term::set_quiet_panic(true);
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.collector.sample(elapsed, want_processes)
        }));
        term::set_quiet_panic(false);
        match result {
            Ok(metrics) => self.apply_sample(metrics, elapsed),
            Err(_) => {
                self.sample_failures = self.sample_failures.saturating_add(1);
                self.metrics.collect_error =
                    Some("collector fault; retaining last sample".to_string());
                self.interval_secs = elapsed.as_secs_f32().clamp(0.0, 30.0);
            }
        }
        self.last_sample = now;
        self.next_sample = now + SAMPLE_INTERVAL;
    }

    fn toggle_pause(&mut self) {
        if self.paused {
            self.paused = false;
            self.collector.reset_baseline();
            let now = Instant::now();
            self.last_sample = now;
            self.next_sample = now + Duration::from_millis(250);
            self.interval_secs = 0.0;
            self.clock = Clock::now_local();
        } else {
            self.paused = true;
        }
    }

    fn set_view(&mut self, view: View) {
        if self.view != view {
            self.view = view;
            self.help = false;
            if view == View::Diagnostics && !self.paused {
                let now = Instant::now();
                if self.next_sample > now + Duration::from_millis(150) {
                    self.next_sample = now + Duration::from_millis(150);
                }
            }
        }
    }

    fn state(&self) -> UiState<'_> {
        UiState {
            metrics: &self.metrics,
            clock: self.clock.as_ref(),
            paused: self.paused,
            view: self.view,
            help: self.help,
            phase: self.phase,
            cpu_history: &self.cpu_history,
            net_rx_history: &self.net_rx_history,
            net_tx_history: &self.net_tx_history,
            unicode: self.unicode,
            theme: &self.theme,
            interval_secs: self.interval_secs,
        }
    }

    fn draw(&self, presenter: &mut Presenter, stdout: &mut impl Write) -> Result<(), String> {
        let (w, h) = term::size();
        if w <= 0 || h <= 0 {
            return Ok(());
        }
        let state = self.state();
        let frame = render_frame(&state, w, h);
        let out = presenter.present(&frame, &self.theme);
        if !out.is_empty() {
            stdout
                .write_all(out.as_bytes())
                .and_then(|_| stdout.flush())
                .map_err(|err| format!("terminal output failed: {err}"))?;
        }
        Ok(())
    }

    fn run_loop(&mut self) -> Result<i32, String> {
        let mut presenter = Presenter::new();
        let mut stdout = io::stdout().lock();
        self.draw(&mut presenter, &mut stdout)?;
        loop {
            if let Some(signum) = term::take_signal() {
                return Ok(128 + signum);
            }

            match term::poll_event(Duration::from_millis(100)) {
                Ok(Some(event)) => match term::key_action(&event) {
                    KeyAction::Quit => return Ok(0),
                    KeyAction::Escape => {
                        if self.help {
                            self.help = false;
                            presenter.invalidate();
                        } else {
                            return Ok(0);
                        }
                    }
                    KeyAction::Help => {
                        self.help = !self.help;
                        presenter.invalidate();
                    }
                    KeyAction::Pause => self.toggle_pause(),
                    KeyAction::Processes => {
                        let view = if self.view == View::Diagnostics {
                            View::Command
                        } else {
                            View::Diagnostics
                        };
                        self.set_view(view);
                    }
                    KeyAction::Cycle => {
                        let view = if self.view == View::Command {
                            View::Diagnostics
                        } else {
                            View::Command
                        };
                        self.set_view(view);
                    }
                    KeyAction::None => {}
                },
                Ok(None) => {}
                Err(err) => return Err(format!("terminal input failed: {err}")),
            }

            self.sample_if_due();
            if !self.paused {
                self.phase = self.phase.wrapping_add(1);
                self.clock = Clock::now_local();
            }

            self.draw(&mut presenter, &mut stdout)?;
        }
    }
}

pub fn run(options: Options) -> Result<i32, String> {
    term::install_panic_hook();
    term::install_signal_handlers();

    if let Some(view) = options.snapshot {
        return run_snapshot(&options, view).map(|_| 0);
    }

    if env::var("TERM").is_ok_and(|t| t == "dumb") {
        return Err(
            "TERM=dumb is not a suitable terminal. Run with a normal terminal or use --snapshot."
                .to_string(),
        );
    }

    let collector: Box<dyn Collector> = Box::new(SysinfoCollector::new());
    let mut app = App::new(collector, &options);
    let mut terminal = Terminal::enter().map_err(|err| err.to_string())?;
    let result = app.run_loop();
    terminal.restore();
    result
}

fn run_snapshot(options: &Options, view: View) -> Result<(), String> {
    let mut collector: Box<dyn Collector> = Box::new(SysinfoCollector::new());
    let mut histories_cpu = Vec::new();
    let mut histories_rx = Vec::new();
    let mut histories_tx = Vec::new();

    for wait_ms in [250u64, 750] {
        std::thread::sleep(Duration::from_millis(wait_ms));
        let start = Instant::now();
        let result = catch_unwind(AssertUnwindSafe(|| {
            collector.sample(Duration::from_millis(wait_ms), view == View::Diagnostics)
        }));
        match result {
            Ok(metrics) => {
                if let Some(cpu) = metrics.cpu {
                    push_capped(&mut histories_cpu, cpu.clamp(0.0, 100.0), 120);
                }
                if let Some(rx) = metrics.net_rx_rate {
                    push_capped(&mut histories_rx, rx, 120);
                }
                if let Some(tx) = metrics.net_tx_rate {
                    push_capped(&mut histories_tx, tx, 120);
                }
                let interval = start.elapsed().as_secs_f32().max(wait_ms as f32 / 1000.0);
                if wait_ms == 750 {
                    let clock = Clock::now_local();
                    let theme = options.theme();
                    let state = UiState {
                        metrics: &metrics,
                        clock: clock.as_ref(),
                        paused: false,
                        view,
                        help: false,
                        phase: 6,
                        cpu_history: &histories_cpu,
                        net_rx_history: &histories_rx,
                        net_tx_history: &histories_tx,
                        unicode: options.unicode(),
                        theme: &theme,
                        interval_secs: interval,
                    };
                    let frame = render_frame(&state, options.width.max(1), options.height.max(1));
                    let mut stdout = io::stdout().lock();
                    if options.plain {
                        stdout
                            .write_all(frame.plain_text().as_bytes())
                            .and_then(|_| stdout.write_all(b"\n"))
                            .map_err(|e| e.to_string())?;
                    } else {
                        let mut presenter = Presenter::new();
                        let rendered = presenter.present(&frame, &theme);
                        stdout
                            .write_all(rendered.as_bytes())
                            .and_then(|_| stdout.write_all(b"\x1b[0m\n"))
                            .map_err(|e| e.to_string())?;
                    }
                    stdout.flush().map_err(|e| e.to_string())?;
                }
            }
            Err(_) => {
                if wait_ms == 750 {
                    // A capture is still useful when a gauge is unavailable.
                    let metrics = Metrics::default();
                    let theme = options.theme();
                    let clock = Clock::now_local();
                    let state = UiState {
                        metrics: &metrics,
                        clock: clock.as_ref(),
                        paused: false,
                        view,
                        help: false,
                        phase: 6,
                        cpu_history: &[],
                        net_rx_history: &[],
                        net_tx_history: &[],
                        unicode: options.unicode(),
                        theme: &theme,
                        interval_secs: 0.0,
                    };
                    let frame = render_frame(&state, options.width.max(1), options.height.max(1));
                    let mut stdout = io::stdout().lock();
                    if options.plain {
                        let _ = writeln!(stdout, "{}", frame.plain_text());
                    } else {
                        let mut presenter = Presenter::new();
                        let _ = write!(stdout, "{}", presenter.present(&frame, &theme));
                    }
                }
            }
        }
    }
    Ok(())
}

fn push_capped<T>(values: &mut Vec<T>, value: T, cap: usize) {
    if values.len() >= cap {
        values.remove(0);
    }
    values.push(value);
}

pub fn main_entry() -> i32 {
    match parse_options(env::args().skip(1).peekable()) {
        Ok(ParseOutcome::Help) => {
            print_help();
            0
        }
        Ok(ParseOutcome::Version) => {
            println!("lcars-mon {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Ok(ParseOutcome::Run(options)) => match run(options) {
            Ok(code) => code,
            Err(err) => {
                eprintln!("lcars-mon: {err}");
                1
            }
        },
        Err(err) => {
            eprintln!("lcars-mon: {err}");
            eprintln!("Try 'lcars-mon --help' for usage.");
            2
        }
    }
}

enum ParseOutcome {
    Help,
    Version,
    Run(Options),
}

fn parse_options<I>(mut args: std::iter::Peekable<I>) -> Result<ParseOutcome, String>
where
    I: Iterator<Item = String>,
{
    let mut options = Options::default();
    let mut saw_snapshot = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParseOutcome::Help),
            "-V" | "--version" => return Ok(ParseOutcome::Version),
            "--ascii" => options.unicode = Some(false),
            "--unicode" => options.unicode = Some(true),
            "--mono" => options.mono = true,
            "--plain" => options.plain = true,
            "--snapshot" => {
                saw_snapshot = true;
                let view = match args.peek().map(String::as_str) {
                    Some("primary") => {
                        args.next();
                        View::Command
                    }
                    Some("diagnostics") => {
                        args.next();
                        View::Diagnostics
                    }
                    _ => View::Command,
                };
                options.snapshot = Some(view);
            }
            "--color" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--color requires auto|truecolor|256|16|mono".to_string())?;
                apply_color(&mut options, &value)?;
            }
            "--size" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--size requires WIDTHxHEIGHT".to_string())?;
                apply_size(&mut options, &value)?;
            }
            "--" => {
                if args.next().is_some() {
                    return Err("unexpected positional argument".to_string());
                }
            }
            other if other.starts_with("--color=") => {
                apply_color(&mut options, &other[8..])?;
            }
            other if other.starts_with("--snapshot=") => {
                saw_snapshot = true;
                let view = &other[11..];
                options.snapshot = Some(match view {
                    "primary" => View::Command,
                    "diagnostics" => View::Diagnostics,
                    _ => return Err(format!("unknown snapshot view '{view}'")),
                });
            }
            other if other.starts_with("--size=") => {
                apply_size(&mut options, &other[7..])?;
            }
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    if options.plain && !saw_snapshot {
        return Err("--plain is only meaningful with --snapshot".to_string());
    }
    Ok(ParseOutcome::Run(options))
}

fn apply_color(options: &mut Options, value: &str) -> Result<(), String> {
    match value {
        "auto" => {
            options.capability = None;
            options.mono = false;
        }
        "truecolor" | "24bit" => options.capability = Some(ColorCapability::Truecolor),
        "256" | "256color" => options.capability = Some(ColorCapability::Ansi256),
        "16" | "16color" => options.capability = Some(ColorCapability::Ansi16),
        "mono" | "monochrome" => {
            options.mono = true;
            options.capability = None;
        }
        _ => return Err(format!("unknown color mode '{value}'")),
    }
    Ok(())
}

fn apply_size(options: &mut Options, value: &str) -> Result<(), String> {
    let (w, h) = value
        .split_once('x')
        .ok_or_else(|| "--size must look like 120x36".to_string())?;
    let w: i32 = w
        .parse()
        .map_err(|_| "--size width is not a number".to_string())?;
    let h: i32 = h
        .parse()
        .map_err(|_| "--size height is not a number".to_string())?;
    if !(10..=500).contains(&w) || !(5..=300).contains(&h) {
        return Err("--size must be between 10x5 and 500x300".to_string());
    }
    options.width = w;
    options.height = h;
    Ok(())
}

fn print_help() {
    println!(
        "\
LCARS 47 // terminal systems monitor

USAGE
  lcars-mon [OPTIONS]

CONTROLS
  q / Esc     quit (Esc closes help first)
  ?           toggle the command reference
  Space       hold / resume sampling
  p           open / close the process diagnostics
  Tab         cycle the command and diagnostic views

OPTIONS
  --ascii            force the ASCII-safe glyph fallback
  --unicode          force Unicode box/block/braille glyphs
  --mono             monochrome palette (still uses structure and tone)
  --color MODE       auto | truecolor | 256 | 16 | mono
  --snapshot[=VIEW]  render one real-data frame and exit
                     VIEW is primary (default) or diagnostics
  --plain            with --snapshot, print cells without ANSI color
  --size WxH         snapshot dimensions (default 120x36)
  -h, --help         show this help
  -V, --version      show the version

True color is detected from COLORTERM/TERM and used when available.
NO_COLOR forces the reduced monochrome treatment. The display is original
LCARS-inspired art and is not affiliated with or copied from any franchise."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::sample_metrics;
    use crate::model::format_rate;

    #[test]
    fn size_parser_rejects_garbage_and_accepts_valid() {
        let mut o = Options::default();
        assert!(apply_size(&mut o, "120x36").is_ok());
        assert_eq!(o.width, 120);
        assert_eq!(o.height, 36);
        assert!(apply_size(&mut o, "bad").is_err());
        assert!(apply_size(&mut o, "0x0").is_err());
    }

    #[test]
    fn color_parser_sets_modes() {
        let mut o = Options::default();
        apply_color(&mut o, "truecolor").unwrap();
        assert_eq!(o.capability, Some(ColorCapability::Truecolor));
        apply_color(&mut o, "mono").unwrap();
        assert!(o.mono);
        assert!(apply_color(&mut o, "nope").is_err());
    }

    struct FakeCollector;

    impl Collector for FakeCollector {
        fn sample(&mut self, _elapsed: Duration, _want_processes: bool) -> Metrics {
            sample_metrics()
        }
        fn initial_metrics(&self) -> Metrics {
            sample_metrics()
        }
        fn backend(&self) -> &'static str {
            "fake"
        }
    }

    #[test]
    fn histories_are_bounded_and_sampling_is_deterministic() {
        let mut app = App::new(Box::new(FakeCollector), &Options::default());
        for _ in 0..200 {
            app.apply_sample(sample_metrics(), Duration::from_secs(1));
        }
        assert_eq!(app.cpu_history.len(), 120);
        assert_eq!(app.net_rx_history.len(), 120);
        assert_eq!(app.metrics.cpu, Some(37.0));
        assert!((app.interval_secs - 1.0).abs() < 0.001);
    }

    #[test]
    fn snapshot_render_is_full_height_at_120x36() {
        let opts = Options {
            snapshot: Some(View::Command),
            plain: true,
            ..Options::default()
        };
        let theme = opts.theme();
        let metrics = sample_metrics();
        let clock = Clock::now_local();
        let state = UiState {
            metrics: &metrics,
            clock: clock.as_ref(),
            paused: false,
            view: View::Command,
            help: false,
            phase: 2,
            cpu_history: &[20.0, 60.0],
            net_rx_history: &[1000.0],
            net_tx_history: &[100.0],
            unicode: true,
            theme: &theme,
            interval_secs: 1.0,
        };
        let b = render_frame(&state, 120, 36);
        let text = b.plain_text();
        assert_eq!(text.lines().count(), 36);
        assert!(text.lines().all(|l| l.chars().count() == 120));
        assert!(format_rate(1024.0).contains("KB/s"));
    }
}
