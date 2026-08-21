//! Exercise completion: the end-of-exercise summary, skill/attempt
//! recording, tempo-run teardown, and the auto-advance scheduling.
//! (Split from `lifecycle.rs` to keep both under the file-size limit.)

use crate::core::{NoteEvent, PitchSpelling};
use crate::engine::session::{
    Deferred, DrillTotals, ExerciseSummary, InputSource, PacingMode, Phase, SessionEngine,
    AUTO_ADVANCE_DELAY, AUTO_ADVANCE_UNLOCK_DELAY,
};
use crate::engine::{RhythmPolicy, TempoPolicy};
use crate::skill::SkillModel;

impl SessionEngine {
    pub(crate) fn finish_exercise(&mut self) {
        if self.phase != Phase::Playing {
            return;
        }
        self.current_expected_midis.clear();
        let timing_report = self.tempo_matcher.as_ref().map(|m| m.report());
        self.teardown_tempo_run();

        self.exercises_completed += 1;
        let now = self.now_ms();
        if let (Some(db), Some(exercise_id)) = (&mut self.db, self.exercise_id) {
            db.complete_exercise(
                exercise_id,
                now,
                self.note_count as i64,
                self.errors_this_exercise as i64,
            );
        }

        // Micro-drill: accumulate and chain straight to the next card;
        // one aggregated summary at the end.
        if let Some(remaining) = self.drill_remaining {
            self.drill_totals.notes += self.note_count;
            self.drill_totals.first_try += self.first_try_correct;
            self.drill_totals.errors += self.errors_this_exercise;
            self.drill_totals.latencies_ms.extend(&self.latencies_ms);
            if remaining > 1 {
                self.drill_remaining = Some(remaining - 1);
                self.next_exercise();
                return;
            }
            self.drill_remaining = None;
            self.refresh_skill();
            let mut drill_unlock: Option<String> = None;
            if let Some(new_midi) = self.skill.unlock_if_earned() {
                drill_unlock = Some(PitchSpelling::name(new_midi));
                let count = self.skill.unlocked_count() as i64;
                let now = self.now_ms();
                if let Some(db) = &mut self.db {
                    db.set_unlocked_item_count(count, now);
                }
                self.refresh_skill();
            }
            let totals = std::mem::replace(&mut self.drill_totals, DrillTotals::new());
            let mean_latency = if totals.latencies_ms.is_empty() {
                None
            } else {
                Some(totals.latencies_ms.iter().sum::<f64>() / totals.latencies_ms.len() as f64)
            };
            let unlocked = drill_unlock.is_some();
            self.phase = Phase::Summary(ExerciseSummary {
                exercise_number: self.exercise_number,
                note_count: totals.notes,
                first_try_correct: totals.first_try,
                error_count: totals.errors,
                mean_latency_ms: mean_latency,
                newly_unlocked: drill_unlock,
                streak: self.streak,
                timing: None,
                bpm: None,
                rhythm_unlocked: None,
                piece_title: None,
                worst_measure: None,
                drill: true,
                self_verified: self.input_source == InputSource::SelfVerify,
            });
            self.schedule_auto_advance(unlocked);
            return;
        }

        // Skill model catch-up: stats changed during play; maybe unlock.
        self.refresh_skill();
        let mut unlocked_name: Option<String> = None;
        if let Some(new_midi) = self.skill.unlock_if_earned() {
            unlocked_name = Some(PitchSpelling::name(new_midi));
            let count = self.skill.unlocked_count() as i64;
            let now = self.now_ms();
            if let Some(db) = &mut self.db {
                db.set_unlocked_item_count(count, now);
            }
            self.refresh_skill();
        }

        // Tempo + rhythm adaptive axes — training only; repertoire pieces
        // have fixed content and shouldn't move the training difficulty.
        let mut rhythm_unlocked_name: Option<String> = None;
        let exercise_bpm = if self.mode == PacingMode::Tempo {
            Some(self.tempo_bpm)
        } else {
            None
        };
        if let Some(timing_report) = &timing_report {
            if self.mode == PacingMode::Tempo && self.active_piece.is_none() {
                if RhythmPolicy::should_advance(self.rhythm_level, timing_report, self.tempo_bpm) {
                    self.rhythm_level += 1;
                    rhythm_unlocked_name =
                        RhythmPolicy::unlock_name(self.rhythm_level).map(str::to_string);
                    let level = self.rhythm_level.to_string();
                    let now = self.now_ms();
                    if let Some(db) = &mut self.db {
                        db.set_setting("rhythm_level", &level, now);
                    }
                }
                let new_bpm = TempoPolicy::next(self.tempo_bpm, timing_report);
                if new_bpm != self.tempo_bpm {
                    self.tempo_bpm = new_bpm;
                    let now = self.now_ms();
                    if let Some(db) = &mut self.db {
                        db.set_setting("tempo_bpm", &new_bpm.to_string(), now);
                    }
                }
            }
        }

        // Repertoire: persist the play with its per-measure heatmap data.
        let mut worst_measure: Option<(usize, i64)> = None;
        if let Some(piece) = self.active_piece.clone() {
            if let Some((index, &errors)) = self
                .errors_by_measure
                .iter()
                .enumerate()
                .max_by_key(|(_, &e)| e)
            {
                if errors > 0 {
                    worst_measure = Some((index + 1, errors));
                }
            }
            let accuracy = if self.note_count == 0 {
                0.0
            } else {
                self.first_try_correct as f64 / self.note_count as f64
            };
            let heat_json =
                serde_json::to_string(&self.errors_by_measure).unwrap_or_else(|_| "[]".into());
            let now = self.now_ms();
            let (note_count, error_count, mode_label) = (
                self.note_count as i64,
                self.errors_this_exercise as i64,
                self.mode.label(),
            );
            if let Some(db) = &mut self.db {
                db.record_piece_play(
                    &piece.slug,
                    &piece.title,
                    mode_label,
                    note_count,
                    error_count,
                    accuracy,
                    &heat_json,
                    now,
                );
            }
        }

        let unlocked = unlocked_name.is_some() || rhythm_unlocked_name.is_some();
        self.phase = Phase::Summary(ExerciseSummary {
            exercise_number: self.exercise_number,
            note_count: self.note_count,
            first_try_correct: self.first_try_correct,
            error_count: self.errors_this_exercise,
            mean_latency_ms: if self.latencies_ms.is_empty() {
                None
            } else {
                Some(self.latencies_ms.iter().sum::<f64>() / self.latencies_ms.len() as f64)
            },
            newly_unlocked: unlocked_name,
            streak: self.streak,
            timing: timing_report,
            bpm: exercise_bpm,
            rhythm_unlocked: rhythm_unlocked_name,
            piece_title: self.active_piece.as_ref().map(|p| p.title.clone()),
            worst_measure,
            drill: false,
            self_verified: self.input_source == InputSource::SelfVerify,
        });
        self.schedule_auto_advance(unlocked);
        agg_gui::animation::request_draw();
    }

    /// MIDI-mode training flows straight into the next exercise after a
    /// glanceable pause. Never in repertoire (it would replay the same
    /// piece forever); the Next Exercise button still skips the wait.
    pub(crate) fn schedule_auto_advance(&mut self, unlocked: bool) {
        if self.input_source != InputSource::Midi || self.active_piece.is_some() {
            return;
        }
        let generation = self.exercise_number;
        let delay = if unlocked {
            AUTO_ADVANCE_UNLOCK_DELAY
        } else {
            AUTO_ADVANCE_DELAY
        };
        self.defer_action(delay, Deferred::AutoAdvance { generation });
    }

    pub(crate) fn teardown_tempo_run(&mut self) {
        self.sweep_running = false;
        self.metronome.stop();
        self.count_in_remaining = None;
        self.tempo_finish_scheduled = false;
    }

    pub(crate) fn refresh_skill(&mut self) {
        let stats = self
            .db
            .as_ref()
            .map(|db| db.item_stats())
            .unwrap_or_default();
        self.skill.refresh(&stats);
    }

    pub(crate) fn record_attempt(
        &mut self,
        midi: u8,
        staff: crate::score::Staff,
        was_error: bool,
        latency_ms: Option<f64>,
    ) {
        let now = self.now_ms();
        if let Some(db) = &mut self.db {
            db.record_item_attempt(
                &SkillModel::item_name_on(midi, staff),
                was_error,
                latency_ms,
                now,
            );
        }
    }

    /// The interval *into* a note is part of what made it hard — track the
    /// shape ("down a 3rd") alongside the pitch item. Only meaningful along
    /// a monophonic line: chords/two-hand events don't record intervals.
    pub(crate) fn record_interval_attempt(
        &mut self,
        index: usize,
        was_error: bool,
        latency_ms: Option<f64>,
    ) {
        if index == 0
            || self.events[index].pitches.len() != 1
            || self.events[index - 1].pitches.len() != 1
        {
            return;
        }
        let delta = PitchSpelling::diatonic_index(self.events[index].pitches[0])
            - PitchSpelling::diatonic_index(self.events[index - 1].pitches[0]);
        // Repertoire can leap arbitrarily; only the tracked shapes count.
        let max_delta = *crate::skill::INTERVAL_DELTAS.iter().max().unwrap();
        if delta.abs() > max_delta {
            return;
        }
        let now = self.now_ms();
        if let Some(db) = &mut self.db {
            db.record_item_attempt(
                &SkillModel::interval_item_name(delta),
                was_error,
                latency_ms,
                now,
            );
        }
    }

    pub(crate) fn log(
        &mut self,
        event: &NoteEvent,
        classification: &str,
        expected_index: Option<usize>,
        offset_ms: Option<f64>,
    ) {
        let now = self.now_ms();
        if let (Some(db), Some(exercise_id)) = (&mut self.db, self.exercise_id) {
            db.log_event(
                exercise_id,
                now,
                match event.kind {
                    crate::core::NoteEventKind::On => "on",
                    crate::core::NoteEventKind::Off => "off",
                },
                event.midi as i64,
                event.velocity.map(|v| v as i64),
                classification,
                expected_index.map(|i| i as i64),
                offset_ms,
            );
        }
    }
}
