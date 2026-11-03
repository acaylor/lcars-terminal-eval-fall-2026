//! The restrained LCARS-inspired palette: warm orange, amber, salmon,
//! lavender and cool blue, plus a few functional tones.

use crate::color::Color;

pub const ORANGE: Color = Color::rgb(255, 153, 0);
pub const AMBER: Color = Color::rgb(255, 196, 92);
pub const GOLD: Color = Color::rgb(255, 170, 51);
pub const SALMON: Color = Color::rgb(240, 132, 108);
pub const PEACH: Color = Color::rgb(255, 196, 160);
pub const LAVENDER: Color = Color::rgb(196, 150, 220);
pub const LILAC: Color = Color::rgb(160, 140, 230);
pub const BLUE: Color = Color::rgb(120, 150, 255);
pub const ICE: Color = Color::rgb(170, 210, 255);
pub const RED: Color = Color::rgb(232, 72, 56);

/// Text cut into colored shapes.
pub const INK: Color = Color::rgb(0, 0, 0);
/// Quiet labels and unavailable states.
pub const DIM: Color = Color::rgb(138, 118, 104);
/// Unfilled gauge tracks and trace baselines.
pub const TRACK: Color = Color::rgb(64, 48, 40);
pub const TRACK_COOL: Color = Color::rgb(44, 46, 72);
