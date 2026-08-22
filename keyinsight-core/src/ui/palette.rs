//! The macOS system colors the SwiftUI views reference by name
//! (`Color.red`, `.orange`, `.blue`, `.green`) plus KeyInSight's own
//! literals, so every widget resolves the same ink.

use agg_gui::color::Color;

/// `Color.red` — error counts, wrong-note flashes.
pub const RED: Color = Color::from_rgb8(0xFF, 0x3B, 0x30);
/// `Color.orange` — streaks, trouble spots, uncertain-pitch notices.
pub const ORANGE: Color = Color::from_rgb8(0xFF, 0x95, 0x00);
/// `Color.blue` — octave-follow notices, unlocks, count-in.
pub const BLUE: Color = Color::from_rgb8(0x00, 0x7A, 0xFF);
/// `Color.accentColor` — macOS's system blue: the prominent buttons, the
/// selected picker segment, focus rings. The app's agg-gui `Visuals` take
/// this accent (`ui/app.rs`) so only one blue exists on screen.
pub const ACCENT: Color = BLUE;
/// `Color.green` — mastered items, clean runs, mic level.
pub const GREEN: Color = Color::from_rgb8(0x28, 0xCD, 0x41);
/// `Color.gray.opacity(0.5)` — locked items in the heat map and legend.
pub const GRAY_LOCKED: Color = Color::rgba(0.5, 0.5, 0.5, 0.5);
/// The next-key highlight — the Swift literal
/// `Color(red: 0.11, green: 0.44, blue: 0.84)` in `PianoKeyboardView`.
pub const KEY_BLUE: Color = Color::rgb(0.11, 0.44, 0.84);
