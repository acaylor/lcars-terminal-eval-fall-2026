//! Presentation. Everything here draws into a [`Canvas`] from an immutable
//! [`Scene`]; nothing reads the OS or the terminal, so views can be rendered
//! and inspected in tests at any size.

pub mod diagnostic;
pub mod font;
pub mod help;
pub mod layout;
pub mod primary;
pub mod shapes;
pub mod theme;
pub mod widgets;

use crate::canvas::Canvas;
use crate::clock::LocalTime;
use crate::color::Color;
use crate::metrics::history::History;
use crate::metrics::sampler::Telemetry;
use crate::metrics::SystemInfo;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Command,
    Diagnostic,
}

/// Which subsystem the command view's large trace follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Cpu,
    Memory,
    Network,
    Load,
}

impl Focus {
    pub const ALL: [Focus; 4] = [Focus::Cpu, Focus::Memory, Focus::Network, Focus::Load];

    pub fn next(self) -> Focus {
        let i = Focus::ALL.iter().position(|&f| f == self).unwrap_or(0);
        Focus::ALL[(i + 1) % Focus::ALL.len()]
    }
    pub fn prev(self) -> Focus {
        let i = Focus::ALL.iter().position(|&f| f == self).unwrap_or(0);
        Focus::ALL[(i + Focus::ALL.len() - 1) % Focus::ALL.len()]
    }
    pub fn name(self) -> &'static str {
        match self {
            Focus::Cpu => "CPU",
            Focus::Memory => "MEM",
            Focus::Network => "NET",
            Focus::Load => "LOAD",
        }
    }
}

/// Everything a frame depends on.
pub struct Scene<'a> {
    pub tel: &'a Telemetry,
    pub history: &'a History,
    pub sys: &'a SystemInfo,
    pub time: &'a LocalTime,
    pub view: View,
    pub focus: Focus,
    pub help: bool,
    pub paused: bool,
    /// Seconds since start; drives animation phase.
    pub phase: f32,
    pub interval: Duration,
    /// Short description of the active render mode, for the help panel.
    pub mode_label: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Condition {
    Nominal,
    Elevated,
    Alert,
}

impl Condition {
    pub fn label(self) -> &'static str {
        match self {
            Condition::Nominal => "NOMINAL",
            Condition::Elevated => "ELEVATED",
            Condition::Alert => "RED ALERT",
        }
    }
    pub fn color(self) -> Color {
        match self {
            Condition::Nominal => theme::BLUE,
            Condition::Elevated => theme::AMBER,
            Condition::Alert => theme::RED,
        }
    }
}

/// Overall status derived from real readings only; unavailable metrics
/// simply don't contribute.
pub fn condition(tel: &Telemetry) -> Condition {
    let mut c = Condition::Nominal;
    let mut raise = |level: Condition| c = c.max(level);
    if let Ok(cpu) = tel.cpu_pct {
        if cpu >= 95.0 {
            raise(Condition::Alert)
        } else if cpu >= 75.0 {
            raise(Condition::Elevated)
        }
    }
    if let Ok(m) = &tel.memory {
        let f = m.used_fraction();
        if f >= 0.95 {
            raise(Condition::Alert)
        } else if f >= 0.85 {
            raise(Condition::Elevated)
        }
    }
    if let Ok(d) = &tel.disk {
        let f = d.used_fraction();
        if f >= 0.97 {
            raise(Condition::Alert)
        } else if f >= 0.90 {
            raise(Condition::Elevated)
        }
    }
    if let (Ok(l), Ok(cores)) = (&tel.load, &tel.core_pct) {
        let per_core = l[0] / cores.len().max(1) as f64;
        if per_core >= 2.0 {
            raise(Condition::Alert)
        } else if per_core >= 1.0 {
            raise(Condition::Elevated)
        }
    }
    c
}

/// Smooth 0..1 pulse at `hz`.
pub fn pulse(phase: f32, hz: f32) -> f32 {
    ((phase * hz * std::f32::consts::TAU).sin() + 1.0) / 2.0
}

/// Square-wave blink at `hz`.
pub fn blink(phase: f32, hz: f32) -> bool {
    (phase * hz).fract() < 0.5
}

/// Renders scenes; owns caches that persist between frames.
#[derive(Default)]
pub struct Ui {
    font: font::Font,
}

impl Ui {
    pub fn new() -> Ui {
        Ui::default()
    }

    pub fn render(&mut self, canvas: &mut Canvas, scene: &Scene) {
        canvas.clear();
        match scene.view {
            View::Command => primary::render(canvas, scene, &mut self.font),
            View::Diagnostic => diagnostic::render(canvas, scene, &mut self.font),
        }
        if scene.help {
            help::render(canvas, scene);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{Disk, Memory, Missing};

    #[test]
    fn focus_cycles_both_ways() {
        let mut f = Focus::Cpu;
        for _ in 0..4 {
            f = f.next();
        }
        assert_eq!(f, Focus::Cpu);
        assert_eq!(Focus::Cpu.prev(), Focus::Load);
    }

    #[test]
    fn condition_levels() {
        let mut t = Telemetry::default();
        assert_eq!(condition(&t), Condition::Nominal, "no data is not an alert");
        t.cpu_pct = Ok(80.0);
        assert_eq!(condition(&t), Condition::Elevated);
        t.memory = Ok(Memory { total: 100, available: 2 });
        assert_eq!(condition(&t), Condition::Alert);
        t.memory = Err(Missing::Failed);
        t.disk = Ok(Disk { mount: "/".into(), total: 100, used: 91, available: 9 });
        assert_eq!(condition(&t), Condition::Elevated);
    }
}
