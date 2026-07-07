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
//!
//! Port status: scaffold. The Swift modules land in the dependency order
//! documented in `docs/porting.md`; until the training loop is up, the app
//! is a status screen that proves the native + WASM pipelines end to end.

pub mod core;
pub mod engine;
pub mod persistence;
pub mod score;
pub mod skill;

use std::sync::Arc;

use agg_gui::text::Font;
use agg_gui::widgets::{FlexColumn, Label};
use agg_gui::App;

/// Version stamp shown on the status screen and reported by the demo site.
pub const PORT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// CascadiaCode bundled into the binary so both targets render identical
/// glyphs without filesystem access (agg-gui's text stack needs a parsed
/// `Font` before the first paint).
pub const DEFAULT_FONT_BYTES: &[u8] = include_bytes!("../assets/CascadiaCode.ttf");

/// Load the default UI font as an `Arc<Font>`.
pub fn load_default_font() -> Arc<Font> {
    Arc::new(Font::from_slice(DEFAULT_FONT_BYTES).expect("keyinsight default font"))
}

/// Platform capability surface. The native and WASM shells implement this so
/// the core can request services without `cfg`-gating. Grows as modules are
/// ported (MIDI port enumeration, audio output, mic capture, storage — see
/// `docs/platform-substitutions.md`).
pub trait KeyInSightPlatform: 'static {}

/// Build the shared KeyInSight widget tree. Both shells call this and
/// forward platform input into the returned [`App`].
///
/// Currently a status screen; it becomes the TrainingView-rooted tree as the
/// UI modules are ported.
pub fn build_keyinsight_app<P: KeyInSightPlatform>(font: Arc<Font>, _platform: P) -> App {
    // KeyInSight is a light-themed app on every platform — sheet music is
    // black ink on a light page, and the chrome follows (see CLAUDE.md).
    agg_gui::set_visuals(agg_gui::Visuals::light());

    let root = FlexColumn::new()
        .with_gap(12.0)
        .with_padding(24.0)
        .add(Box::new(
            Label::new("KeyInSight", Arc::clone(&font)).with_font_size(32.0),
        ))
        .add(Box::new(
            Label::new(
                "Rust + agg-gui port — scaffold",
                Arc::clone(&font),
            )
            .with_font_size(16.0),
        ))
        .add(Box::new(
            Label::new(
                format!("port version {PORT_VERSION}"),
                Arc::clone(&font),
            )
            .with_font_size(13.0),
        ));

    App::new(Box::new(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopPlatform;
    impl KeyInSightPlatform for NoopPlatform {}

    /// The status app must build and survive a layout pass on both targets —
    /// this is the end-to-end smoke test CI runs until real modules land.
    #[test]
    fn status_app_builds_and_lays_out() {
        let mut app = build_keyinsight_app(load_default_font(), NoopPlatform);
        app.layout(agg_gui::geometry::Size::new(1024.0, 768.0));
    }
}
