//! lcarsmon — an LCARS-inspired terminal system monitor.
//!
//! Layering (each layer only depends on the ones above it):
//! - `metrics`: OS collectors (Linux /proc, macOS Mach/sysctl/libproc) and
//!   the deterministic `Sampler` that derives rates.
//! - `canvas`, `raster`, `color`: an in-memory cell grid and sub-cell rasters.
//! - `ui`: pure rendering of a `Scene` into a canvas.
//! - `ansi`: diffing canvases into minimal escape sequences.
//! - `term`, `run`: the only code that touches the real terminal.

pub mod ansi;
pub mod app;
pub mod canvas;
pub mod clock;
pub mod color;
pub mod format;
pub mod input;
pub mod metrics;
pub mod raster;
pub mod run;
pub mod term;
pub mod ui;
