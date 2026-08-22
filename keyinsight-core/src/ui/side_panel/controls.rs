//! The controls block — `// MARK: - Controls` in `UI/SidePanel.swift`:
//! Hear It / Stop, Restart, and the per-phase action buttons. A Swift
//! button whose label swaps (`Hear It`/`Stop`, `Play`/`Stop`) becomes
//! two conditional buttons.
//!
//! Geometry follows the Swift source: the buttons whose label carries
//! `.frame(maxWidth: .infinity)` (Hear It / Stop, Restart, Nailed It, Try
//! Again) span the panel; the rest sit at their natural width, leading.
//! `.keyboardShortcut(.defaultAction)` (Nailed It, Run It Back, Replay,
//! Next Exercise) maps to `Button::with_default_action()` — Return fires
//! the one that is visible, unless a sheet is up.

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::layout_props::HAnchor;
use agg_gui::widgets::{Button, Conditional, FlexColumn, FlexRow, Label, Tooltip};

use crate::ui::fonts::{icon, size, UiFonts};
use crate::ui::help;

use super::cells::{
    drill_playing_cell, drill_self_verify_cell, free_play_cell, free_play_play_cell,
    free_play_stop_cell, hear_it_cell, repertoire_playing_cell, self_verify_cell, stop_cell,
    summary_midi_caption_cell, summary_repertoire_cell, summary_survival_cell,
    summary_training_cell, survival_playing_cell,
};
use super::Engine;

/// The Swift `Label(…).frame(maxWidth: .infinity)`: span the panel width
/// with the label centered (the Button default).
fn full_width(button: Button) -> Button {
    button.with_h_anchor(HAnchor::STRETCH)
}

pub(super) fn controls_section(engine: &Engine, fonts: &UiFonts) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(8.0);

    // Hear It / Stop (visible while playback content exists; label and
    // icon swap while playing back — two conditional buttons).
    {
        let visible = hear_it_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(full_width(
                Button::new("Hear It", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::PLAY, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().toggle_playback()),
            )),
        )));
    }
    {
        let visible = stop_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(full_width(
                Button::new("Stop", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::STOP, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().toggle_playback()),
            )),
        )));
    }
    // Repertoire: start the song over from the top at any point.
    {
        let visible = repertoire_playing_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(full_width(
                Button::new("Restart", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::UNDO, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().next_exercise()),
            )),
        )));
    }
    // Free play: Play/Stop the recorded take, Clear, Exit (an HStack).
    {
        let visible = free_play_cell(engine);
        let play_visible = free_play_play_cell(engine);
        let stop_visible = free_play_stop_cell(engine);
        let play_enabled = Rc::clone(engine);
        let play = Rc::clone(engine);
        let stop = Rc::clone(engine);
        let clear = Rc::clone(engine);
        let exit = Rc::clone(engine);
        let row = FlexRow::new()
            .with_gap(8.0)
            // Disabled until a take exists; one help string for both faces.
            .add(Box::new(Conditional::new(
                play_visible,
                Box::new(Tooltip::new(
                    Box::new(
                        Button::new("Play", Arc::clone(&fonts.regular))
                            .with_subtle().with_active_fn(|| false)
                            .with_icon(icon::PLAY, Arc::clone(&fonts.icons))
                            .with_enabled_fn(move || play_enabled.borrow().free_play_count() > 0)
                            .on_click(move || play.borrow_mut().toggle_free_play_playback()),
                    ),
                    help::FREE_PLAY_PLAYBACK,
                    Arc::clone(&fonts.regular),
                )),
            )))
            .add(Box::new(Conditional::new(
                stop_visible,
                Box::new(Tooltip::new(
                    Box::new(
                        Button::new("Stop", Arc::clone(&fonts.regular))
                            .with_subtle().with_active_fn(|| false)
                            .with_icon(icon::STOP, Arc::clone(&fonts.icons))
                            .on_click(move || stop.borrow_mut().toggle_free_play_playback()),
                    ),
                    help::FREE_PLAY_PLAYBACK,
                    Arc::clone(&fonts.regular),
                )),
            )))
            .add(Box::new(
                Button::new("Clear", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || clear.borrow_mut().clear_free_play()),
            ))
            .add(Box::new(
                Button::new("Exit Free Play", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || exit.borrow_mut().exit_free_play()),
            ));
        column = column.add(Box::new(Conditional::new(visible, Box::new(row))));
    }
    // Unplugged grading: Nailed It is the prominent default action; End
    // Drill joins while a drill runs.
    {
        let visible = self_verify_cell(engine);
        let nailed = Rc::clone(engine);
        let again = Rc::clone(engine);
        let end_visible = drill_self_verify_cell(engine);
        let end = Rc::clone(engine);
        let grading = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(full_width(
                Button::new("Nailed It", Arc::clone(&fonts.regular))
                    .with_icon(icon::CHECK, Arc::clone(&fonts.icons))
                    .with_default_action()
                    .on_click(move || nailed.borrow_mut().self_verify_grade(true)),
            )))
            .add(Box::new(full_width(
                Button::new("Try Again", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::UNDO, Arc::clone(&fonts.icons))
                    .on_click(move || again.borrow_mut().self_verify_grade(false)),
            )))
            .add(Box::new(Conditional::new(
                end_visible,
                Box::new(end_drill_button(fonts, move || end.borrow_mut().end_drill())),
            )));
        column = column.add(Box::new(Conditional::new(visible, Box::new(grading))));
    }
    // Survival: End Run.
    {
        let visible = survival_playing_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(Tooltip::new(
                Box::new(
                    Button::new("End Run", Arc::clone(&fonts.regular))
                        .with_subtle().with_active_fn(|| false)
                        .on_click(move || click.borrow_mut().end_survival_run()),
                ),
                help::END_RUN,
                Arc::clone(&fonts.regular),
            )),
        )));
    }
    // Drill (detected input): End Drill.
    {
        let visible = drill_playing_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(end_drill_button(fonts, move || click.borrow_mut().end_drill())),
        )));
    }
    // Summary, survival: Run It Back (prominent, default) + Back to Training.
    {
        let visible = summary_survival_cell(engine);
        let again = Rc::clone(engine);
        let back = Rc::clone(engine);
        let survival = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Run It Back", Arc::clone(&fonts.regular))
                    .with_default_action()
                    .on_click(move || again.borrow_mut().enter_survival()),
            ))
            .add(Box::new(
                Button::new("Back to Training", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || back.borrow_mut().resume_training()),
            ));
        column = column.add(Box::new(Conditional::new(visible, Box::new(survival))));
    }
    // Summary, repertoire: Replay (prominent, default) + Back to Training.
    {
        let visible = summary_repertoire_cell(engine);
        let replay = Rc::clone(engine);
        let back = Rc::clone(engine);
        let repertoire = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Replay", Arc::clone(&fonts.regular))
                    .with_default_action()
                    .on_click(move || replay.borrow_mut().next_exercise()),
            ))
            .add(Box::new(
                Button::new("Back to Training", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || back.borrow_mut().exit_repertoire()),
            ));
        column = column.add(Box::new(Conditional::new(visible, Box::new(repertoire))));
    }
    // Summary, training: Next Exercise (prominent, default; + auto-continue
    // note on MIDI).
    {
        let visible = summary_training_cell(engine);
        let next = Rc::clone(engine);
        let caption_visible = summary_midi_caption_cell(engine);
        let training = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Next Exercise", Arc::clone(&fonts.regular))
                    .with_default_action()
                    .on_click(move || next.borrow_mut().next_exercise()),
            ))
            .add(Box::new(Conditional::new(
                caption_visible,
                Box::new(
                    Label::new("Continuing automatically…", Arc::clone(&fonts.regular))
                        .with_font_size(size::CAPTION)
                        .with_dim(true),
                ),
            )));
        column = column.add(Box::new(Conditional::new(visible, Box::new(training))));
    }
    column
}

/// `Button("End Drill") { engine.endDrill() }.help("Wrap up with your
/// totals")` — the same button in the self-verify grading block and the
/// detected-input drill branch.
fn end_drill_button(fonts: &UiFonts, on_click: impl FnMut() + 'static) -> Tooltip {
    Tooltip::new(
        Box::new(
            Button::new("End Drill", Arc::clone(&fonts.regular))
                .with_subtle().with_active_fn(|| false)
                .on_click(on_click),
        ),
        help::END_DRILL,
        Arc::clone(&fonts.regular),
    )
}
