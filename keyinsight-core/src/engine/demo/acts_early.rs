//! DemoDriver acts 1–3.5: self-paced to the first unlock, the scripted
//! tempo exercise, the bundled piece, and practice-from-here.

use std::collections::HashSet;

use super::{ms_or_na, DemoDriver, DemoResult};
use crate::core::PitchSpelling;
use crate::engine::session::{ExerciseSummary, PacingMode, Phase};
use crate::score::RepertoireLibrary;

/// Act 1 gives up after this many self-paced exercises without an unlock.
pub const MAX_SELF_PACED_EXERCISES: i64 = 14;
/// Act 3.5 restarts the piece from this match event.
pub const PARTIAL_START_EVENT: usize = 4;

/// Act 2 timing profile by note index: 1 early, 2 late, 3 missed, 4
/// preceded by a wrong-pitch strike. Everything else on time.
#[derive(Default)]
struct TempoPlan {
    injected: HashSet<usize>,
    wrong_injected: bool,
}

impl TempoPlan {
    fn offset_ms(index: usize) -> Option<f64> {
        match index {
            1 => Some(-80.0), // early tick
            2 => Some(80.0),  // late tick
            3 => None,        // never played → missed
            _ => Some(0.0),
        }
    }
}

impl DemoDriver<'_> {
    // --- Act 1: self-paced until first unlock ---

    /// One wrong note in each of the first two exercises, then clean play
    /// until the first skill unlock (`selfPacedStep`).
    pub fn act1_unlock(&mut self) -> DemoResult<()> {
        let mut played_wrong_this_exercise = false;
        loop {
            match self.engine.phase().clone() {
                Phase::Playing => {
                    let Some(expected) = self.engine.current_expected_midi() else {
                        self.after(0.3)?;
                        continue;
                    };
                    let wants_wrong_note = self.engine.exercises_completed() < 2;
                    if wants_wrong_note
                        && !played_wrong_this_exercise
                        && self.engine.current_note_index() == 1
                    {
                        self.inject(expected + 4);
                        self.say(format!(
                            "demo: injected wrong note (expected {})",
                            PitchSpelling::name(expected)
                        ));
                        // (snapshot "exercise{n}-wrong-ghost" — not ported)
                        self.after(0.2)?;
                        played_wrong_this_exercise = true;
                        continue;
                    }
                    self.inject(expected);
                    self.after(0.1)?;
                }
                Phase::Summary(summary) => {
                    let mut line = format!(
                        "demo: exercise {} complete — {} notes, {} errors, streak {}",
                        summary.exercise_number,
                        summary.note_count,
                        summary.error_count,
                        summary.streak
                    );
                    if let Some(unlocked) = &summary.newly_unlocked {
                        line += &format!(", UNLOCKED {unlocked}");
                    }
                    self.say(line);

                    if summary.newly_unlocked.is_some() {
                        self.say(
                            "demo: act 1 passed (unlock earned) — switching to tempo mode"
                                .to_string(),
                        );
                        return Ok(());
                    } else if self.engine.exercises_completed() >= MAX_SELF_PACED_EXERCISES {
                        return Err(self.fail(&format!(
                            "no unlock within {MAX_SELF_PACED_EXERCISES} exercises"
                        )));
                    } else {
                        self.engine.next_exercise();
                        self.after(0.3)?;
                        played_wrong_this_exercise = false;
                    }
                }
                Phase::Loading => self.after(0.3)?,
                Phase::Failed(message) => return Err(self.fail(&message)),
            }
        }
    }

    // --- Act 2: one scripted tempo exercise ---

    /// Switch to tempo mode and play one ≥5-note exercise against the
    /// metronome clock with the scripted timing profile, then check the
    /// timing report classified every strike (`beginTempoAct` /
    /// `tempoStep` / `finishTempoAct`).
    pub fn act2_tempo(&mut self) -> DemoResult<()> {
        let mut attempt = 0;
        loop {
            if attempt >= 10 {
                return Err(self.fail("couldn't get a ≥5-note tempo exercise"));
            }
            if self.engine.mode() != PacingMode::Tempo {
                self.engine.set_mode(PacingMode::Tempo);
            } else {
                self.engine.next_exercise();
            }
            // The scripted profile needs at least 5 sounded notes.
            if self.engine.note_count() < 5 {
                self.after(0.2)?;
                attempt += 1;
                continue;
            }
            break;
        }
        self.say(format!(
            "demo: tempo exercise started — {} notes at {} BPM",
            self.engine.note_count(),
            self.engine.tempo_bpm() as i32
        ));
        let mut plan = TempoPlan::default();
        self.after(0.1)?;
        loop {
            if let Phase::Summary(summary) = self.engine.phase().clone() {
                return self.finish_tempo_act(&summary);
            }
            let debug = if *self.engine.phase() == Phase::Playing {
                self.engine.tempo_debug()
            } else {
                None
            };
            let Some(debug) = debug else {
                self.after(0.1)?;
                continue;
            };

            for (index, expected) in debug.targets.iter().enumerate() {
                if debug.resolutions[index].is_some() || plan.injected.contains(&index) {
                    continue;
                }
                let Some(offset) = TempoPlan::offset_ms(index) else {
                    continue;
                };
                // Wrong-pitch strike shortly before note 4's target.
                if index == 4 && !plan.wrong_injected {
                    if debug.now_ms >= expected.target_ms - 40.0 {
                        plan.wrong_injected = true;
                        self.inject(expected.midi + 1);
                        self.say("demo: injected wrong pitch before note 5".to_string());
                    }
                    continue;
                }
                if debug.now_ms >= expected.target_ms + offset {
                    plan.injected.insert(index);
                    self.inject(expected.midi);
                }
            }
            self.after(0.02)?;
        }
    }

    fn finish_tempo_act(&mut self, summary: &ExerciseSummary) -> DemoResult<()> {
        let Some(timing) = &summary.timing else {
            return Err(self.fail("tempo summary missing timing report"));
        };
        self.say(format!(
            "demo: tempo exercise complete — {} on time, {} early, {} late, {} missed, {} errors, mean offset {}",
            timing.on_time,
            timing.early,
            timing.late,
            timing.missed,
            summary.error_count,
            ms_or_na(timing.mean_abs_offset_ms)
        ));
        // (snapshot "tempo-complete" — not ported)
        let ok = timing.missed == 1
            && timing.early >= 1
            && timing.late >= 1
            && summary.error_count >= 1
            && timing.hit_count() == timing.expected_count - 1;
        if !ok {
            return Err(self.fail("tempo classifications don't match the script"));
        }
        self.say("demo: act 2 passed (tempo classification) — starting repertoire act".to_string());
        Ok(())
    }

    // --- Act 3: play a bundled piece ---

    /// Play Minuet in G (the key-signature path: F# from the signature)
    /// with one wrong note mid-piece so the measure heatmap has signal.
    /// Returns the piece's note count and its play count afterwards (Act
    /// 3.5's baseline).
    pub fn act3_repertoire(&mut self) -> DemoResult<(usize, i64)> {
        let pieces = RepertoireLibrary::bundled();
        // Minuet in G exercises the key-signature path (F# from the signature).
        let Some(piece) = pieces
            .iter()
            .find(|p| p.slug == "minuet-in-g")
            .or_else(|| pieces.first())
            .cloned()
        else {
            return Err(self.fail("no bundled pieces found"));
        };
        self.say(format!(
            "demo: repertoire — {}: {} notes, {} measures, fifths {}",
            piece.title,
            piece.exercise.sounded_notes().len(),
            piece.exercise.measures().len(),
            piece.exercise.fifths
        ));
        self.engine.set_mode(PacingMode::SelfPaced);
        self.engine.start_piece(piece);
        self.after(0.3)?;
        let mut injected_wrong = false;
        loop {
            match self.engine.phase().clone() {
                Phase::Playing => {
                    let Some(expected) = self.engine.current_expected_midi() else {
                        self.after(0.2)?;
                        continue;
                    };
                    // One wrong note mid-piece so the measure heatmap has signal.
                    if !injected_wrong && self.engine.current_note_index() == 6 {
                        self.inject(expected + 2);
                        self.after(0.05)?;
                        injected_wrong = true;
                        continue;
                    }
                    self.inject(expected);
                    self.after(0.05)?;
                }
                Phase::Summary(summary) => {
                    self.say(format!(
                        "demo: piece complete — {}, {}/{} first try, worst measure {}",
                        summary.piece_title.as_deref().unwrap_or("?"),
                        summary.first_try_correct,
                        summary.note_count,
                        summary
                            .worst_measure
                            .map(|(number, errors)| format!("{number} ({errors} errors)"))
                            .unwrap_or_else(|| "none".to_string())
                    ));
                    // (snapshot "repertoire-complete" — not ported)
                    self.report();
                    let stats = self.engine.piece_stats("minuet-in-g");
                    let plays = stats.map(|s| s.0).unwrap_or(0);
                    self.say(format!(
                        "demo: piece stats — plays {}, best {}",
                        plays,
                        stats
                            .map(|s| format!("{:.0}%", s.1 * 100.0))
                            .unwrap_or_else(|| "n/a".to_string())
                    ));
                    let ok = summary.piece_title.is_some()
                        && summary.worst_measure.is_some()
                        && summary.note_count == summary.first_try_correct + 1
                        && plays >= 1;
                    if !ok {
                        return Err(self.fail("repertoire results don't match the script"));
                    }
                    self.say(
                        "demo: act 3 passed (repertoire) — starting practice-from-here act"
                            .to_string(),
                    );
                    return Ok((summary.note_count, plays));
                }
                Phase::Loading => self.after(0.2)?,
                Phase::Failed(message) => return Err(self.fail(&message)),
            }
        }
    }

    // --- Act 3.5: practice-from-here (partial replay) ---

    /// Restart the piece from event 4 and play the tail cleanly; section
    /// practice must not count as a play of the piece.
    pub fn act3_5_practice_from_here(
        &mut self,
        full_note_count: usize,
        plays_before: i64,
    ) -> DemoResult<()> {
        self.engine.practice_from(PARTIAL_START_EVENT);
        self.say(format!(
            "demo: practice-from-here — event {}, measure {}, {} notes remain",
            PARTIAL_START_EVENT,
            self.engine.replay_start_measure(),
            self.engine.note_count()
        ));
        if self.engine.note_count() != full_note_count - PARTIAL_START_EVENT
            || self.engine.current_note_number() != 1
        {
            return Err(self.fail(&format!(
                "partial replay counts wrong ({} vs {} - {})",
                self.engine.note_count(),
                full_note_count,
                PARTIAL_START_EVENT
            )));
        }
        self.after(0.2)?;
        loop {
            match self.engine.phase().clone() {
                Phase::Playing => {
                    let Some(expected) = self.engine.current_expected_midi() else {
                        self.after(0.2)?;
                        continue;
                    };
                    self.inject(expected);
                    self.after(0.05)?;
                }
                Phase::Summary(summary) => {
                    let plays = self
                        .engine
                        .piece_stats("minuet-in-g")
                        .map(|s| s.0)
                        .unwrap_or(0);
                    self.say(format!(
                        "demo: partial replay complete — {}/{} first try, plays {} (was {})",
                        summary.first_try_correct, summary.note_count, plays, plays_before
                    ));
                    // Clean pass over the tail only; section practice must not
                    // count as a play of the piece.
                    let ok = summary.note_count == full_note_count - PARTIAL_START_EVENT
                        && summary.first_try_correct == summary.note_count
                        && plays == plays_before;
                    if !ok {
                        return Err(
                            self.fail("practice-from-here results don't match the script")
                        );
                    }
                    self.engine.clear_replay_start();
                    self.say(
                        "demo: act 3.5 passed (practice-from-here) — starting free-play act"
                            .to_string(),
                    );
                    return Ok(());
                }
                Phase::Loading => self.after(0.2)?,
                Phase::Failed(message) => return Err(self.fail(&message)),
            }
        }
    }
}
