//! # KeyInSight Core
//!
//! Target-agnostic core for the KeyInSight port: the training engine, score
//! model, skill model, and every visible widget. Per `docs/architecture.md`,
//! all UI paints through agg-gui's `DrawCtx` — the native and WASM shells in
//! sibling crates own only the OS window/canvas, event loop, and platform
//! capability implementations.
//!
//! The crate is `wasm32`-clean: no `tokio`, no `winit`, no `wgpu`, no
//! `midir`, no `cpal`. Platform shells inject capabilities through the
//! [`KeyInSightPlatform`] trait.

pub mod audio;
pub mod core;
pub mod engine;
pub mod input;
pub mod notation;
pub mod persistence;
pub mod score;
pub mod skill;
pub mod ui;

use std::sync::Arc;

use agg_gui::text::Font;

pub use ui::{build_keyinsight_app, KeyInSightHandles, KeyInSightPlatform};

/// Version stamp reported by the demo site.
pub const PORT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// CascadiaCode bundled into the binary so both targets render identical
/// glyphs without filesystem access (agg-gui's text stack needs a parsed
/// `Font` before the first paint). Music glyphs come from verovio-rust's
/// bundled Leipzig.
pub const DEFAULT_FONT_BYTES: &[u8] = include_bytes!("../assets/CascadiaCode.ttf");

/// Load the default UI font as an `Arc<Font>`.
pub fn load_default_font() -> Arc<Font> {
    Arc::new(Font::from_slice(DEFAULT_FONT_BYTES).expect("keyinsight default font"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopPlatform;
    impl KeyInSightPlatform for NoopPlatform {}

    /// The full app must build and survive a layout pass on both targets —
    /// the end-to-end smoke test CI runs (an exercise is generated,
    /// engraved, and laid out into the widget tree).
    #[test]
    fn full_app_builds_and_lays_out() {
        let (mut app, handles) = build_keyinsight_app(load_default_font(), NoopPlatform);
        app.layout(agg_gui::geometry::Size::new(1180.0, 620.0));
        handles.tick();
        assert_eq!(
            *handles.engine.borrow().phase(),
            crate::engine::Phase::Playing
        );
    }
}
