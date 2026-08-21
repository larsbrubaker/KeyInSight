//! DemoDriver acts 4–8: the Free Play mirror, the micro-drill, the
//! reference-playback smoke test + follow-cursor audit, Unplugged
//! self-verification, and the survival run.

use super::{ms_or_na, DemoAudio, DemoDriver, DemoResult};
use crate::core::PitchSpelling;
use crate::engine::session::{HandMode, InputSource, Phase, DRILL_LENGTH, PLAYBACK_PREVIEW_BPM};
use crate::engine::SurvivalPolicy;

/// Act 8: play cleanly past the window seam (and through the deferred
/// window swap) for this many notes, then burn the lives. Injection is
/// paced so the swap timer actually fires mid-run rather than being
/// outrun.
pub const SURVIVAL_TARGET_NOTES: usize = 30;
pub const SURVIVAL_NOTE_GAP: f64 = 0.2;

impl DemoDriver<'_> {
    // --- Act 4: Free Play mirror ---

    /// Mirror a five-note riff and check the count and the last note.
    pub fn act4_free_play(&mut self) -> DemoResult<()> {
        self.engine.enter_free_play();
        let riff: [u8; 5] = [60, 64, 67, 72, 76];
        // `after(0.15 * (i + 1))` from one moment: one note every 0.15 s.
        for midi in riff {
            self.after(0.15)?;
            self.inject(midi);
        }
        // …and the check at 0.15 * (count + 2).
        self.after(0.15 * 2.0)?;
        self.say(format!(
            "demo: free play — {} notes mirrored, last {}",
            self.engine.free_play_count(),
            self.engine.last_free_play_note().unwrap_or("none")
        ));
        // (snapshot "freeplay" — not ported)
        if self.engine.free_play_count() != riff.len()
            || self.engine.last_free_play_note() != Some("E5")
        {
            return Err(self.fail("free-play mirror count/last note wrong"));
        }
        self.say("demo: act 4 passed (free play) — starting micro-drill act".to_string());
        self.engine.exit_free_play();
        Ok(())
    }

    // --- Act 5: micro-drill ---

    /// Endless drill: play `DRILL_LENGTH` cards, then end it ourselves and
    /// check the aggregated summary.
    pub fn act5_drill(&mut self) -> DemoResult<()> {
        self.engine.start_drill();
        self.after(0.2)?;
        let mut last_card: Option<u8> = None;
        loop {
            match self.engine.phase().clone() {
                Phase::Playing => {
                    // Endless drill: play drillLength cards, then end it ourselves.
                    if self.engine.drill_cards_done() >= DRILL_LENGTH {
                        self.engine.end_drill();
                        self.after(0.2)?;
                        continue;
                    }
                    let Some(expected) = self.engine.current_expected_midi() else {
                        self.after(0.05)?;
                        continue;
                    };
                    // Consecutive cards must differ — an identical card would
                    // be indistinguishable from no card at all.
                    if last_card == Some(expected) {
                        return Err(self.fail(&format!(
                            "drill repeated the same card ({})",
                            PitchSpelling::name(expected)
                        )));
                    }
                    self.inject(expected);
                    self.after(0.05)?;
                    last_card = Some(expected);
                }
                Phase::Summary(summary) => {
                    self.say(format!(
                        "demo: drill complete — {} cards, {} first try, mean {}",
                        summary.note_count,
                        summary.first_try_correct,
                        ms_or_na(summary.mean_latency_ms)
                    ));
                    self.report();
                    if !(summary.drill
                        && summary.note_count == DRILL_LENGTH as usize
                        && summary.first_try_correct == DRILL_LENGTH as usize)
                    {
                        return Err(self.fail("drill summary doesn't match the script"));
                    }
                    self.say("demo: act 5 passed (drill) — playback smoke test".to_string());
                    return Ok(());
                }
                Phase::Loading => self.after(0.2)?,
                Phase::Failed(message) => return Err(self.fail(&message)),
            }
        }
    }

    // --- Act 6: reference-playback smoke test ---

    /// Hear It on the drill's last card, then the follow-cursor audit on a
    /// fresh multi-note exercise (`playbackSmokeTest` / `followAudit`).
    pub fn act6_playback(&mut self) -> DemoResult<()> {
        self.engine.toggle_playback();
        if !self.engine.is_playing_back() {
            // Headless environments may have no audio output — report, don't fail.
            self.say("demo: playback unavailable in this environment — skipped".to_string());
            return Ok(());
        }
        self.say(format!(
            "demo: playback started ({})",
            DemoAudio::INSTRUMENT_DESCRIPTION
        ));
        // The drill's last card is a whole note: 4 beats at 90 BPM ≈ 2.7 s.
        self.after(4.0)?;
        if self.engine.is_playing_back() {
            return Err(self.fail("playback never completed"));
        }
        self.follow_audit()
    }

    /// Regression audit for the skipping-cursor bug: play back a
    /// multi-note exercise and require the follow cursor to have painted
    /// EVERY note, in order (the controller logs painted indices).
    ///
    /// The Swift audit skipped itself when every app window was occluded
    /// (rAF throttled to zero = nothing painting, which is correct cursor
    /// behavior, not the bug). Not ported: the Rust follow cursor is
    /// pumped by the scripted frame loop in [`DemoDriver::after`], never
    /// by a window's paint schedule, so there is no occlusion case.
    fn follow_audit(&mut self) -> DemoResult<()> {
        self.engine.next_exercise();
        self.after(0.6)?;
        let count = self.engine.note_count();
        self.engine.toggle_playback();
        if !self.engine.is_playing_back() {
            self.say("demo: follow audit skipped (no audio) — starting self-verify act".to_string());
            return Ok(());
        }
        self.say(format!(
            "demo: follow audit — {} notes at {} BPM",
            count, PLAYBACK_PREVIEW_BPM as i32
        ));
        // Up to 2 measures of 4/4 at 90 BPM ≈ 5.3 s + halt margin.
        self.after(6.5)?;
        let log: Vec<usize> = self.engine.notation.borrow().follow_log().to_vec();
        let expected: Vec<usize> = (0..count).collect();
        if log != expected {
            return Err(self.fail(&format!(
                "follow cursor skipped notes: painted {log:?}, expected 0..<{count}"
            )));
        }
        self.say(format!(
            "demo: act 6 passed (playback + follow cursor painted all {count} notes)"
        ));
        Ok(())
    }

    // --- Act 7: Unplugged self-verification ---

    /// First pass fails, second nails it; the summary records one
    /// repeated pass.
    pub fn act7_self_verify(&mut self) -> DemoResult<()> {
        self.engine.set_input_source(InputSource::SelfVerify);
        self.engine.next_exercise();
        self.after(0.3)?;
        // First pass fails, second nails it.
        self.engine.self_verify_grade(false);
        self.say("demo: self-verify — graded Try Again (pass 1)".to_string());
        self.after(0.3)?;
        self.engine.self_verify_grade(true);
        self.after(0.3)?;
        let summary = match self.engine.phase() {
            Phase::Summary(summary)
                if summary.self_verified
                    && summary.error_count == 1
                    && summary.first_try_correct == 0 =>
            {
                summary.clone()
            }
            _ => return Err(self.fail("self-verify summary doesn't match the script")),
        };
        self.say(format!(
            "demo: self-verify complete — {} notes, 1 repeated pass, recorded to item stats",
            summary.note_count
        ));
        Ok(())
    }

    // --- Act 8: survival run (OQ-25) ---

    /// Play cleanly past the window seam (and through the deferred window
    /// swap), then burn all three lives and check the scored summary +
    /// persisted best. Ends the session on success (the Swift `exit(0)`).
    pub fn act8_survival(&mut self) -> DemoResult<()> {
        self.engine.set_input_source(InputSource::Keyboard);
        // Auto + survival must be two hands EVERY chunk (no jarring
        // hand-mode switches mid-run).
        self.engine.set_hand_mode(HandMode::Auto);
        self.engine.enter_survival();
        if !self.engine.is_survival()
            || self.engine.survival_lives() != SurvivalPolicy::START_LIVES
        {
            return Err(self.fail("survival didn't start with a full error budget"));
        }
        if !self
            .engine
            .exercise_info()
            .is_some_and(|info| info.contains("two hands"))
        {
            return Err(self.fail(&format!(
                "survival under Auto isn't two-handed ({})",
                self.engine.exercise_info().unwrap_or("no info")
            )));
        }
        // Survival diverts the session (the side panel's training controls
        // step aside until the run ends).
        if !self.engine.is_diverted() {
            return Err(self.fail("survival didn't divert the session"));
        }
        self.say(format!(
            "demo: survival started — {} notes in chunk 1, {}",
            self.engine.note_count(),
            self.engine.exercise_info().unwrap_or("")
        ));
        self.after(0.2)?;
        let mut wrongs_injected = 0;
        loop {
            if let Phase::Summary(summary) = self.engine.phase().clone() {
                let Some(survival) = &summary.survival else {
                    return Err(self.fail("survival ended without a survival report"));
                };
                self.say(format!(
                    "demo: survival run over — score {}, {} notes at {} npm, difficulty {:.1}, {}",
                    survival.score,
                    survival.notes,
                    survival.notes_per_minute.round() as i64,
                    survival.difficulty,
                    if survival.is_new_best {
                        "NEW BEST".to_string()
                    } else {
                        format!("best {}", survival.best)
                    }
                ));
                if !(survival.notes >= SURVIVAL_TARGET_NOTES
                    && survival.score > 0
                    && survival.is_new_best
                    && self.engine.survival_best() == survival.score)
                {
                    return Err(self.fail("survival results don't match the script"));
                }
                // The sliding window must have advanced at least once —
                // seamlessly, mid-play — during a 30-note run.
                if self.engine.survival_window_gen() < 2 {
                    return Err(self.fail(&format!(
                        "survival window never advanced (gen {})",
                        self.engine.survival_window_gen()
                    )));
                }
                self.say(
                    "demo: OK — unlock, tempo, repertoire, free play, drill, playback, self-verify, and survival all verified"
                        .to_string(),
                );
                self.engine.end_session();
                return Ok(());
            }
            let mut expected_set: Vec<u8> =
                self.engine.current_expected_midis().iter().copied().collect();
            expected_set.sort_unstable();
            if *self.engine.phase() != Phase::Playing || expected_set.is_empty() {
                self.after(0.2)?;
                continue;
            }
            // Read cleanly past a chunk boundary, then die on purpose.
            // Two-hand onsets are sets — strike every member (within the
            // chord window).
            if self.engine.survival_notes() < SURVIVAL_TARGET_NOTES {
                for midi in expected_set {
                    self.inject(midi);
                }
                self.after(SURVIVAL_NOTE_GAP)?;
            } else {
                let highest = *expected_set.last().expect("checked non-empty");
                self.inject(highest + 1);
                wrongs_injected += 1;
                self.say(format!(
                    "demo: survival — injected wrong note {}, lives left {}",
                    wrongs_injected,
                    self.engine.survival_lives()
                ));
                self.after(0.1)?;
            }
        }
    }
}
