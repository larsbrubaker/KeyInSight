//! The side panel's status block — `statusSection` and `tempoStatus`
//! from `UI/SidePanel.swift` as [`InfoRow`] builders, preserving every
//! color, icon, and font treatment. The summary branch lives in
//! `summary.rs`.

use std::rc::Rc;

use crate::engine::{InputSource, PacingMode, Phase, SessionEngine, SurvivalPolicy};
use crate::ui::fonts::{icon, size, UiFonts};
use crate::ui::palette;
use crate::ui::{InfoRow, InfoRows, RowStyle};

use super::summary::summary_rows;
use super::Engine;

pub(super) fn status_section(engine: &Engine, fonts: &UiFonts) -> InfoRows {
    let engine = Rc::clone(engine);
    InfoRows::new(fonts, move || status_rows(&engine.borrow()))
}

/// Build the rows for the current engine phase.
pub(super) fn status_rows(engine: &SessionEngine) -> Vec<InfoRow> {
    match engine.phase() {
        Phase::Loading => vec![InfoRow::text("Preparing…", size::BODY).with_dim()],
        Phase::Playing if engine.is_free_play() => {
            let mut rows = vec![InfoRow::text(
                format!("{} notes played", engine.free_play_count()),
                size::BODY,
            )
            .with_style(RowStyle::Mono)];
            if let Some(last) = engine.last_free_play_note() {
                rows.push(
                    InfoRow::text(format!("Last note: {last}"), size::BODY)
                        .with_style(RowStyle::Mono),
                );
            }
            rows
        }
        Phase::Playing if engine.is_survival() => survival_rows(engine),
        Phase::Playing
            if engine.drill_active() && engine.input_source() != InputSource::SelfVerify =>
        {
            drill_rows(engine)
        }
        Phase::Playing => playing_rows(engine),
        Phase::Summary(summary) => summary_rows(summary),
        Phase::Failed(message) => vec![InfoRow::text(message.clone(), size::BODY)
            .with_icon(icon::WARNING)
            .with_color(palette::RED)],
    }
}

/// `.playing where isSurvival`: the lives row, the note count, and the
/// streak once it reaches 5.
fn survival_rows(engine: &SessionEngine) -> Vec<InfoRow> {
    let hearts = (0..SurvivalPolicy::START_LIVES)
        .map(|i| {
            if i < engine.survival_lives() {
                (icon::HEART, Some(palette::RED))
            } else {
                (icon::HEART_OUTLINE, None)
            }
        })
        .collect();
    let mut rows = vec![
        InfoRow::glyph_run(hearts, size::BODY),
        InfoRow::text(format!("{} notes", engine.survival_notes()), size::BODY)
            .with_style(RowStyle::Mono),
    ];
    if engine.streak() >= 5 {
        rows.push(
            InfoRow::text(format!("{} streak", engine.streak()), size::BODY)
                .with_icon(icon::FLAME)
                .with_style(RowStyle::Mono)
                .with_color(palette::ORANGE),
        );
    }
    rows
}

/// `.playing where drillActive` (detected input): card number, the streak
/// at any length (orange from 5), and the lit-key hint after a miss.
fn drill_rows(engine: &SessionEngine) -> Vec<InfoRow> {
    let streak = InfoRow::text(format!("{} streak", engine.streak()), size::BODY)
        .with_icon(icon::FLAME)
        .with_style(RowStyle::Mono);
    let streak = if engine.streak() >= 5 {
        streak.with_color(palette::ORANGE)
    } else {
        streak.with_dim()
    };
    let mut rows = vec![
        InfoRow::text(format!("Card {}", engine.drill_cards_done() + 1), size::BODY)
            .with_style(RowStyle::Mono),
        streak,
    ];
    if engine.drill_hint_keys() {
        rows.push(
            InfoRow::text("Find the lit key below", size::CALLOUT)
                .with_icon(icon::KEYBOARD)
                .with_color(palette::BLUE),
        );
    }
    rows
}

fn playing_rows(engine: &SessionEngine) -> Vec<InfoRow> {
    let mut rows = Vec::new();
    if engine.input_source() == InputSource::SelfVerify {
        rows.push(
            InfoRow::text(
                format!("Pass {}", engine.self_verify_attempts() + 1),
                size::BODY,
            )
            .with_style(RowStyle::Mono),
        );
    } else {
        rows.push(
            InfoRow::text(
                format!(
                    "Note {} of {}",
                    engine.current_note_number(),
                    engine.note_count()
                ),
                size::BODY,
            )
            .with_style(RowStyle::Mono),
        );
    }
    if engine.errors_this_exercise() > 0 {
        rows.push(
            InfoRow::text(format!("{} wrong", engine.errors_this_exercise()), size::BODY)
                .with_style(RowStyle::Mono)
                .with_color(palette::RED),
        );
    }
    if engine.streak() >= 5 {
        rows.push(
            InfoRow::text(format!("{} first-try streak", engine.streak()), size::BODY)
                .with_icon(icon::FLAME)
                .with_style(RowStyle::Mono)
                .with_color(palette::ORANGE),
        );
    }
    if engine.anchored_octaves() != 0 {
        let sign = if engine.anchored_octaves() > 0 { "+" } else { "" };
        rows.push(
            InfoRow::text(
                format!("Following your octave ({sign}{})", engine.anchored_octaves()),
                size::CALLOUT,
            )
            .with_icon(icon::UP_DOWN)
            .with_color(palette::BLUE),
        );
    }
    if engine.heard_uncertain() {
        rows.push(
            InfoRow::text("Heard something — couldn't tell what", size::CALLOUT)
                .with_icon(icon::EAR)
                .with_color(palette::ORANGE),
        );
    }
    // help "A burst of wrong notes looks like mashing, not practice —
    // stats resume with the next clean note."
    if engine.stats_suppressed() {
        rows.push(
            InfoRow::text("Noisy input — progress tracking paused", size::CALLOUT)
                .with_icon(icon::PAUSE_CIRCLE)
                .with_dim(),
        );
    }
    if engine.active_pacing() == PacingMode::Tempo {
        let bpm = format!("{} BPM", engine.tempo_bpm() as i64);
        if let Some(count_in) = engine.count_in_remaining() {
            rows.push(
                InfoRow::text(format!("Ready… {count_in}"), size::BODY)
                    .with_style(RowStyle::Bold)
                    .with_color(palette::BLUE),
            );
            rows.push(InfoRow::text(bpm, size::CALLOUT).with_style(RowStyle::Mono).with_dim());
        } else {
            rows.push(InfoRow::beat_dots(
                engine.beat_in_measure() as usize,
                4,
                bpm,
                size::CALLOUT,
            ));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::side_panel::test_engine;

    #[test]
    fn survival_status_shows_lives_hearts_and_note_count() {
        let mut engine = test_engine();
        engine.enter_survival();
        let rows = status_rows(&engine);
        let hearts = rows[0].glyphs.as_ref().expect("hearts glyph run");
        assert_eq!(hearts.len(), SurvivalPolicy::START_LIVES as usize);
        assert!(hearts.iter().all(|(glyph, color)| {
            *glyph == icon::HEART && *color == Some(palette::RED)
        }));
        assert_eq!(rows[1].text, "0 notes");
        assert_eq!(rows.len(), 2, "no streak row below 5");
    }

    #[test]
    fn drill_status_shows_card_and_streak_at_any_length() {
        let mut engine = test_engine();
        engine.set_input_source(InputSource::Keyboard);
        engine.start_drill();
        let rows = status_rows(&engine);
        assert_eq!(rows[0].text, "Card 1");
        assert_eq!(rows[1].text, "0 streak");
        assert_eq!(rows[1].icon, Some(icon::FLAME));
        assert!(rows[1].dim && rows[1].color.is_none(), "secondary below 5");
        assert_eq!(rows.len(), 2, "no lit-key hint without a miss");
    }

    #[test]
    fn training_status_counts_from_the_current_note_number() {
        let mut engine = test_engine();
        engine.set_input_source(InputSource::Keyboard);
        let rows = status_rows(&engine);
        assert_eq!(
            rows[0].text,
            format!("Note 1 of {}", engine.note_count())
        );
    }
}
