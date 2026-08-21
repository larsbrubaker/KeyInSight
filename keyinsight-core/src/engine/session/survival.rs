//! Survival mode (OQ-25): endless neutral-bias chunks at the current
//! level with an error budget of lives. The score is volume × reading
//! rate × difficulty. The window on screen is three stitched one-line
//! chunks; crossing the seam between the first two slides the score up a
//! line and — once that slide has settled — the window advances
//! invisibly to [B, C, D] with the play position carried across.

use crate::core::Rng64;
use crate::engine::session::{
    ExerciseSummary, HandMode, Phase, SessionEngine, SurvivalReport, SURVIVAL_CHUNK_MEASURES,
};
use crate::engine::{SelfPacedMatcher, SurvivalPolicy};
use crate::notation::NoteState;
use crate::score::{
    ChordShape, DifficultyDescriptors, Exercise, Hands, MusicXmlEncoder, PitchOption,
};
use crate::ui::KeyboardLayout;

impl SessionEngine {
    /// Start a run: endless chunks at the current level, neutral bias
    /// (assessment, not drilling), an error budget of lives.
    pub fn enter_survival(&mut self) {
        self.stop_playback();
        self.teardown_tempo_run();
        self.active_piece = None;
        self.drill_active = false;
        self.is_free_play = false;
        self.pending_replay = None;
        self.replay_start_event = 0;
        self.is_survival = true;
        self.survival_lives = SurvivalPolicy::START_LIVES;
        self.survival_notes = 0;
        self.survival_difficulties.clear();
        self.survival_upcoming.clear();
        self.survival_seam_events = 0;
        self.survival_fifths = self.pick_survival_fifths();
        self.survival_start = (self.clock)();
        self.streak = 0;
        self.next_exercise();
    }

    /// One key for the whole run: stitched windows can't change signature,
    /// and key-hopping every few bars read as churn anyway.
    fn pick_survival_fifths(&mut self) -> i32 {
        let mut keys: Vec<i32> = self
            .skill
            .available_keys()
            .iter()
            .map(|k| k.fifths)
            .collect();
        if self.hand_mode != HandMode::Right {
            let bass: Vec<i32> = self
                .bass_skill
                .available_keys()
                .iter()
                .map(|k| k.fifths)
                .collect();
            keys.retain(|f| bass.contains(f));
        }
        // Swift draws `randomElement` from a Set; a sorted, deduplicated
        // list makes the draw deterministic per seed.
        keys.sort_unstable();
        keys.dedup();
        if keys.is_empty() {
            return 0;
        }
        keys[self.rng.next_below(keys.len())]
    }

    /// One two-measure survival chunk at the current level, neutral bias.
    /// Auto is always two hands (no jarring mode switches mid-run); the
    /// weaker hand carries the melodic walk more often.
    pub(crate) fn generate_survival_chunk(&mut self) -> Exercise {
        self.generator.config.measures = SURVIVAL_CHUNK_MEASURES;
        self.generator.config.rhythm_level = self.rhythm_level;
        let hands = if self.hand_mode == HandMode::Auto {
            let right = self.skill.mean_active_weight();
            let left = self.bass_skill.mean_active_weight();
            self.generator.config.melody_in_bass = self.rng.next_f64_below(right + left) < left;
            Hands::Both
        } else {
            let hands = self.resolve_hands();
            self.generator.config.melody_in_bass = false;
            hands
        };
        self.generator.config.hands = hands;
        let model = if hands == Hands::Left || self.generator.config.melody_in_bass {
            &self.bass_skill
        } else {
            &self.skill
        };
        self.generator.config.fifths = self.survival_fifths;
        self.generator.config.interval_weights = Default::default();
        self.generator.config.transition_weights = Default::default();
        self.generator.config.allowed_steps = self.skill.unlocked_interval_sizes();
        self.generator.config.probe_step = None;
        self.generator.config.chord_shapes = if hands == Hands::Right {
            self.skill
                .unlocked_chord_shapes()
                .iter()
                .filter_map(|name| ChordShape::by_skill_item(name).map(|shape| (shape, 1.0)))
                .collect()
        } else {
            Vec::new()
        };
        self.generator.config.probe_chord_shape = None;
        let neutral: Vec<PitchOption> = model
            .active_pitch_options_in_key(self.survival_fifths)
            .iter()
            .map(|option| PitchOption::new(option.midi))
            .collect();
        let chunk = self.generator.generate(&neutral, &mut self.rng);
        self.survival_difficulties
            .push(DifficultyDescriptors::compute(&chunk).index());
        chunk
    }

    /// Crossing the seam scheduled this: replace the window [A, B, C]
    /// with [B, C, D] once the seam line's slide animation has settled.
    /// The re-render puts the active line back at the top of the fresh
    /// page — visually where the slide just left it — so the swap is
    /// imperceptible.
    pub(crate) fn advance_survival_window(&mut self, generation: i64) {
        if !self.is_survival
            || generation != self.survival_window_gen
            || self.phase != Phase::Playing
            || self.survival_upcoming.is_empty()
        {
            return;
        }
        let Some(played) = self
            .matcher
            .as_ref()
            .map(|m| m.index().saturating_sub(self.survival_seam_events))
        else {
            return;
        };
        let mut parts = self.survival_upcoming.clone();
        parts.push(self.generate_survival_chunk());
        let window = Exercise::stitched(&parts);
        let xml = MusicXmlEncoder::encode_with_breaks(&window, Some(2));
        let rendered = self.renderer.borrow_mut().render_with(&xml, true);
        let Some(rendered) = rendered else {
            return; // keep playing the old window; recover at its end
        };
        if !self.bind_rendered(&window, &rendered) {
            return;
        }
        self.survival_window_gen += 1;
        self.survival_upcoming = parts[1..].to_vec();
        self.survival_seam_events = parts[0].match_events().len();
        self.matcher = Some(SelfPacedMatcher::with_start_index(
            window.expected_sets(),
            played,
        ));
        self.note_count = self.events.len();
        self.current_note_index = played;
        self.anchor_eligible = self.events.iter().all(|e| e.pitches.len() == 1);
        self.measure_by_event = window.event_measure_indices();
        self.errors_by_measure = vec![0; window.measure_count()];
        self.notation.borrow_mut().load_score();
        {
            let mut notation = self.notation.borrow_mut();
            for index in 0..played {
                for id in &self.event_ids[index] {
                    notation.set_state(id, Some(NoteState::Correct));
                }
                self.consumed_positions[index] = (0..self.events[index].pitches.len()).collect();
            }
        }
        self.set_current(played);
        self.current_note_start = (self.clock)();
        let all_pitches: Vec<u8> = self.events.iter().flat_map(|e| e.pitches.clone()).collect();
        self.keyboard_layout = KeyboardLayout::covering(
            all_pitches.iter().min().copied().unwrap_or(48),
            all_pitches.iter().max().copied().unwrap_or(84),
        );
        self.exercise_number += 1;
        let now = self.now_ms();
        if let (Some(db), Some(session_id)) = (&mut self.db, self.session_id) {
            if let Some(exercise_id) = self.exercise_id {
                db.complete_exercise(
                    exercise_id,
                    now,
                    self.survival_seam_events as i64,
                    self.errors_this_exercise as i64,
                );
            }
            let fresh = parts.last().expect("parts has the fresh chunk");
            let spec = serde_json::to_string(fresh).unwrap_or_else(|_| "{}".to_string());
            self.exercise_id = Some(db.create_exercise(
                session_id,
                self.exercise_number,
                &spec,
                now,
                Some(DifficultyDescriptors::compute(fresh).json()),
                None,
            ));
        }
        // The Swift engine leaves `exercise` at the first window; here the
        // accessor feeds the UI (inspect/key name), so it tracks the window.
        self.exercise = Some(window);
        if self.demo_trace {
            println!("engine: survival window advanced (played {played} into the seam)");
        }
        agg_gui::animation::request_draw();
    }

    /// Called from both `.wrong` and `.restarted` in the self-paced path —
    /// nowhere else.
    pub(crate) fn survival_life_lost(&mut self) {
        if !self.is_survival {
            return;
        }
        self.survival_lives -= 1;
        if self.survival_lives <= 0 {
            self.end_survival_run();
        }
    }

    /// Close the run (death or the End Run button) with a scored summary.
    pub fn end_survival_run(&mut self) {
        if !self.is_survival {
            return;
        }
        self.is_survival = false;
        self.current_expected_midis.clear();
        let seconds = (self.clock)() - self.survival_start;
        let difficulty = if self.survival_difficulties.is_empty() {
            1.0
        } else {
            self.survival_difficulties.iter().sum::<f64>() / self.survival_difficulties.len() as f64
        };
        let notes = self.survival_notes as i32;
        let score = SurvivalPolicy::score(notes, seconds, difficulty);
        let is_new_best = score > self.survival_best && self.survival_notes > 0;
        let now = self.now_ms();
        if is_new_best {
            self.survival_best = score;
            if let Some(db) = &mut self.db {
                db.set_setting("survival_best", &score.to_string(), now);
            }
        }
        let error_count = (SurvivalPolicy::START_LIVES - self.survival_lives) as usize;
        if let (Some(db), Some(exercise_id)) = (&mut self.db, self.exercise_id) {
            db.complete_exercise(
                exercise_id,
                now,
                self.survival_notes as i64,
                error_count as i64,
            );
        }
        self.set_phase(Phase::Summary(ExerciseSummary {
            exercise_number: self.exercise_number,
            note_count: self.survival_notes,
            first_try_correct: self.survival_notes,
            error_count,
            mean_latency_ms: None,
            newly_unlocked: None,
            streak: self.streak,
            timing: None,
            bpm: None,
            rhythm_unlocked: None,
            piece_title: None,
            worst_measure: None,
            drill: false,
            self_verified: false,
            survival: Some(SurvivalReport {
                score,
                notes: self.survival_notes,
                notes_per_minute: SurvivalPolicy::notes_per_minute(notes, seconds),
                difficulty,
                best: self.survival_best,
                is_new_best,
            }),
        }));
        agg_gui::animation::request_draw();
    }
}
