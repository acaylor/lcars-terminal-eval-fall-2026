//! The interactive event loop and the one-shot snapshot renderer.

use crate::ansi::{encode_diff, to_ansi_lines, RenderMode};
use crate::app::{Action, Monitor};
use crate::canvas::Canvas;
use crate::clock::LocalTime;
use crate::input::decode;
use crate::metrics::platform_collector;
use crate::term::{self, Signal, Signals, Terminal};
use crate::ui::View;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Animation cadence. Frames are aligned to wall-clock multiples of this so
/// the clock's seconds digit flips promptly.
const FRAME_MS: u64 = 200;
/// The first rates appear after this delay instead of a full interval.
const WARMUP: Duration = Duration::from_millis(300);

pub struct Options {
    pub interval: Duration,
    pub mode: RenderMode,
    /// Exit automatically after this long (for measurement scripts).
    pub exit_after: Option<Duration>,
}

fn ms_to_next_frame() -> u64 {
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    FRAME_MS - now_ms % FRAME_MS + 1
}

/// Double-buffered canvases: `back` is drawn, diffed against `front`, then swapped.
struct Screen {
    front: Option<Canvas>,
    back: Canvas,
    out: Vec<u8>,
}

impl Screen {
    fn new(size: (u16, u16)) -> Screen {
        Screen { front: None, back: Canvas::new(size.0, size.1), out: Vec::with_capacity(64 * 1024) }
    }
    fn resize(&mut self, size: (u16, u16)) {
        if self.back.size() != size {
            self.back = Canvas::new(size.0, size.1);
        }
        self.front = None; // force a full repaint
    }
    fn present(&mut self, mon: &mut Monitor, term: &mut Terminal) -> std::io::Result<()> {
        mon.compose(&mut self.back, &LocalTime::now());
        self.out.clear();
        encode_diff(self.front.as_ref(), &self.back, mon.mode, &mut self.out);
        term.write(&self.out)?;
        match self.front.as_mut() {
            Some(f) if f.size() == self.back.size() => std::mem::swap(f, &mut self.back),
            _ => self.front = Some(self.back.clone()),
        }
        Ok(())
    }
}

pub fn run(opts: Options) -> Result<(), String> {
    term::check_tty()?;
    let started = Instant::now();
    let signals = Signals::install().map_err(|e| format!("cannot install signal handlers: {e}"))?;
    let mut term = Terminal::enter().map_err(|e| format!("cannot switch the terminal to raw mode: {e}"))?;
    let mut mon = Monitor::new(platform_collector(), opts.interval, opts.mode);
    mon.sample();
    let mut next_sample = Instant::now() + WARMUP.min(opts.interval);
    let mut screen = Screen::new(term::size());
    let mut input = [0u8; 256];

    loop {
        screen.present(&mut mon, &mut term).map_err(|e| format!("terminal write failed: {e}"))?;
        if opts.exit_after.is_some_and(|d| started.elapsed() >= d) {
            return Ok(());
        }

        let now = Instant::now();
        let mut timeout = ms_to_next_frame();
        if !mon.state.paused {
            timeout = timeout.min(next_sample.saturating_duration_since(now).as_millis() as u64);
        }
        let ready = term::wait(signals.fd(), timeout as i32);

        if ready.signal {
            for sig in signals.drain() {
                match sig {
                    Signal::Resize | Signal::Continue => screen.resize(term::size()),
                    Signal::Terminate => return Ok(()),
                    Signal::Stop => {
                        term.suspend().map_err(|e| format!("cannot resume terminal: {e}"))?;
                        screen.resize(term::size());
                    }
                }
            }
        }
        if ready.hangup && !ready.input {
            return Ok(());
        }
        if ready.input {
            let n = term::read_input(&mut input).map_err(|e| format!("reading keyboard input failed: {e}"))?;
            for key in decode(&input[..n]) {
                match mon.state.on_key(key) {
                    Action::Quit => return Ok(()),
                    Action::Suspend => {
                        term.suspend().map_err(|e| format!("cannot resume terminal: {e}"))?;
                        screen.resize(term::size());
                    }
                    Action::SampleNow => {
                        mon.sample();
                        next_sample = Instant::now() + WARMUP.min(opts.interval);
                    }
                    Action::None => {}
                }
            }
        }
        let now = Instant::now();
        if !mon.state.paused && now >= next_sample {
            mon.sample();
            next_sample += opts.interval;
            if next_sample <= now {
                // We fell behind (suspend, slow terminal): don't burst-sample.
                next_sample = now + opts.interval;
            }
        }
    }
}

/// Render one frame without a terminal: two samples `gap` apart so rates
/// are real, then print plain text or ANSI.
pub fn snapshot(view: View, size: (u16, u16), mode: RenderMode, ansi: bool, gap: Duration) -> String {
    let mut mon = Monitor::new(platform_collector(), gap, mode);
    mon.state.view = view;
    mon.sample();
    std::thread::sleep(gap);
    mon.sample();
    let mut canvas = Canvas::new(size.0, size.1);
    mon.compose(&mut canvas, &LocalTime::now());
    if ansi {
        to_ansi_lines(&canvas, mode)
    } else if mode.ascii {
        let mut out = String::new();
        for y in 0..canvas.height() {
            let line: String = canvas.row(y).iter().map(|&c| crate::ansi::ascii_cell(c).ch).collect();
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    } else {
        canvas.to_plain_text()
    }
}
