//! The controls block — `// MARK: - Controls` in `UI/SidePanel.swift`:
//! Hear It / Stop, Restart, and the per-phase action buttons. A Swift
//! button whose label swaps (`Hear It`/`Stop`, `Play`/`Stop`) becomes
//! two conditional buttons.

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widgets::{Button, Conditional, FlexColumn, FlexRow, Label};

use crate::ui::fonts::{icon, size, UiFonts};

use super::cells::{
    drill_playing_cell, drill_self_verify_cell, free_play_cell, free_play_play_cell,
    free_play_stop_cell, hear_it_cell, repertoire_playing_cell, self_verify_cell, stop_cell,
    summary_midi_caption_cell, summary_repertoire_cell, summary_survival_cell,
    summary_training_cell, survival_playing_cell,
};
use super::Engine;

pub(super) fn controls_section(engine: &Engine, fonts: &UiFonts) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(8.0);

    // Hear It / Stop (visible while playback content exists; label and
    // icon swap while playing back — two conditional buttons).
    {
        let visible = hear_it_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(
                Button::new("Hear It", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::PLAY, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().toggle_playback()),
            ),
        )));
    }
    {
        let visible = stop_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(
                Button::new("Stop", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::STOP, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().toggle_playback()),
            ),
        )));
    }
    // Repertoire: start the song over from the top at any point.
    {
        let visible = repertoire_playing_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(
                Button::new("Restart", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::UNDO, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().next_exercise()),
            ),
        )));
    }
    // Free play: Play/Stop the recorded take, Clear, Exit.
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
            // help "Replay everything you've played, at your timing";
            // disabled until a take exists.
            .add(Box::new(Conditional::new(
                play_visible,
                Box::new(
                    Button::new("Play", Arc::clone(&fonts.regular))
                        .with_subtle().with_active_fn(|| false)
                        .with_icon(icon::PLAY, Arc::clone(&fonts.icons))
                        .with_enabled_fn(move || play_enabled.borrow().free_play_count() > 0)
                        .on_click(move || play.borrow_mut().toggle_free_play_playback()),
                ),
            )))
            .add(Box::new(Conditional::new(
                stop_visible,
                Box::new(
                    Button::new("Stop", Arc::clone(&fonts.regular))
                        .with_subtle().with_active_fn(|| false)
                        .with_icon(icon::STOP, Arc::clone(&fonts.icons))
                        .on_click(move || stop.borrow_mut().toggle_free_play_playback()),
                ),
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
    // Drill joins while a drill runs (help "Wrap up with your totals").
    {
        let visible = self_verify_cell(engine);
        let nailed = Rc::clone(engine);
        let again = Rc::clone(engine);
        let end_visible = drill_self_verify_cell(engine);
        let end = Rc::clone(engine);
        let grading = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Nailed It", Arc::clone(&fonts.regular))
                    .with_icon(icon::CHECK, Arc::clone(&fonts.icons))
                    .on_click(move || nailed.borrow_mut().self_verify_grade(true)),
            ))
            .add(Box::new(
                Button::new("Try Again", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::UNDO, Arc::clone(&fonts.icons))
                    .on_click(move || again.borrow_mut().self_verify_grade(false)),
            ))
            .add(Box::new(Conditional::new(
                end_visible,
                Box::new(
                    Button::new("End Drill", Arc::clone(&fonts.regular))
                        .with_subtle().with_active_fn(|| false)
                        .on_click(move || end.borrow_mut().end_drill()),
                ),
            )));
        column = column.add(Box::new(Conditional::new(visible, Box::new(grading))));
    }
    // Survival: End Run (help "Stop here and take the score").
    {
        let visible = survival_playing_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(
                Button::new("End Run", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || click.borrow_mut().end_survival_run()),
            ),
        )));
    }
    // Drill (detected input): End Drill (help "Wrap up with your totals").
    {
        let visible = drill_playing_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(
                Button::new("End Drill", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || click.borrow_mut().end_drill()),
            ),
        )));
    }
    // Summary, survival: Run It Back (prominent) + Back to Training.
    {
        let visible = summary_survival_cell(engine);
        let again = Rc::clone(engine);
        let back = Rc::clone(engine);
        let survival = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Run It Back", Arc::clone(&fonts.regular))
                    .on_click(move || again.borrow_mut().enter_survival()),
            ))
            .add(Box::new(
                Button::new("Back to Training", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || back.borrow_mut().resume_training()),
            ));
        column = column.add(Box::new(Conditional::new(visible, Box::new(survival))));
    }
    // Summary, repertoire: Replay + Back to Training.
    {
        let visible = summary_repertoire_cell(engine);
        let replay = Rc::clone(engine);
        let back = Rc::clone(engine);
        let repertoire = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Replay", Arc::clone(&fonts.regular))
                    .on_click(move || replay.borrow_mut().next_exercise()),
            ))
            .add(Box::new(
                Button::new("Back to Training", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || back.borrow_mut().exit_repertoire()),
            ));
        column = column.add(Box::new(Conditional::new(visible, Box::new(repertoire))));
    }
    // Summary, training: Next Exercise (+ auto-continue note on MIDI).
    {
        let visible = summary_training_cell(engine);
        let next = Rc::clone(engine);
        let caption_visible = summary_midi_caption_cell(engine);
        let training = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Next Exercise", Arc::clone(&fonts.regular))
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
