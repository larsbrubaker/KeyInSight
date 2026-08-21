//! The app's bundled typefaces — the port of the SwiftUI font
//! environment. macOS renders the Swift app in the system sans (SF Pro)
//! with bold titles, monospaced digits, and SF Symbols; here that maps
//! to Inter (regular + bold), Cascadia Code for the monospaced runs, and
//! Font Awesome glyphs through agg-gui's icon-font path
//! (`docs/architecture.md`).

use std::sync::Arc;

use agg_gui::text::Font;

/// Inter (OFL) — see `assets/Inter-LICENSE.txt`.
pub const UI_FONT_BYTES: &[u8] = include_bytes!("../../assets/Inter-Regular.ttf");
pub const UI_BOLD_FONT_BYTES: &[u8] = include_bytes!("../../assets/Inter-Bold.ttf");
/// Cascadia Code (OFL) — monospaced digits and note names.
pub const MONO_FONT_BYTES: &[u8] = include_bytes!("../../assets/CascadiaCode.ttf");
/// Font Awesome Free solid (OFL font license).
pub const ICON_FONT_BYTES: &[u8] = include_bytes!("../../assets/fa.ttf");

/// The four faces every widget builder receives.
#[derive(Clone)]
pub struct UiFonts {
    /// Body text (SwiftUI `.body`/`.callout`/`.caption`).
    pub regular: Arc<Font>,
    /// Titles and headlines (SwiftUI `.bold()`/`.headline`).
    pub bold: Arc<Font>,
    /// Monospaced digits and note names (SwiftUI `.monospaced()`).
    pub mono: Arc<Font>,
    /// Icon glyphs (SF Symbols → Font Awesome).
    pub icons: Arc<Font>,
}

impl UiFonts {
    /// Parse the bundled faces. Both shells call this once at startup.
    pub fn bundled() -> Self {
        Self {
            regular: Arc::new(Font::from_slice(UI_FONT_BYTES).expect("Inter Regular parses")),
            bold: Arc::new(Font::from_slice(UI_BOLD_FONT_BYTES).expect("Inter Bold parses")),
            mono: Arc::new(Font::from_slice(MONO_FONT_BYTES).expect("Cascadia Code parses")),
            icons: Arc::new(Font::from_slice(ICON_FONT_BYTES).expect("Font Awesome parses")),
        }
    }
}

/// SwiftUI text styles at their macOS point sizes.
pub mod size {
    /// `.title2` — sheet titles.
    pub const TITLE2: f64 = 17.0;
    /// `.title3` — the side panel header.
    pub const TITLE3: f64 = 15.0;
    /// `.headline` / `.body`.
    pub const BODY: f64 = 13.0;
    /// `.callout` — secondary rows, instructions.
    pub const CALLOUT: f64 = 12.0;
    /// `.caption` — legend, fine print.
    pub const CAPTION: f64 = 10.0;
}

/// SF Symbol → Font Awesome Free (solid) glyph mapping, one constant per
/// symbol the Swift views name.
pub mod icon {
    /// `person.crop.circle`
    pub const USER_CIRCLE: char = '\u{f2bd}';
    /// `pencil`
    pub const PENCIL: char = '\u{f040}';
    /// `person.badge.plus`
    pub const USER_PLUS: char = '\u{f234}';
    /// `pianokeys` (Font Awesome Free has no piano — computer keys read
    /// closest for the beginner key strip toggle).
    pub const KEYBOARD: char = '\u{f11c}';
    /// `arrow.uturn.backward` / `arrow.counterclockwise`
    pub const UNDO: char = '\u{f0e2}';
    /// `chart.bar`
    pub const CHART_BAR: char = '\u{f080}';
    /// `play.fill`
    pub const PLAY: char = '\u{f04b}';
    /// `stop.fill`
    pub const STOP: char = '\u{f04d}';
    /// `checkmark`
    pub const CHECK: char = '\u{f00c}';
    /// `checkmark.circle`
    pub const CHECK_CIRCLE: char = '\u{f058}';
    /// `checkmark.seal.fill`
    pub const CHECK_SEAL: char = '\u{f058}';
    /// `flame.fill`
    pub const FLAME: char = '\u{f06d}';
    /// `arrow.up.arrow.down.circle`
    pub const UP_DOWN: char = '\u{f07d}';
    /// `ear.trianglebadge.exclamationmark`
    pub const EAR: char = '\u{f2a2}';
    /// `exclamationmark.triangle`
    pub const WARNING: char = '\u{f071}';
    /// `metronome` (no FA metronome; the clock reads as timing)
    pub const METRONOME: char = '\u{f017}';
    /// `target`
    pub const TARGET: char = '\u{f140}';
    /// `lock.open.fill`
    pub const LOCK_OPEN: char = '\u{f09c}';
    /// `music.note`
    pub const MUSIC: char = '\u{f001}';
    /// `books.vertical`
    pub const BOOKS: char = '\u{f02d}';
    /// `bolt`
    pub const BOLT: char = '\u{f0e7}';
    /// `arrow.right.to.line` (FA sign-in: an arrow running into a bar)
    pub const ARROW_RIGHT_TO_LINE: char = '\u{f090}';
    /// `xmark.circle.fill`
    pub const XMARK_CIRCLE: char = '\u{f057}';
    /// `heart.fill` — a remaining survival life
    pub const HEART: char = '\u{f004}';
    /// `heart` (outline) — a spent survival life
    pub const HEART_OUTLINE: char = '\u{f08a}';
    /// `pause.circle`
    pub const PAUSE_CIRCLE: char = '\u{f28b}';
    /// `flag.checkered`
    pub const FLAG_CHECKERED: char = '\u{f11e}';
    /// `trophy.fill`
    pub const TROPHY: char = '\u{f091}';
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every mapped icon must resolve to a real outline in the bundled
    /// Font Awesome face — a missing glyph renders as blank space.
    #[test]
    fn all_icons_present_in_font() {
        let fonts = UiFonts::bundled();
        for (name, glyph) in [
            ("USER_CIRCLE", icon::USER_CIRCLE),
            ("PENCIL", icon::PENCIL),
            ("USER_PLUS", icon::USER_PLUS),
            ("KEYBOARD", icon::KEYBOARD),
            ("UNDO", icon::UNDO),
            ("CHART_BAR", icon::CHART_BAR),
            ("PLAY", icon::PLAY),
            ("STOP", icon::STOP),
            ("CHECK", icon::CHECK),
            ("CHECK_CIRCLE", icon::CHECK_CIRCLE),
            ("FLAME", icon::FLAME),
            ("UP_DOWN", icon::UP_DOWN),
            ("EAR", icon::EAR),
            ("WARNING", icon::WARNING),
            ("METRONOME", icon::METRONOME),
            ("TARGET", icon::TARGET),
            ("LOCK_OPEN", icon::LOCK_OPEN),
            ("MUSIC", icon::MUSIC),
            ("BOOKS", icon::BOOKS),
            ("BOLT", icon::BOLT),
            ("ARROW_RIGHT_TO_LINE", icon::ARROW_RIGHT_TO_LINE),
            ("XMARK_CIRCLE", icon::XMARK_CIRCLE),
            ("HEART", icon::HEART),
            ("HEART_OUTLINE", icon::HEART_OUTLINE),
            ("PAUSE_CIRCLE", icon::PAUSE_CIRCLE),
            ("FLAG_CHECKERED", icon::FLAG_CHECKERED),
            ("TROPHY", icon::TROPHY),
        ] {
            assert!(
                fonts.icons.glyph_visual_bounds(glyph, 16.0).is_some(),
                "icon {name} (U+{:04X}) missing from bundled Font Awesome",
                glyph as u32
            );
        }
    }

    /// The UI faces must carry the typographic specials the labels use.
    #[test]
    fn ui_font_covers_special_characters() {
        let fonts = UiFonts::bundled();
        for ch in ['…', '·', '—', '±', '◀', '▶', '×'] {
            assert!(
                fonts.regular.glyph_visual_bounds(ch, 16.0).is_some(),
                "'{ch}' missing from Inter Regular"
            );
        }
    }
}
