//! The summary rows — `summarySection` in `UI/SidePanel.swift`: a
//! survival block first (which suppresses the timing / first-try block),
//! then errors, trouble spot, and unlocks.

use crate::engine::ExerciseSummary;
use crate::ui::fonts::{icon, size};
use crate::ui::palette;
use crate::ui::{InfoRow, RowStyle};

pub(super) fn summary_rows(summary: &ExerciseSummary) -> Vec<InfoRow> {
    let mut rows = Vec::new();
    if let Some(survival) = &summary.survival {
        rows.push(
            InfoRow::text(format!("Score: {}", survival.score), size::BODY)
                .with_icon(icon::FLAG_CHECKERED)
                .with_style(RowStyle::Bold),
        );
        rows.push(
            InfoRow::text(
                format!(
                    "{} notes · {} notes/min · difficulty {:.1}",
                    survival.notes,
                    survival.notes_per_minute.round() as i64,
                    survival.difficulty
                ),
                size::CALLOUT,
            )
            .with_dim(),
        );
        if survival.is_new_best {
            rows.push(
                InfoRow::text("New best!", size::BODY)
                    .with_icon(icon::TROPHY)
                    .with_style(RowStyle::Bold)
                    .with_color(palette::ORANGE),
            );
        } else if survival.best > 0 {
            rows.push(InfoRow::text(format!("Best: {}", survival.best), size::CALLOUT).with_dim());
        }
    }
    if summary.drill {
        rows.push(InfoRow::text("Micro-drill complete", size::BODY).with_style(RowStyle::Bold));
    } else if summary.self_verified {
        rows.push(
            InfoRow::text("Self-verified", size::BODY)
                .with_icon(icon::CHECK_CIRCLE)
                .with_style(RowStyle::Bold),
        );
    }
    if summary.survival.is_some() {
        // The survival block above replaces the timing / first-try stats.
    } else if let Some(timing) = &summary.timing {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::SurvivalReport;

    fn base_summary() -> ExerciseSummary {
        ExerciseSummary {
            exercise_number: 1,
            note_count: 20,
            first_try_correct: 18,
            error_count: 2,
            mean_latency_ms: Some(500.0),
            newly_unlocked: None,
            streak: 0,
            timing: None,
            bpm: None,
            rhythm_unlocked: None,
            piece_title: None,
            worst_measure: None,
            drill: false,
            self_verified: false,
            survival: None,
        }
    }

    fn survival_summary(best: i64, is_new_best: bool) -> ExerciseSummary {
        ExerciseSummary {
            survival: Some(SurvivalReport {
                score: 340,
                notes: 20,
                notes_per_minute: 47.6,
                difficulty: 2.4,
                best,
                is_new_best,
            }),
            ..base_summary()
        }
    }

    #[test]
    fn training_summary_leads_with_first_try_accuracy() {
        let texts: Vec<String> = summary_rows(&base_summary()).into_iter().map(|r| r.text).collect();
        assert_eq!(
            texts,
            ["90% first try", "18 of 20 notes", "0.5 s per note", "2 wrong notes"]
        );
    }

    #[test]
    fn survival_summary_replaces_the_accuracy_block() {
        let rows = summary_rows(&survival_summary(200, true));
        assert_eq!(rows[0].text, "Score: 340");
        assert_eq!(rows[0].icon, Some(icon::FLAG_CHECKERED));
        assert_eq!(rows[0].style, RowStyle::Bold);
        assert_eq!(rows[1].text, "20 notes · 48 notes/min · difficulty 2.4");
        assert_eq!(rows[2].text, "New best!");
        assert_eq!(rows[2].icon, Some(icon::TROPHY));
        assert_eq!(rows[2].color, Some(palette::ORANGE));
        // Timing / first-try block suppressed; trailing rows still run.
        assert!(rows.iter().all(|r| !r.text.contains("first try")));
        assert!(rows.iter().all(|r| !r.text.contains("in the window")));
        assert_eq!(rows[3].text, "2 wrong notes");
        assert_eq!(rows.len(), 4);
    }

    #[test]
    fn survival_summary_shows_the_standing_best_when_not_beaten() {
        let rows = summary_rows(&survival_summary(500, false));
        assert_eq!(rows[2].text, "Best: 500");
        assert!(rows[2].dim);
        // No best row at all on a first run.
        let rows = summary_rows(&survival_summary(0, false));
        assert_eq!(rows[2].text, "2 wrong notes");
    }
}
