//! Presentation. `draw` takes a `Scene` (pure data) and paints a `Buffer`,
//! which is what makes every view testable without a terminal.

pub mod diag;
pub mod font;
pub mod frame;
pub mod help;
pub mod primary;
pub mod widgets;

use crate::metrics::sampler::Telemetry;
use crate::metrics::StaticInfo;
use crate::term::{Buffer, Color, ColorMode, Glyphs};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Command,
    Diagnostic,
}

impl View {
    pub fn next(self) -> View {
        match self {
            View::Command => View::Diagnostic,
            View::Diagnostic => View::Command,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            View::Command => "CMD 01",
            View::Diagnostic => "DIAG 02",
        }
    }
}

/// Wall-clock fields, computed by the app so views stay free of libc.
#[derive(Clone, Debug, Default)]
pub struct Clock {
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub weekday: &'static str,
    pub day: u32,
    pub month: &'static str,
    pub year: i32,
}

impl Clock {
    pub fn time(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
    pub fn date(&self) -> String {
        format!("{} {:02} {} {}", self.weekday, self.day, self.month, self.year)
    }
}

pub struct Scene<'a> {
    pub telemetry: &'a Telemetry,
    pub info: &'a StaticInfo,
    pub clock: &'a Clock,
    pub view: View,
    pub help: bool,
    pub paused: bool,
    /// Animation counter, advanced once per rendered frame.
    pub frame: u64,
    pub color_mode: ColorMode,
    pub interval_ms: u64,
    pub warnings: &'a [String],
}

/// Size class. Only the composition changes between tiers; the data does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Tiny,
    Compact,
    Standard,
}

impl Tier {
    pub fn for_size(w: usize, h: usize) -> Tier {
        if w < 60 || h < 16 {
            Tier::Tiny
        } else if w < 100 || h < 30 {
            Tier::Compact
        } else {
            Tier::Standard
        }
    }
    pub fn rail_width(self) -> i32 {
        match self {
            Tier::Tiny => 5,
            Tier::Compact => 8,
            Tier::Standard => 12,
        }
    }
}

/// System status derived from CPU and load, used by the focal readout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Calibrating,
    Nominal,
    Elevated,
    Critical,
}

impl Status {
    pub fn derive(t: &Telemetry, cores: usize) -> Status {
        let Some(cpu) = t.cpu else { return Status::Calibrating };
        let load_ratio = t.load.map(|l| l[0] / cores.max(1) as f64).unwrap_or(0.0);
        let mem_ratio = t.memory.map(|m| m.used as f64 / m.total.max(1) as f64).unwrap_or(0.0);
        if cpu >= 0.9 || load_ratio >= 2.0 || mem_ratio >= 0.95 {
            Status::Critical
        } else if cpu >= 0.6 || load_ratio >= 1.0 || mem_ratio >= 0.85 {
            Status::Elevated
        } else {
            Status::Nominal
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Status::Calibrating => "CALIBRATING",
            Status::Nominal => "NOMINAL",
            Status::Elevated => "ELEVATED",
            Status::Critical => "CRITICAL",
        }
    }
    pub fn color(self) -> Color {
        match self {
            Status::Calibrating => Color::LAVENDER,
            Status::Nominal => Color::BLUE,
            Status::Elevated => Color::AMBER,
            Status::Critical => Color::RED,
        }
    }
}

pub fn draw(buf: &mut Buffer, g: &Glyphs, scene: &Scene) {
    buf.clear();
    let tier = Tier::for_size(buf.width(), buf.height());
    match scene.view {
        View::Command => primary::draw(buf, g, scene, tier),
        View::Diagnostic => diag::draw(buf, g, scene, tier),
    }
    if scene.help {
        help::draw(buf, g, scene);
    }
}

/// Bottom-bar key hints shared by both views; discoverability lives in the frame.
pub fn key_segments(scene: &Scene, tier: Tier) -> Vec<frame::Segment> {
    use frame::Segment;
    let hold = if scene.paused { "SPC RESUME" } else { "SPC HOLD" };
    let proc_label = if scene.view == View::Diagnostic {
        "P CLOSE"
    } else {
        "P PROC"
    };
    match tier {
        Tier::Tiny => vec![
            Segment::new(Color::ORANGE, "", tier.rail_width() + 2),
            Segment::new(Color::LAVENDER, "Q ? SPC P TAB", 0),
        ],
        Tier::Compact => vec![
            Segment::new(Color::ORANGE, "", tier.rail_width() + 6),
            Segment::new(Color::LAVENDER, "Q QUIT", 10),
            Segment::new(Color::SALMON, "? HELP", 10),
            Segment::new(Color::BLUE, hold, 14),
            Segment::new(Color::LAVENDER, proc_label, 11),
            Segment::new(Color::AMBER, "TAB VIEW", 0),
        ],
        Tier::Standard => vec![
            Segment::new(Color::ORANGE, "", tier.rail_width() + 14),
            Segment::new(Color::LAVENDER, "Q QUIT", 12),
            Segment::new(Color::SALMON, "? HELP", 12),
            Segment::new(Color::BLUE, hold, 16),
            Segment::new(Color::LAVENDER, proc_label, 13),
            Segment::new(Color::AMBER, "TAB VIEW", 14),
            Segment::new(Color::DEEP_BLUE, "", 0),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers() {
        assert_eq!(Tier::for_size(120, 36), Tier::Standard);
        assert_eq!(Tier::for_size(80, 24), Tier::Compact);
        assert_eq!(Tier::for_size(100, 29), Tier::Compact);
        assert_eq!(Tier::for_size(40, 10), Tier::Tiny);
        assert_eq!(Tier::for_size(0, 0), Tier::Tiny);
    }

    #[test]
    fn status_thresholds() {
        let mut t = crate::metrics::sampler::Sampler::new(crate::metrics::rates::CounterWidth::Bits64)
            .telemetry()
            .clone();
        assert_eq!(Status::derive(&t, 4), Status::Calibrating);
        t.cpu = Some(0.2);
        assert_eq!(Status::derive(&t, 4), Status::Nominal);
        t.load = Some([6.0, 0.0, 0.0]);
        assert_eq!(Status::derive(&t, 4), Status::Elevated);
        t.cpu = Some(0.95);
        assert_eq!(Status::derive(&t, 4), Status::Critical);
    }
}
