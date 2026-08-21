//! Pure text/color helpers for the Progress sheet rows and footer — the
//! parts of `ProgressPanel.swift` with no view in them.

use agg_gui::color::Color;

use crate::core::PitchSpelling;
use crate::engine::ProgressEntry;
use crate::notation::NoteState;
use crate::ui::palette;

/// Trouble-transition error tint: `entry.errorPercent >= 35 ? .red : .orange`.
pub(super) const TROUBLE_RED_THRESHOLD: i64 = 35;

pub(super) fn transition_color(error_percent: i64) -> Color {
    if error_percent >= TROUBLE_RED_THRESHOLD {
        palette::RED
    } else {
        palette::ORANGE
    }
}

/// Chord status tint: green when `"unlocked"`, else theme secondary
/// (`None` = dim).
pub(super) fn chord_status_color(status: &str) -> Option<Color> {
    (status == "unlocked").then_some(palette::GREEN)
}

/// `errorPercent.map { "\($0)% err" } ?? "—"`.
pub(super) fn error_text(error_percent: Option<i64>) -> String {
    error_percent
        .map(|e| format!("{e}% err"))
        .unwrap_or_else(|| "—".to_string())
}

/// `latencyMs.map { String(format: "%.1f s", $0 / 1000) } ?? "—"`.
pub(super) fn latency_text(latency_ms: Option<f64>) -> String {
    latency_ms
        .map(|l| format!("{:.1} s", l / 1000.0))
        .unwrap_or_else(|| "—".to_string())
}

/// The footer's mastery tally over both staves' unlocked items.
pub(super) fn mastery_tally(entries: &[ProgressEntry], bass_entries: &[ProgressEntry]) -> String {
    let unlocked: Vec<&ProgressEntry> = entries
        .iter()
        .chain(bass_entries.iter())
        .filter(|e| e.unlocked)
        .collect();
    let mastered = unlocked.iter().filter(|e| e.mastered).count();
    format!("{mastered} of {} active items mastered", unlocked.len())
}

/// The footer's next-unlock line from the treble and bass ladders'
/// `nextLockedMidi` (`"\(name) (left)"` for the bass), joined " · ".
pub(super) fn next_unlock_text(treble_next: Option<u8>, bass_next: Option<u8>) -> String {
    let names: Vec<String> = [
        treble_next.map(PitchSpelling::name),
        bass_next.map(|midi| format!("{} (left)", PitchSpelling::name(midi))),
    ]
    .into_iter()
    .flatten()
    .collect();
    if names.is_empty() {
        "All items unlocked".to_string()
    } else {
        format!(
            "Next unlock: {} — master all active items",
            names.join(" · ")
        )
    }
}

pub(super) fn heat_color(heat: NoteState) -> Color {
    match heat {
        NoteState::Mastered => palette::GREEN,
        NoteState::Weak => palette::RED,
        NoteState::Locked => palette::GRAY_LOCKED,
        _ => palette::ORANGE,
    }
}

/// `record.startedAt.formatted(date: .abbreviated, time: .shortened)` —
/// "Jul 7, 16:32" from epoch milliseconds (UTC; the engine clock has no
/// timezone database).
pub(super) fn format_timestamp(epoch_ms: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = epoch_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    // civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{} {}, {}, {:02}:{:02}",
        MONTHS[(month - 1) as usize],
        day,
        year,
        tod / 3600,
        (tod % 3600) / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(unlocked: bool, mastered: bool) -> ProgressEntry {
        ProgressEntry {
            midi: 60,
            name: "C4".to_string(),
            unlocked,
            mastered,
            attempts: 0,
            error_percent: None,
            latency_ms: None,
            heat: NoteState::Locked,
        }
    }

    #[test]
    fn trouble_transition_turns_red_at_35_percent() {
        assert_eq!(transition_color(34), palette::ORANGE);
        assert_eq!(transition_color(35), palette::RED);
        assert_eq!(transition_color(80), palette::RED);
    }

    #[test]
    fn chord_status_is_green_only_when_unlocked() {
        assert_eq!(chord_status_color("unlocked"), Some(palette::GREEN));
        assert_eq!(chord_status_color("probing"), None);
        assert_eq!(chord_status_color("locked"), None);
    }

    #[test]
    fn stat_texts_fall_back_to_a_dash() {
        assert_eq!(error_text(Some(12)), "12% err");
        assert_eq!(error_text(None), "—");
        assert_eq!(latency_text(Some(1234.0)), "1.2 s");
        assert_eq!(latency_text(None), "—");
    }

    #[test]
    fn footer_tally_spans_both_staves() {
        let treble = vec![entry(true, true), entry(true, false), entry(false, false)];
        let bass = vec![entry(true, true)];
        assert_eq!(mastery_tally(&treble, &bass), "2 of 3 active items mastered");
    }

    #[test]
    fn next_unlock_joins_treble_and_bass() {
        assert_eq!(next_unlock_text(None, None), "All items unlocked");
        assert_eq!(
            next_unlock_text(Some(69), None),
            "Next unlock: A4 — master all active items"
        );
        assert_eq!(
            next_unlock_text(None, Some(47)),
            "Next unlock: B2 (left) — master all active items"
        );
        assert_eq!(
            next_unlock_text(Some(69), Some(47)),
            "Next unlock: A4 · B2 (left) — master all active items"
        );
    }

    #[test]
    fn timestamp_formats_as_abbreviated_date_and_short_time() {
        // 2023-11-14 22:13:20 UTC
        assert_eq!(format_timestamp(1_700_000_000_000), "Nov 14, 2023, 22:13");
    }
}
