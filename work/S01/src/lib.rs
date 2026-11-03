//! lcars: an LCARS-inspired terminal system monitor.
//!
//! Layering, bottom to top:
//! * `metrics` - OS collectors behind the `Collector` trait, plus the pure
//!   `Sampler` that turns raw counters into rates and histories.
//! * `term` - cell buffer, colour quantisation, glyph sets, diff renderer
//!   and the terminal session (raw mode + restoration).
//! * `ui` - the LCARS compositions, drawn into a buffer from a `Scene`.
//! * `app` - state machine and main loop tying the layers together.

pub mod app;
pub mod metrics;
pub mod term;
pub mod ui;
