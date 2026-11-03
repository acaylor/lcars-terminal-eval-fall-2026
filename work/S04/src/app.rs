use crate::metrics::Collector;
use crate::render::{self, detect_color, detect_unicode, Buffer, View, ViewState};
use crate::term::{Key, Term};
use std::time::{Duration, Instant};

const UI_TICK: Duration = Duration::from_millis(1000);
const SAMPLE_EVERY: u32 = 1;

pub fn run(term: &mut Term) -> Result<(), String> {
    let mut collector = Collector::new();
    let mut ui = ViewState::new(detect_color(), detect_unicode());
    let mut sample = collector.sample(false);
    let mut prev = Buffer::new(1, 1);
    let mut last_sample = Instant::now();
    let mut force = true;
    loop {
        if term.quit_requested() {
            break;
        }
        let (w, h) = term.size();
        if term.take_resize() || prev.width != w || prev.height != h {
            force = true;
        }
        let due = last_sample.elapsed() >= UI_TICK * SAMPLE_EVERY;
        if !ui.paused && due {
            let want_proc = ui.view == View::Diag;
            sample = collector.sample(want_proc);
            if let Some(cpu) = sample.cpu_pct {
                ui.cpu_trace.push(cpu);
                if ui.cpu_trace.len() > 64 {
                    ui.cpu_trace.remove(0);
                }
            }
            last_sample = Instant::now();
        }
        ui.frame = ui.frame.wrapping_add(1);
        let next = render::render(&sample, &ui, w, h);
        let bytes = render::diff_rows(&prev, &next, ui.color, force);
        if !bytes.is_empty() {
            term.write(&bytes);
        }
        prev = next;
        force = false;
        let wait = if ui.paused {
            250
        } else {
            UI_TICK.as_millis() as i32
        };
        if let Some(key) = term.poll_key(wait) {
            if !handle_key(key, &mut ui, &mut collector, &mut sample) {
                break;
            }
            force = false;
        }
    }
    Ok(())
}

fn handle_key(
    key: Key,
    ui: &mut ViewState,
    collector: &mut Collector,
    sample: &mut crate::metrics::Sample,
) -> bool {
    match key {
        Key::Quit => false,
        Key::Help => {
            ui.help = !ui.help;
            true
        }
        Key::Pause => {
            ui.paused = !ui.paused;
            true
        }
        Key::Diag => {
            ui.view = if ui.view == View::Diag {
                View::Primary
            } else {
                View::Diag
            };
            ui.focus = 0;
            if ui.view == View::Diag {
                *sample = collector.sample(true);
            }
            true
        }
        Key::Tab => {
            ui.cycle_focus();
            true
        }
    }
}
