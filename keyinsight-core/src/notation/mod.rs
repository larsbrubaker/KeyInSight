//! The notation layer: engraving via verovio-rust, the feedback-state
//! controller, and plain-language vocabulary.
//!
//! Ports `Sources/KeyInSight/Notation/`. The Swift WKWebView/SVG pipeline
//! maps to verovio-rust + an agg-gui widget (`docs/platform-substitutions.md`):
//! CSS class flips become per-id color overrides, the HTML ghost overlay
//! becomes widget painting, and the rAF follow loop becomes a schedule the
//! widget advances each painted frame.

mod controller;
mod renderer;
mod slide;
mod vocabulary;
mod widget;

#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod tests;

pub use controller::{NotationController, NoteState};
pub use renderer::{NotationRenderer, Rendered, READING_STAFF_PX};
pub use vocabulary::NotationVocabulary;
pub use widget::{NotationFit, NotationWidget};

#[cfg(test)]
mod wrap_tests {
    use super::*;

    /// Long repertoire flows onto multiple systems at the widget width
    /// (dump: `cargo test dump_wrapped_gymnopedie -- --ignored --nocapture`).
    #[test]
    fn long_pieces_wrap_at_the_system_width() {
        let xml = String::from_utf8_lossy(include_bytes!(
            "../../assets/pieces/gymnopedie-1.musicxml"
        ))
        .into_owned();
        let mut renderer = NotationRenderer::new();
        renderer.set_system_width(860.0);
        renderer.render(&xml).expect("gymnopedie engraves");
        let layout = renderer.toolkit().current_layout().expect("layout");
        assert!(
            layout.width <= 860.0,
            "wrapped inside the widget width, got {}",
            layout.width
        );
        assert!(
            layout.height > layout.width * 0.5,
            "94 notes stack into multiple rows (height {})",
            layout.height
        );
    }

    #[test]
    #[ignore]
    fn dump_wrapped_gymnopedie() {
        let xml = String::from_utf8_lossy(include_bytes!(
            "../../assets/pieces/gymnopedie-1.musicxml"
        ))
        .into_owned();
        let mut renderer = NotationRenderer::new();
        renderer.fit_view(871.0, 567.0);
        renderer.render(&xml).expect("gymnopedie engraves");
        let (width, height) = {
            let layout = renderer.toolkit().current_layout().unwrap();
            (layout.width.ceil() as u32, layout.height.ceil() as u32)
        };
        let mut framebuffer = agg_gui::framebuffer::Framebuffer::new(width, height);
        let mut ctx = agg_gui::gfx_ctx::GfxCtx::new(&mut framebuffer);
        agg_gui::draw_ctx::DrawCtx::clear(&mut ctx, agg_gui::color::Color::white());
        let font = verovio_rust::leipzig_font();
        renderer.toolkit().render(
            &mut ctx,
            &font,
            0.0,
            height as f64,
            &verovio_rust::RenderOptions::default(),
        );
        drop(ctx);
        let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
        let px = framebuffer.pixels();
        for y in (0..height as usize).rev() {
            for x in 0..width as usize {
                let i = (y * width as usize + x) * 4;
                out.extend_from_slice(&px[i..i + 3]);
            }
        }
        let path = std::env::temp_dir().join("gymnopedie_wrapped.ppm");
        std::fs::write(&path, out).unwrap();
        println!("wrote {}", path.display());
    }
}
