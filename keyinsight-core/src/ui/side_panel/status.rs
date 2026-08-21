//! The side panel's status block — `statusSection`, `tempoStatus`, and
//! `summarySection` from `UI/SidePanel.swift` as [`InfoRow`] builders,
//! preserving every color, icon, and font treatment.

use crate::engine::{ExerciseSummary, InputSource, PacingMode, Phase, SessionEngine};
use crate::ui::fonts::{icon, size};
use crate::ui::palette;
use crate::ui::{InfoRow, RowStyle};

/// Build the rows for the current engine phase.
pub fn status_rows(engine: &SessionEngine) -> Vec<InfoRow> {
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
        Phase::Playing => playing_rows(engine),
        Phase::Summary(summary) => summary_rows(summary),
        Phase::Failed(message) => vec![InfoRow::text(message.clone(), size::BODY)
            .with_icon(icon::WARNING)
            .with_color(palette::RED)],
    }
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
                    engine.current_note_index() + 1,
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

fn summary_rows(summary: &ExerciseSummary) -> Vec<InfoRow> {
    let mut rows = Vec::new();
    if summary.drill {
        rows.push(InfoRow::text("Micro-drill complete", size::BODY).with_style(RowStyle::Bold));
    } else if summary.self_verified {
        rows.push(
            InfoRow::text("Self-verified", size::BODY)
                .with_icon(icon::CHECK_CIRCLE)
                .with_style(RowStyle::Bold),
        );
    }
    if let Some(timing) = &summary.timing {
        rows.push(
            InfoRow::text(
                format!("{}% in the window", (timing.hit_rate() * 100.0).round() as i64),
                size::BODY,
            )
            .with_icon(icon::METRONOME)
            .with_style(RowStyle::Bold),
        );
        rows.push(
            InfoRow::text(
                format!(
                    "{} on time · {} early · {} late · {} missed",
                    timing.on_time, timing.early, timing.late, timing.missed
                ),
                size::CALLOUT,
            )
            .with_dim(),
        );
        if let Some(offset) = timing.mean_abs_offset_ms {
            rows.push(
                InfoRow::text(format!("±{offset:.0} ms mean offset"), size::CALLOUT).with_dim(),
            );
        }
    } else {
        rows.push(
            InfoRow::text(
                format!("{}% first try", summary.accuracy_percent()),
                size::BODY,
            )
            .with_icon(icon::TARGET)
            .with_style(RowStyle::Bold),
        );
        rows.push(
            InfoRow::text(
                format!("{} of {} notes", summary.first_try_correct, summary.note_count),
                size::CALLOUT,
            )
            .with_dim(),
        );
        if let Some(latency) = summary.mean_latency_ms {
            rows.push(
                InfoRow::text(format!("{:.1} s per note", latency / 1000.0), size::CALLOUT)
                    .with_dim(),
            );
        }
    }
    if summary.error_count > 0 {
        let text = if summary.self_verified {
            format!(
                "{} repeated {}",
                summary.error_count,
                if summary.error_count == 1 { "pass" } else { "passes" }
            )
        } else {
            format!("{} wrong notes", summary.error_count)
        };
        rows.push(InfoRow::text(text, size::CALLOUT).with_color(palette::RED));
    }
    if let Some((number, errors)) = summary.worst_measure {
        rows.push(
            InfoRow::text(
                format!("Measure {number} is your trouble spot ({errors})"),
                size::CALLOUT,
            )
            .with_color(palette::ORANGE),
        );
    }
    if let Some(unlocked) = &summary.newly_unlocked {
        rows.push(
            InfoRow::text(format!("{unlocked} unlocked!"), size::BODY)
                .with_icon(icon::LOCK_OPEN)
                .with_style(RowStyle::Bold)
                .with_color(palette::BLUE),
        );
    }
    if let Some(rhythm) = &summary.rhythm_unlocked {
        rows.push(
            InfoRow::text(format!("New rhythm: {rhythm}!"), size::BODY)
                .with_icon(icon::MUSIC)
                .with_style(RowStyle::Bold)
                .with_color(palette::BLUE),
        );
    }
    rows
}
