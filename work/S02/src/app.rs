//! Application state and the terminal-independent parts of the main loop:
//! key handling, sampling, and frame composition.

use crate::ansi::RenderMode;
use crate::canvas::Canvas;
use crate::clock::LocalTime;
use crate::color::ColorMode;
use crate::input::Key;
use crate::metrics::sampler::Sampler;
use crate::metrics::{Collector, SystemInfo};
use crate::ui::{Focus, Scene, Ui, View};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Suspend,
    /// Take a sample right away (resume, or process table requested).
    SampleNow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppState {
    pub view: View,
    pub focus: Focus,
    pub help: bool,
    pub paused: bool,
}

impl Default for AppState {
    fn default() -> Self {
        AppState { view: View::Command, focus: Focus::Cpu, help: false, paused: false }
    }
}

impl AppState {
    pub fn on_key(&mut self, key: Key) -> Action {
        match key {
            Key::Char('q') | Key::Char('Q') | Key::CtrlC => Action::Quit,
            Key::Esc if self.help => {
                self.help = false;
                Action::None
            }
            Key::Esc => Action::Quit,
            Key::CtrlZ => Action::Suspend,
            Key::Char('?') | Key::Char('h') | Key::Char('H') => {
                self.help = !self.help;
                Action::None
            }
            Key::Char(' ') => {
                self.paused = !self.paused;
                if self.paused {
                    Action::None
                } else {
                    Action::SampleNow
                }
            }
            Key::Char('p') | Key::Char('P') => {
                self.view = match self.view {
                    View::Command => View::Diagnostic,
                    View::Diagnostic => View::Command,
                };
                if self.view == View::Diagnostic && !self.paused {
                    Action::SampleNow
                } else {
                    Action::None
                }
            }
            Key::Tab => {
                match self.view {
                    View::Command => self.focus = self.focus.next(),
                    View::Diagnostic => self.view = View::Command,
                }
                Action::None
            }
            Key::BackTab => {
                match self.view {
                    View::Command => self.focus = self.focus.prev(),
                    View::Diagnostic => self.view = View::Command,
                }
                Action::None
            }
            _ => Action::None,
        }
    }
}

/// Owns collection, derived telemetry and rendering state.
pub struct Monitor {
    collector: Box<dyn Collector>,
    sampler: Sampler,
    pub sys: SystemInfo,
    pub state: AppState,
    ui: Ui,
    pub interval: Duration,
    pub mode: RenderMode,
    epoch: Instant,
}

impl Monitor {
    pub fn new(mut collector: Box<dyn Collector>, interval: Duration, mode: RenderMode) -> Monitor {
        let sys = collector.system_info();
        Monitor { collector, sampler: Sampler::new(), sys, state: AppState::default(), ui: Ui::new(), interval, mode, epoch: Instant::now() }
    }

    /// Sample now. The process table is only scanned while it is visible,
    /// which keeps the default view cheap.
    pub fn sample(&mut self) {
        let want_procs = self.state.view == View::Diagnostic;
        let raw = self.collector.sample(want_procs);
        self.sampler.ingest(raw, self.epoch.elapsed());
        if want_procs {
            let pids: Vec<u32> = match &self.sampler.telemetry().procs {
                Some(Ok(p)) => p.top.iter().map(|t| t.pid).collect(),
                _ => vec![],
            };
            for pid in pids {
                if let Some(name) = self.collector.full_name(pid) {
                    self.sampler.rename_process(pid, name);
                }
            }
        }
    }

    pub fn mode_label(&self) -> String {
        let c = match self.mode.color {
            ColorMode::TrueColor => "TRUECOLOR",
            ColorMode::Ansi256 => "256 COLOR",
            ColorMode::Ansi16 => "16 COLOR",
            ColorMode::Mono => "MONO",
        };
        format!("{} · {}", c, if self.mode.ascii { "ASCII" } else { "UNICODE" })
    }

    /// Render the current state into `canvas`.
    pub fn compose(&mut self, canvas: &mut Canvas, time: &LocalTime) {
        let label = self.mode_label();
        let scene = Scene {
            tel: self.sampler.telemetry(),
            history: &self.sampler.history,
            sys: &self.sys,
            time,
            view: self.state.view,
            focus: self.state.focus,
            help: self.state.help,
            paused: self.state.paused,
            phase: self.epoch.elapsed().as_secs_f32(),
            interval: self.interval,
            mode_label: &label,
        };
        self.ui.render(canvas, &scene);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_controls() {
        let mut s = AppState::default();
        assert_eq!(s.on_key(Key::Char('?')), Action::None);
        assert!(s.help);
        assert_eq!(s.on_key(Key::Esc), Action::None, "Esc closes help first");
        assert!(!s.help);
        assert_eq!(s.on_key(Key::Char(' ')), Action::None);
        assert!(s.paused);
        assert_eq!(s.on_key(Key::Char(' ')), Action::SampleNow);
        assert_eq!(s.on_key(Key::Char('p')), Action::SampleNow);
        assert_eq!(s.view, View::Diagnostic);
        assert_eq!(s.on_key(Key::Char('p')), Action::None);
        assert_eq!(s.view, View::Command);
        s.on_key(Key::Tab);
        assert_eq!(s.focus, Focus::Memory);
        s.on_key(Key::BackTab);
        assert_eq!(s.focus, Focus::Cpu);
        assert_eq!(s.on_key(Key::Char('q')), Action::Quit);
        assert_eq!(s.on_key(Key::Esc), Action::Quit);
        assert_eq!(s.on_key(Key::CtrlC), Action::Quit);
    }

    #[test]
    fn tab_from_diagnostic_returns_to_command() {
        let mut s = AppState { view: View::Diagnostic, ..AppState::default() };
        s.on_key(Key::Tab);
        assert_eq!(s.view, View::Command);
        assert_eq!(s.focus, Focus::Cpu);
    }
}
