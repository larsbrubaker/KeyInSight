//! The hover-help strings — every SwiftUI `.help("…")` in
//! `Sources/KeyInSight/UI/*.swift`, verbatim, in one greppable table. The
//! widgets wrap themselves in [`agg_gui::widgets::Tooltip`] with these;
//! the completeness test below reads the Swift sources so a new `.help`
//! upstream can't be missed here.

use crate::engine::SurvivalPolicy;

// ── BottomBar.swift ─────────────────────────────────────────────────────

pub const RENAME_PLAYER: &str = "Rename this player";
pub const ADD_PLAYER: &str = "Add a player";
pub const PLAYER_SETTINGS: &str = "Player settings — octave following, keys strip";
/// The Keys button while no piece is active (`engine.activePiece == nil`).
pub const KEYS_TRAINING: &str = "Show the next key to play (remembered for training exercises)";
/// The Keys button while a repertoire piece is active.
pub const KEYS_PIECE: &str = "Show the next key to play (remembered for this piece)";
pub const RESUME_TRAINING: &str = "Back to adaptive exercises";
pub const ABOUT: &str = "How this trainer works — the skills, the vocabulary, the modes";

// ── SidePanel.swift ─────────────────────────────────────────────────────

/// The practice-from-here chip's clear button.
pub const CLEAR_REPLAY_START: &str = "Back to the top of the piece";
/// Status row "Following your octave (±n)". The status rows are painted
/// by [`crate::ui::InfoRows`], one widget for the whole block, so a
/// per-row hover target needs either per-row child widgets or an agg-gui
/// hook to submit a tip from a custom widget — not wired yet.
#[allow(dead_code)]
pub const FOLLOWING_OCTAVE: &str =
    "You started an octave off the written notes — that's fine, the exercise follows you.";
/// Status row "Noisy input — progress tracking paused" (an InfoRows row,
/// same note as [`FOLLOWING_OCTAVE`]).
#[allow(dead_code)]
pub const STATS_SUPPRESSED: &str =
    "A burst of wrong notes looks like mashing, not practice — stats resume with the next clean note.";
/// Free play's Play / Stop button.
pub const FREE_PLAY_PLAYBACK: &str = "Replay everything you've played, at your timing";
/// End Drill (both the self-verify copy and the detected-input one).
pub const END_DRILL: &str = "Wrap up with your totals";
pub const END_RUN: &str = "Stop here and take the score";
/// The Hands picker.
pub const HANDS: &str = "Right = treble clef, Left = bass clef, Both = hands together. Auto rotates toward your weaker hand and mixes in two-hand exercises once the bass range is learned.";
pub const CALIBRATE: &str = "Measure this device's input latency";
pub const MIC_LEVEL: &str = "Microphone level";

/// The Survival footer button: `"Endless reading, \(SurvivalPolicy.startLives)
/// lives — beat your best score"`.
pub fn survival() -> String {
    format!(
        "Endless reading, {} lives — beat your best score",
        SurvivalPolicy::START_LIVES
    )
}

/// Every help string, for the completeness check.
#[cfg(test)]
pub fn all() -> Vec<String> {
    let mut all: Vec<String> = [
        RENAME_PLAYER,
        ADD_PLAYER,
        PLAYER_SETTINGS,
        KEYS_TRAINING,
        KEYS_PIECE,
        RESUME_TRAINING,
        ABOUT,
        CLEAR_REPLAY_START,
        FOLLOWING_OCTAVE,
        STATS_SUPPRESSED,
        FREE_PLAY_PLAYBACK,
        END_DRILL,
        END_RUN,
        HANDS,
        CALIBRATE,
        MIC_LEVEL,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    all.push(survival());
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Every string literal inside `.help( … )` in a Swift source — the
    /// argument may be a plain literal or a ternary of two, possibly over
    /// several lines; interpolations are substituted like the app does.
    fn swift_help_strings(source: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = source;
        while let Some(at) = rest.find(".help(") {
            let after = &rest[at + ".help(".len()..];
            // Balanced parentheses (string contents hold none here).
            let mut depth = 1usize;
            let mut end = after.len();
            for (i, ch) in after.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let arg = &after[..end];
            let mut chunks = arg.split('"');
            chunks.next(); // before the first quote
            while let (Some(literal), Some(_)) = (chunks.next(), chunks.next()) {
                out.push(literal.replace(
                    "\\(SurvivalPolicy.startLives)",
                    &SurvivalPolicy::START_LIVES.to_string(),
                ));
            }
            rest = &after[end..];
        }
        out
    }

    #[test]
    fn every_swift_help_string_is_in_the_table_verbatim() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../keyinsight-swift-reference/Sources/KeyInSight/UI");
        let mut found = Vec::new();
        let table = all();
        for entry in std::fs::read_dir(&dir).expect("Swift UI sources (submodule)") {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("swift") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for help in swift_help_strings(&source) {
                assert!(
                    table.contains(&help),
                    "{}: .help({help:?}) has no entry in ui/help.rs",
                    path.display()
                );
                found.push(help);
            }
        }
        // The count guards the extractor itself: 17 `.help(` sites (6 in
        // BottomBar.swift, 11 in SidePanel.swift), one a two-string
        // ternary, and "Wrap up with your totals" twice.
        assert_eq!(found.len(), 18, "help strings found in the Swift UI sources: {found:#?}");
    }

    #[test]
    fn table_entries_are_unique_and_non_empty() {
        let table = all();
        for (i, entry) in table.iter().enumerate() {
            assert!(!entry.is_empty());
            assert!(!table[..i].contains(entry), "duplicate help string {entry:?}");
        }
        assert_eq!(survival(), "Endless reading, 3 lives — beat your best score");
    }

    #[test]
    fn extractor_handles_ternaries_and_interpolation() {
        let source = r#"
            .help("One")
            .help(flag
                  ? "Two"
                  : "Three")
            .help("Endless reading, \(SurvivalPolicy.startLives) lives")
        "#;
        assert_eq!(
            swift_help_strings(source),
            ["One", "Two", "Three", "Endless reading, 3 lives"]
        );
    }
}
