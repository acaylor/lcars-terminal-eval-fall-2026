use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

use crate::canvas::Canvas;
use crate::collector::{create_platform_collector, MetricCollector};
use crate::model::SystemTelemetry;
use crate::sampler::TelemetrySampler;
use crate::terminal::{query_terminal_size, InputEvent, TerminalSession};
use crate::theme::{ColorMode, GlyphMode};
use crate::ui::{render_ui, UiState, ViewMode};

/// Coordinates metric sampling, state transitions, and double-buffered diff rendering.
pub struct AppController {
    collector: Box<dyn MetricCollector>,
    sampler: TelemetrySampler,
    pub state: UiState,
    pub telemetry: SystemTelemetry,
    last_sample_time: Option<Instant>,
    curr_canvas: Canvas,
    prev_canvas: Option<Canvas>,
    ansi_buf: String,
}

impl AppController {
    pub fn new(color_mode: ColorMode, glyph_mode: GlyphMode, refresh_ms: u64) -> Self {
        let (w, h) = query_terminal_size();
        let state = UiState {
            view: ViewMode::PrimaryCommand,
            show_help: false,
            paused: false,
            focus_index: 0,
            color_mode,
            glyph_mode,
            refresh_ms: refresh_ms.clamp(100, 10_000),
        };

        Self {
            collector: create_platform_collector(),
            sampler: TelemetrySampler::new(),
            curr_canvas: Canvas::new(w, h, glyph_mode),
            prev_canvas: None,
            ansi_buf: String::with_capacity(w * h * 2),
            telemetry: SystemTelemetry::default(),
            last_sample_time: None,
            state,
        }
    }

    /// Takes a fresh OS sample and computes rates against actual elapsed monotonic time.
    pub fn sample_now(&mut self) {
        let now = Instant::now();
        let dt = self
            .last_sample_time
            .map(|prev| now.saturating_duration_since(prev))
            .unwrap_or(Duration::ZERO);
        self.last_sample_time = Some(now);

        let include_procs = self.state.view == ViewMode::ProcessDiagnostics;
        let raw = self.collector.collect(include_procs);
        self.telemetry = self.sampler.ingest(raw, dt);
    }

    /// Applies an input event and returns `false` if the application should exit.
    /// Second return value indicates whether a fresh sample should be taken immediately.
    pub fn apply_event(&mut self, ev: InputEvent) -> (bool, bool) {
        match ev {
            InputEvent::Quit => {
                if self.state.show_help {
                    self.state.show_help = false;
                    (true, false)
                } else {
                    (false, false)
                }
            }
            InputEvent::ToggleHelp => {
                self.state.show_help = !self.state.show_help;
                (true, false)
            }
            InputEvent::TogglePause => {
                self.state.paused = !self.state.paused;
                (true, !self.state.paused)
            }
            InputEvent::ToggleProcesses => {
                self.state.view = self.state.view.toggle();
                let need_procs = self.state.view == ViewMode::ProcessDiagnostics;
                (true, need_procs)
            }
            InputEvent::CycleViewOrFocus => {
                self.state.focus_index = self.state.focus_index.wrapping_add(1);
                self.state.view = self.state.view.toggle();
                let need_procs = self.state.view == ViewMode::ProcessDiagnostics;
                (true, need_procs)
            }
            InputEvent::CycleColorMode => {
                self.state.color_mode = self.state.color_mode.cycle_next();
                self.prev_canvas = None;
                (true, false)
            }
            InputEvent::ToggleGlyphMode => {
                self.state.glyph_mode = self.state.glyph_mode.toggle();
                self.prev_canvas = None;
                (true, false)
            }
            InputEvent::Resize => {
                let (w, h) = query_terminal_size();
                self.curr_canvas
                    .resize_and_clear(w, h, self.state.glyph_mode);
                self.prev_canvas = None;
                (true, false)
            }
            InputEvent::None => (true, false),
        }
    }

    /// Renders the current UI state into `curr_canvas` and emits a minimal cell diff to `writer`.
    pub fn render_frame<W: Write>(&mut self, writer: &mut W) -> io::Result<usize> {
        let (w, h) = query_terminal_size();
        if w != self.curr_canvas.width
            || h != self.curr_canvas.height
            || self.curr_canvas.glyph_mode != self.state.glyph_mode
        {
            self.curr_canvas
                .resize_and_clear(w, h, self.state.glyph_mode);
            self.prev_canvas = None;
        }

        render_ui(&mut self.curr_canvas, &self.telemetry, &self.state);
        self.curr_canvas.render_diff(
            self.prev_canvas.as_ref(),
            self.state.color_mode,
            &mut self.ansi_buf,
        );

        writer.write_all(self.ansi_buf.as_bytes())?;
        writer.flush()?;
        self.prev_canvas = Some(self.curr_canvas.clone());
        Ok(self.ansi_buf.len())
    }

    /// Runs the interactive terminal event and rendering loop until exit or termination signal.
    pub fn run_interactive(&mut self) -> Result<(), String> {
        let mut session = TerminalSession::enter()?;
        let mut stdout = io::stdout();

        // Initial baseline + short warmup delta so first interactive frame has live CPU%
        self.sample_now();
        thread::sleep(Duration::from_millis(25));
        self.sample_now();
        self.render_frame(&mut stdout)
            .map_err(|e| format!("Terminal write error: {}", e))?;

        let refresh_interval = Duration::from_millis(self.state.refresh_ms);
        let mut next_tick = Instant::now() + refresh_interval;

        loop {
            let now = Instant::now();
            let wait_ms = next_tick
                .checked_duration_since(now)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
                .min(250);

            let ev = session.poll_event(wait_ms);
            if ev != InputEvent::None {
                let (keep_running, force_sample) = self.apply_event(ev);
                if !keep_running {
                    break;
                }
                if force_sample {
                    self.sample_now();
                }
                self.render_frame(&mut stdout)
                    .map_err(|e| format!("Terminal write error: {}", e))?;
            }

            if Instant::now() >= next_tick {
                if !self.state.paused {
                    self.sample_now();
                    self.render_frame(&mut stdout)
                        .map_err(|e| format!("Terminal write error: {}", e))?;
                }
                next_tick = Instant::now() + refresh_interval;
            }
        }

        session.restore();
        Ok(())
    }

    /// Renders a single headless snapshot at `(width, height)` using real OS metrics.
    pub fn capture_live_canvas(&mut self, view: ViewMode, width: usize, height: usize) -> Canvas {
        self.state.view = view;
        let include_procs = view == ViewMode::ProcessDiagnostics;
        let s1 = self.collector.collect(include_procs);
        let _ = self.sampler.ingest(s1, Duration::ZERO);
        thread::sleep(Duration::from_millis(40));
        let s2 = self.collector.collect(include_procs);
        self.telemetry = self.sampler.ingest(s2, Duration::from_millis(40));

        let mut canvas = Canvas::new(width, height, self.state.glyph_mode);
        render_ui(&mut canvas, &self.telemetry, &self.state);
        canvas
    }
}
