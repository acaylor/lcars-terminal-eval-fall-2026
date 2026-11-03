use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use lcars_monitor::buffer::{ColorMode, DiffRenderer};
use lcars_monitor::metrics::Collector;
use lcars_monitor::model::History;
use lcars_monitor::render::{self, RenderOptions, View};
use lcars_monitor::terminal::TerminalSession;
use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};
use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default)]
struct Config {
    ascii: bool,
    mono: bool,
    snapshot: Option<View>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("lcars-monitor: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let Some(config) = parse_args()? else {
        return Ok(());
    };
    let mut collector = Collector::new()?;
    if let Some(view) = config.snapshot {
        return snapshot(&mut collector, view, config.ascii);
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "an interactive terminal is required; use --snapshot primary for redirected output"
                .into(),
        );
    }
    if std::env::var("TERM").is_ok_and(|term| term == "dumb") {
        return Err(
            "TERM=dumb cannot display the interface; use a capable terminal or --snapshot primary"
                .into(),
        );
    }
    interactive(collector, config)
}

fn parse_args() -> Result<Option<Config>, String> {
    let mut config = Config::default();
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--ascii" => config.ascii = true,
            "--mono" => config.mono = true,
            "--snapshot" => {
                let view = arguments
                    .next()
                    .ok_or_else(|| "--snapshot requires 'primary' or 'secondary'".to_string())?;
                config.snapshot = Some(match view.as_str() {
                    "primary" => View::Primary,
                    "secondary" | "diagnostics" => View::Diagnostics,
                    _ => {
                        return Err(format!(
                            "unknown snapshot view '{view}'; use primary or secondary"
                        ));
                    }
                });
            }
            "-h" | "--help" => {
                println!(
                    "lcars-monitor 1.0.0\n\nUSAGE:\n    lcars-monitor [--ascii] [--mono]\n    lcars-monitor --snapshot primary|secondary [--ascii]\n\nCONTROLS:\n    q / Esc  Quit\n    Space    Pause/resume\n    p        Toggle process diagnostics\n    Tab      Cycle views\n    ?        Help\n\nFLAGS:\n    --ascii    Disable Unicode glyphs\n    --mono     Use monochrome output (NO_COLOR is also honored)\n    --snapshot Render one 120x36 view to standard output"
                );
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("lcars-monitor 1.0.0");
                return Ok(None);
            }
            _ => return Err(format!("unrecognized argument '{argument}'; use --help")),
        }
    }
    Ok(Some(config))
}

fn snapshot(collector: &mut Collector, view: View, ascii: bool) -> Result<(), String> {
    // A baseline and twelve timed samples provide genuine CPU/network deltas
    // and a real waveform for captures; no synthetic metrics are substituted.
    let mut snapshot = collector.sample(view == View::Diagnostics);
    let mut history = History::default();
    history.push(snapshot.cpu.unwrap_or(0.0));
    for _ in 0..12 {
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        snapshot = collector.sample(view == View::Diagnostics);
        history.push(snapshot.cpu.unwrap_or(0.0));
    }
    let buffer = render::render(
        &snapshot,
        &history,
        view,
        120,
        36,
        RenderOptions {
            ascii,
            ..RenderOptions::default()
        },
    );
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(buffer.plain_text().as_bytes())
        .and_then(|()| stdout.write_all(b"\n"))
        .map_err(|error| format!("cannot write snapshot: {error}"))
}

fn interactive(mut collector: Collector, config: Config) -> Result<(), String> {
    let terminate = Arc::new(AtomicBool::new(false));
    for signal in [SIGINT, SIGTERM, SIGHUP] {
        signal_hook::flag::register(signal, Arc::clone(&terminate))
            .map_err(|error| format!("cannot install signal handler: {error}"))?;
    }

    let mut terminal = TerminalSession::new()
        .map_err(|error| format!("cannot enter terminal raw mode: {error}"))?;
    let mut renderer = DiffRenderer::new(ColorMode::detect(config.mono));
    let mut view = View::Primary;
    let mut options = RenderOptions {
        ascii: config.ascii,
        ..RenderOptions::default()
    };
    let mut history = History::default();
    let mut snapshot = collector.sample(false);
    history.push(snapshot.cpu.unwrap_or(0.0));
    let (mut width, mut height) = crossterm::terminal::size()
        .map_err(|error| format!("cannot read terminal size: {error}"))?;
    renderer
        .draw(
            terminal.stdout(),
            &render::render(&snapshot, &history, view, width, height, options),
        )
        .map_err(|error| format!("cannot draw interface: {error}"))?;
    let mut next_sample = Instant::now() + REFRESH_INTERVAL;

    while !terminate.load(Ordering::Relaxed) {
        let timeout = next_sample
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(100));
        if event::poll(timeout).map_err(|error| format!("cannot read terminal input: {error}"))? {
            match event::read().map_err(|error| format!("cannot decode terminal input: {error}"))? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if key.code == KeyCode::Esc
                        || key.code == KeyCode::Char('q')
                        || (key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL))
                    {
                        break;
                    }
                    match key.code {
                        KeyCode::Char(' ') => options.paused = !options.paused,
                        KeyCode::Char('?') => options.help = !options.help,
                        KeyCode::Char('p') | KeyCode::Tab => {
                            view = view.toggled();
                            options.help = false;
                            if !options.paused {
                                snapshot = collector.sample(view == View::Diagnostics);
                                history.push(snapshot.cpu.unwrap_or(0.0));
                                next_sample = Instant::now() + REFRESH_INTERVAL;
                            }
                        }
                        _ => {}
                    }
                }
                Event::Resize(new_width, new_height) => {
                    width = new_width;
                    height = new_height;
                    renderer.invalidate();
                }
                _ => {}
            }
            draw(
                &mut terminal,
                &mut renderer,
                &snapshot,
                &history,
                view,
                width,
                height,
                options,
            )?;
        }

        if !options.paused && Instant::now() >= next_sample {
            snapshot = collector.sample(view == View::Diagnostics);
            history.push(snapshot.cpu.unwrap_or(0.0));
            options.tick = options.tick.wrapping_add(1);
            next_sample = Instant::now() + REFRESH_INTERVAL;
            draw(
                &mut terminal,
                &mut renderer,
                &snapshot,
                &history,
                view,
                width,
                height,
                options,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn draw(
    terminal: &mut TerminalSession,
    renderer: &mut DiffRenderer,
    snapshot: &lcars_monitor::model::Snapshot,
    history: &History,
    view: View,
    width: u16,
    height: u16,
    options: RenderOptions,
) -> Result<(), String> {
    renderer
        .draw(
            terminal.stdout(),
            &render::render(snapshot, history, view, width, height, options),
        )
        .map_err(|error| format!("cannot update interface: {error}"))
}
