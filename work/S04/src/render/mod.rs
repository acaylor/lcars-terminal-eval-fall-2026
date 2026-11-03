mod buffer;
mod color;
mod draw;
mod font;
mod view;

use crate::metrics::Sample;
pub use buffer::{diff_rows, Buffer, Cell};
pub use color::{detect_color, detect_unicode, ColorMode, Rgb, BLACK};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Primary,
    Diag,
}

#[derive(Clone, Debug)]
pub struct ViewState {
    pub view: View,
    pub help: bool,
    pub paused: bool,
    pub focus: usize,
    pub frame: u64,
    pub cpu_trace: Vec<f64>,
    pub color: ColorMode,
    pub unicode: bool,
}

impl ViewState {
    pub fn new(color: ColorMode, unicode: bool) -> Self {
        Self {
            view: View::Primary,
            help: false,
            paused: false,
            focus: 0,
            frame: 0,
            cpu_trace: Vec::new(),
            color,
            unicode,
        }
    }

    pub fn focus_count(&self) -> usize {
        5
    }

    pub fn cycle_focus(&mut self) {
        let n = self.focus_count();
        self.focus = (self.focus + 1) % n;
    }
}

pub fn render(sample: &Sample, ui: &ViewState, width: u16, height: u16) -> Buffer {
    let mut buf = Buffer::new(width, height);
    view::paint(&mut buf, sample, ui);
    buf
}

pub fn shape_map(sample: &Sample, ui: &ViewState, width: u16, height: u16) -> String {
    render(sample, ui, width, height).shape_map()
}
