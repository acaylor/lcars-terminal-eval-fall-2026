//! Terminal layer: a terminal-independent cell buffer, colour handling,
//! glyph sets, and the diff renderer that turns two buffers into the minimal
//! ANSI byte stream. Nothing in here touches a real terminal except
//! `session`, which owns raw mode and restoration.

pub mod buffer;
pub mod color;
pub mod glyphs;
pub mod render;
pub mod session;

pub use buffer::{Buffer, Style};
pub use color::{Color, ColorMode};
pub use glyphs::Glyphs;
