//! The instructions callout — `// MARK: - Instructions` in
//! `UI/SidePanel.swift`: one paragraph per activity / input source.

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::widgets::Container;

use crate::engine::{InputSource, PacingMode, SessionEngine, SurvivalPolicy};
use crate::ui::fonts::{size, UiFonts};
use crate::ui::DynamicLabel;

use super::Engine;

pub(super) fn instruction_text(engine: &SessionEngine) -> String {
    if engine.is_free_play() {
        return "Play anything, either hand or both — it appears as notation. Rhythm is simplified; the staff shows your most recent notes. Everything is recorded: Play replays it, Clear starts a fresh take.".to_string();
    }
    if engine.is_survival() {
        return format!(
            "Endless reading at your current level — the next line arrives as you finish this one. {} wrong notes ends the run. Score rewards how much you read and how fast: keep your eyes moving ahead of your hands.",
            SurvivalPolicy::START_LIVES
        );
    }
    if engine.drill_active() {
        return "One note at a time, biased toward your weak spots, for as long as you like. Each card plays its sound as it appears — name it on the keyboard and build the streak. Miss one and the keys light the answer; it'll come back a few cards later. End Drill wraps up with your totals.".to_string();
    }
    match engine.input_source() {
        InputSource::SelfVerify => "Play the phrase on your instrument. Use Hear It to compare, then grade yourself honestly — repeated passes still count as practice.".to_string(),
        InputSource::Microphone => "Play single notes on your instrument near the mic. The meter below shows what it hears; uncertain notes are never marked wrong.".to_string(),
        InputSource::Midi => {
            if engine.active_pacing() == PacingMode::Tempo {
                "Wait for the count-in, then play with the clicks. ◀ early · ▶ late · amber = missed.".to_string()
            } else {
                "Play the blue note on your keyboard; the cursor waits for you. Hover over any symbol to learn its name.".to_string()
            }
        }
        InputSource::Keyboard => {
            if engine.active_pacing() == PacingMode::Tempo {
                "Wait for the count-in, then play with the clicks. ◀ early · ▶ late · amber = missed. A S D F G H J K = C–C, W E T Y U = sharps.".to_string()
            } else {
                "Play the blue note; the cursor waits for you. A S D F G H J K = C–C, W E T Y U = sharps, Z/X shift octave. Hover over any symbol to learn its name.".to_string()
            }
        }
    }
}

/// The rounded gray callout box (`Color.gray.opacity(0.08)`, radius 8,
/// padding 10).
pub(super) fn instructions_box(engine: &Engine, fonts: &UiFonts) -> Container {
    let engine = Rc::clone(engine);
    let label = DynamicLabel::new(
        move || instruction_text(&engine.borrow()),
        Arc::clone(&fonts.regular),
    )
    .with_font_size(size::CALLOUT)
    .with_dim(true)
    .with_wrap(true);
    Container::new()
        .with_background(Color::rgba(0.5, 0.5, 0.5, 0.08))
        .with_corner_radius(8.0)
        .with_padding(10.0)
        .with_fit_height(true)
        .add(Box::new(label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::side_panel::test_engine;

    #[test]
    fn activity_branches_win_over_the_input_source() {
        let mut engine = test_engine();
        engine.set_input_source(InputSource::SelfVerify);
        assert!(instruction_text(&engine).starts_with("Play the phrase on your instrument."));

        engine.start_drill();
        assert!(instruction_text(&engine).starts_with("One note at a time, biased toward your weak spots, for as long as you like."));

        engine.enter_survival();
        let text = instruction_text(&engine);
        assert!(text.starts_with("Endless reading at your current level"));
        assert!(text.contains("3 wrong notes ends the run."));

        engine.enter_free_play();
        assert!(instruction_text(&engine).starts_with("Play anything, either hand or both"));
        assert!(instruction_text(&engine).ends_with("Clear starts a fresh take."));
    }

    #[test]
    fn keyboard_and_midi_branch_on_the_active_pacing() {
        let mut engine = test_engine();
        engine.set_input_source(InputSource::Keyboard);
        assert!(instruction_text(&engine).contains("Z/X shift octave"));
        engine.set_input_source(InputSource::Midi);
        assert!(instruction_text(&engine).starts_with("Play the blue note on your keyboard"));

        engine.set_mode(PacingMode::Tempo);
        if engine.active_pacing() == PacingMode::Tempo {
            assert!(instruction_text(&engine).starts_with("Wait for the count-in"));
        } else {
            // Polyphonic content keeps self-paced; the text follows.
            assert!(instruction_text(&engine).starts_with("Play the blue note"));
        }
        engine.set_input_source(InputSource::Microphone);
        assert!(instruction_text(&engine).starts_with("Play single notes on your instrument near the mic."));
    }
}
