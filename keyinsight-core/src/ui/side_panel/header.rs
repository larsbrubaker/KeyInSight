//! The panel header — `// MARK: - Header` in `UI/SidePanel.swift`:
//! title + activity subtitle, the practice-from-here chip, and the
//! optional exercise info line (spacing 3).

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widgets::{Button, Conditional, FlexColumn, FlexRow, Tooltip};

use crate::engine::SessionEngine;
use crate::ui::fonts::{icon, size, UiFonts};
use crate::ui::help;
use crate::ui::{palette, DynamicLabel, InfoRow, InfoRows};

use super::cells::replay_start_cell;
use super::Engine;

/// The `.title3.bold()` line, by activity (branch order is the Swift's).
pub(super) fn header_title(engine: &SessionEngine) -> String {
    if engine.is_free_play() {
        "Free Play".to_string()
    } else if engine.is_survival() {
        "Survival".to_string()
    } else if let Some(piece) = engine.active_piece() {
        piece.title.clone()
    } else if engine.drill_active() {
        "Micro-drill".to_string()
    } else {
        format!("Exercise {}", engine.exercises_completed() + 1)
    }
}

/// The secondary `.callout` line under the title.
pub(super) fn header_subtitle(engine: &SessionEngine) -> String {
    if engine.is_free_play() {
        "Live notation mirror".to_string()
    } else if engine.is_survival() {
        if engine.survival_best() > 0 {
            format!("Endless reading · best score {}", engine.survival_best())
        } else {
            "Endless reading at your level".to_string()
        }
    } else if engine.active_piece().is_some() {
        "Repertoire · click any note to practice from there".to_string()
    } else if engine.drill_active() {
        "Note naming · no timer, build the streak".to_string()
    } else {
        "Adaptive training".to_string()
    }
}

/// The repertoire practice-from-here chip text: one blue callout row
/// while a replay start spot is set, nothing otherwise.
pub(super) fn replay_chip_rows(engine: &SessionEngine) -> Vec<InfoRow> {
    if engine.active_piece().is_some()
        && !engine.is_free_play()
        && !engine.is_survival()
        && engine.replay_start_event() > 0
    {
        vec![InfoRow::text(
            format!("Starting at measure {}", engine.replay_start_measure()),
            size::CALLOUT,
        )
        .with_icon(icon::ARROW_RIGHT_TO_LINE)
        .with_color(palette::BLUE)]
    } else {
        Vec::new()
    }
}

/// Title + activity subtitle + optional exercise info (spacing 3).
pub(super) fn header(engine: &Engine, fonts: &UiFonts) -> FlexColumn {
    let title = {
        let engine = Rc::clone(engine);
        DynamicLabel::new(
            move || header_title(&engine.borrow()),
            Arc::clone(&fonts.bold),
        )
        .with_font_size(size::TITLE3)
    };
    let subtitle = {
        let engine = Rc::clone(engine);
        DynamicLabel::new(
            move || header_subtitle(&engine.borrow()),
            Arc::clone(&fonts.regular),
        )
        .with_font_size(size::CALLOUT)
        .with_dim(true)
    };
    // "Starting at measure N" + xmark.circle.fill → clearReplayStart()
    // (the Swift `HStack(spacing: 6)`).
    let replay_chip = {
        let visible = replay_start_cell(engine);
        let rows_engine = Rc::clone(engine);
        let clear = Rc::clone(engine);
        let row = FlexRow::new()
            .with_gap(6.0)
            .add_flex(
                Box::new(InfoRows::new(fonts, move || {
                    replay_chip_rows(&rows_engine.borrow())
                })),
                1.0,
            )
            .add(Box::new(Tooltip::new(
                // `.buttonStyle(.plain)` icon button, secondary ink.
                Box::new(
                    Button::new("", Arc::clone(&fonts.regular))
                        .with_ghost()
                        .with_active_fn(|| false)
                        .with_compact()
                        .with_icon(icon::XMARK_CIRCLE, Arc::clone(&fonts.icons))
                        .on_click(move || clear.borrow_mut().clear_replay_start()),
                ),
                help::CLEAR_REPLAY_START,
                Arc::clone(&fonts.regular),
            )));
        Conditional::new(visible, Box::new(row))
    };
    let info = {
        let engine = Rc::clone(engine);
        DynamicLabel::new(
            move || {
                let engine = engine.borrow();
                if engine.is_free_play() {
                    String::new()
                } else {
                    engine.exercise_info().unwrap_or("").to_string()
                }
            },
            Arc::clone(&fonts.regular),
        )
        .with_font_size(size::CALLOUT)
        .with_dim(true)
    };
    FlexColumn::new()
        .with_gap(3.0)
        .add(Box::new(title))
        .add(Box::new(subtitle))
        .add(Box::new(replay_chip))
        .add(Box::new(info))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::RepertoireLibrary;
    use crate::ui::side_panel::test_engine;

    #[test]
    fn header_follows_the_activity_branch_order() {
        let mut engine = test_engine();
        assert_eq!(header_title(&engine), "Exercise 1");
        assert_eq!(header_subtitle(&engine), "Adaptive training");
        assert!(replay_chip_rows(&engine).is_empty());

        engine.start_drill();
        assert_eq!(header_title(&engine), "Micro-drill");
        assert_eq!(header_subtitle(&engine), "Note naming · no timer, build the streak");

        engine.enter_survival();
        assert_eq!(header_title(&engine), "Survival");
        assert_eq!(header_subtitle(&engine), "Endless reading at your level");

        engine.enter_free_play();
        assert_eq!(header_title(&engine), "Free Play");
        assert_eq!(header_subtitle(&engine), "Live notation mirror");
    }

    #[test]
    fn repertoire_header_shows_the_practice_from_chip() {
        let mut engine = test_engine();
        let piece = RepertoireLibrary::bundled()
            .into_iter()
            .find(|p| p.slug == "twinkle-twinkle")
            .expect("bundled piece");
        let title = piece.title.clone();
        engine.start_piece(piece);
        assert_eq!(header_title(&engine), title);
        assert_eq!(
            header_subtitle(&engine),
            "Repertoire · click any note to practice from there"
        );
        assert!(replay_chip_rows(&engine).is_empty());

        engine.practice_from(4);
        let rows = replay_chip_rows(&engine);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].text,
            format!("Starting at measure {}", engine.replay_start_measure())
        );
        assert_eq!(rows[0].icon, Some(icon::ARROW_RIGHT_TO_LINE));
        assert_eq!(rows[0].color, Some(palette::BLUE));

        engine.clear_replay_start();
        assert!(replay_chip_rows(&engine).is_empty());
    }
}
