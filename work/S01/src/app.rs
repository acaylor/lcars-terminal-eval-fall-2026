//! Application state and the main loop, split into small steps: sample,
//! render, wait, handle input. Nothing here knows how a view is drawn.

use crate::metrics::rates::CounterWidth;
use crate::metrics::sampler::Sampler;
use crate::metrics::{Collector, StaticInfo};
use crate::term::render;
use crate::term::session::Session;
use crate::term::{Buffer, ColorMode, Glyphs};
use crate::ui::{self, Clock, Scene, View};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::io::Write;
use std::time::{Duration, Instant};

/// Animation/clock cadence. Metrics are sampled on their own interval; this
/// only drives the seconds display, the pulse and the top-bar scanner.
const FRAME: Duration = Duration::from_millis(250);

pub struct Options {
    pub interval: Duration,
    pub color_mode: ColorMode,
    pub glyphs: Glyphs,
    pub view: View,
    /// Quit automatically after this long (measurement aid); zero means
    /// "right after the first frame".
    pub exit_after: Option<Duration>,
}

pub struct App {
    collector: Box<dyn Collector>,
    sampler: Sampler,
    info: StaticInfo,
    view: View,
    help: bool,
    paused: bool,
    frame: u64,
    opts: Options,
    next_sample: Instant,
    dirty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Continue,
    Quit,
}

impl App {
    pub fn new(collector: Box<dyn Collector>, opts: Options) -> App {
        let info = collector.static_info();
        // Both collectors read 64-bit counters (/proc/net/dev, NET_RT_IFLIST2).
        let width = CounterWidth::Bits64;
        App {
            collector,
            sampler: Sampler::new(width),
            info,
            view: opts.view,
            help: false,
            paused: false,
            frame: 0,
            next_sample: Instant::now(),
            dirty: true,
            opts,
        }
    }

    /// Take a reading now and fold it into the telemetry.
    pub fn sample(&mut self) {
        let snap = self.collector.sample(self.view == View::Diagnostic);
        self.sampler.push(snap);
        self.next_sample = Instant::now() + self.opts.interval;
        self.dirty = true;
    }

    /// Apply one key press. Pure state transition; returns whether to quit.
    pub fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) -> Action {
        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Action::Quit,
            KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => return Action::Quit,
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Char(' ') => {
                self.paused = !self.paused;
                if !self.paused {
                    // Resume with a fresh reading rather than waiting a full interval.
                    self.next_sample = Instant::now();
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => self.set_view(if self.view == View::Diagnostic {
                View::Command
            } else {
                View::Diagnostic
            }),
            KeyCode::Tab | KeyCode::BackTab => self.set_view(self.view.next()),
            _ => return Action::Continue,
        }
        self.dirty = true;
        Action::Continue
    }

    fn set_view(&mut self, v: View) {
        if v != self.view {
            self.view = v;
            if v == View::Diagnostic && !self.paused {
                // The process table is only scanned while visible, so fetch it now.
                self.next_sample = Instant::now();
            }
        }
    }

    pub fn draw(&self, buf: &mut Buffer, warnings: &[String]) {
        let scene = Scene {
            telemetry: self.sampler.telemetry(),
            info: &self.info,
            clock: &local_clock(),
            view: self.view,
            help: self.help,
            paused: self.paused,
            frame: self.frame,
            color_mode: self.opts.color_mode,
            interval_ms: self.opts.interval.as_millis() as u64,
            warnings,
        };
        ui::draw(buf, &self.opts.glyphs, &scene);
    }

    pub fn view(&self) -> View {
        self.view
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn help(&self) -> bool {
        self.help
    }

    /// Run on the live terminal until quit. Returns the time of the first
    /// rendered frame relative to `start` for the startup measurement.
    pub fn run(&mut self, start: Instant) -> Result<Duration, String> {
        let session = Session::open()?;
        let mut out = std::io::stdout();
        let (mut w, mut h) = Session::size();
        let mut prev: Option<Buffer> = None;
        let mut next = Buffer::new(w, h);
        let mut ansi = String::with_capacity(w * h * 12);
        let mut first_frame: Option<Duration> = None;
        let mut next_frame = Instant::now();
        let deadline = self.opts.exit_after.map(|d| start + d);
        self.sample();
        loop {
            if Session::terminate_requested() {
                break;
            }
            let now = Instant::now();
            if let Some(d) = deadline {
                if now >= d && first_frame.is_some() {
                    break;
                }
            }
            if !self.paused && now >= self.next_sample {
                self.sample();
            }
            if now >= next_frame || self.dirty {
                self.frame += 1;
                let warnings = self.collector.warnings();
                self.draw(&mut next, &warnings);
                ansi.clear();
                let full = prev.is_none();
                render::diff(prev.as_ref(), &next, self.opts.color_mode, full, &mut ansi);
                out.write_all(ansi.as_bytes())
                    .map_err(|e| format!("write to terminal failed: {e}"))?;
                out.flush().map_err(|e| format!("flush failed: {e}"))?;
                match prev.as_mut() {
                    Some(p) => std::mem::swap(p, &mut next),
                    None => prev = Some(next.clone()),
                }
                self.dirty = false;
                first_frame.get_or_insert_with(|| start.elapsed());
                next_frame = now + FRAME;
            }
            // Sleep until whichever comes first: next animation frame, next
            // sample, or a key/resize event.
            let mut wake = next_frame;
            if !self.paused {
                wake = wake.min(self.next_sample);
            }
            if let Some(d) = deadline {
                wake = wake.min(d);
            }
            let timeout = wake.saturating_duration_since(Instant::now());
            match event::poll(timeout) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(k)) if k.kind != KeyEventKind::Release => {
                        if self.handle_key(k.code, k.modifiers) == Action::Quit {
                            break;
                        }
                    }
                    Ok(Event::Resize(nw, nh)) => {
                        (w, h) = (nw as usize, nh as usize);
                        next = Buffer::new(w, h);
                        prev = None; // forces one full repaint at the new size
                        let _ = out.write_all(b"\x1b[2J");
                        self.dirty = true;
                    }
                    Ok(_) => {}
                    Err(e) => return Err(format!("reading terminal input failed: {e}")),
                },
                Ok(false) => {}
                Err(e) => return Err(format!("polling terminal input failed: {e}")),
            }
        }
        drop(session);
        Ok(first_frame.unwrap_or_default())
    }
}

/// Local wall-clock time through `localtime_r`, honouring TZ and
/// /etc/localtime on both platforms without any date-formatting dependency.
pub fn local_clock() -> Clock {
    const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
    const MONTHS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&now, &mut tm) }.is_null() {
        return Clock {
            weekday: "---",
            month: "---",
            ..Clock::default()
        };
    }
    Clock {
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
        second: tm.tm_sec as u32,
        weekday: DAYS[(tm.tm_wday.clamp(0, 6)) as usize],
        day: tm.tm_mday as u32,
        month: MONTHS[(tm.tm_mon.clamp(0, 11)) as usize],
        year: tm.tm_year + 1900,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::Snapshot;

    struct FakeCollector {
        calls: std::cell::Cell<u32>,
    }
    impl Collector for FakeCollector {
        fn static_info(&self) -> StaticInfo {
            StaticInfo {
                hostname: "test".into(),
                cores: 2,
                ..Default::default()
            }
        }
        fn sample(&mut self, _want_processes: bool) -> Snapshot {
            self.calls.set(self.calls.get() + 1);
            Snapshot::empty(Instant::now())
        }
        fn warnings(&self) -> Vec<String> {
            vec![]
        }
    }

    fn app() -> App {
        App::new(
            Box::new(FakeCollector {
                calls: std::cell::Cell::new(0),
            }),
            Options {
                interval: Duration::from_secs(1),
                color_mode: ColorMode::TrueColor,
                glyphs: Glyphs::unicode(),
                view: View::Command,
                exit_after: None,
            },
        )
    }

    #[test]
    fn keys_drive_state() {
        let mut a = app();
        assert_eq!(a.handle_key(KeyCode::Char('?'), KeyModifiers::NONE), Action::Continue);
        assert!(a.help());
        a.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert!(a.paused());
        a.handle_key(KeyCode::Char('p'), KeyModifiers::NONE);
        assert_eq!(a.view(), View::Diagnostic);
        a.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(a.view(), View::Command);
        a.handle_key(KeyCode::Char('p'), KeyModifiers::NONE);
        a.handle_key(KeyCode::Char('p'), KeyModifiers::NONE);
        assert_eq!(a.view(), View::Command);
        assert_eq!(a.handle_key(KeyCode::Char('x'), KeyModifiers::NONE), Action::Continue);
        assert_eq!(a.handle_key(KeyCode::Esc, KeyModifiers::NONE), Action::Quit);
        assert_eq!(a.handle_key(KeyCode::Char('q'), KeyModifiers::NONE), Action::Quit);
        assert_eq!(a.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL), Action::Quit);
    }

    #[test]
    fn draw_without_terminal() {
        let mut a = app();
        a.sample();
        let mut b = Buffer::new(120, 36);
        a.draw(&mut b, &[]);
        assert!(b.contains_text("TEST"));
        assert!(!b.contains_text("PROCESSES"));
        a.handle_key(KeyCode::Char('p'), KeyModifiers::NONE);
        a.draw(&mut b, &[]);
        assert!(b.contains_text("PROCESSES"));
    }

    #[test]
    fn clock_is_sane() {
        let c = local_clock();
        assert!(c.hour < 24 && c.minute < 60 && c.second < 61);
        assert!(c.year >= 2024);
        assert_eq!(c.time().len(), 8);
    }
}
