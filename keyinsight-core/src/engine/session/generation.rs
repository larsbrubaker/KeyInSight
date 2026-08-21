//! Exercise generation: `next_exercise` (replay / piece / drill card /
//! adaptive training content), hand selection, and the adaptive generator
//! configuration drawn from the skill models.

use crate::core::{PitchSpelling, Rng64};
use crate::engine::session::{HandMode, PacingMode, Phase, SessionEngine};
use crate::engine::{SelfPacedMatcher, TempoExpected, TempoMatcher};
use crate::score::{
    ChordShape, DifficultyDescriptors, Exercise, ExerciseGenerator, Hands, MusicXmlEncoder, Staff,
};
use crate::skill::{KeyOption, SkillModel, SEED_COUNT};
use crate::ui::KeyboardLayout;

impl SessionEngine {
    // --- Hand selection ---

    pub(crate) fn resolve_hands(&mut self) -> Hands {
        match self.hand_mode {
            HandMode::Right => Hands::Right,
            HandMode::Left => Hands::Left,
            HandMode::Both => Hands::Both,
            HandMode::Auto => self.auto_hands(),
        }
    }

    /// Auto rotation: weakness-weighted between the hands (the weaker hand
    /// drills more; unseen items make a hand "weak"), with two-hand
    /// exercises joining at an equal share once the bass seed range is
    /// mastered — adaptive progression into hands-together (OQ-23).
    fn auto_hands(&mut self) -> Hands {
        let right = self.skill.mean_active_weight();
        let left = self.bass_skill.mean_active_weight();
        let mut options: Vec<(Hands, f64)> = vec![(Hands::Right, right), (Hands::Left, left)];
        if self.bass_skill.unlocked_count() > SEED_COUNT {
            options.push((Hands::Both, (right + left) / 2.0));
        }
        let total: f64 = options.iter().map(|(_, w)| w).sum();
        let mut roll = self.rng.next_f64_below(total);
        for (hands, weight) in options {
            roll -= weight;
            if roll < 0.0 {
                return hands;
            }
        }
        Hands::Right
    }

    /// Drill cards are single notes, so a card trains one hand: the chosen
    /// one, or a per-card weakness-weighted pick in Auto (Both drills the
    /// reading skill that two-hand melodies use — treble).
    pub(crate) fn drill_staff(&mut self) -> Staff {
        match self.hand_mode {
            HandMode::Left => Staff::Bass,
            HandMode::Right | HandMode::Both => Staff::Treble,
            HandMode::Auto => {
                let right = self.skill.mean_active_weight();
                let left = self.bass_skill.mean_active_weight();
                if self.rng.next_f64_below(right + left) < left {
                    Staff::Bass
                } else {
                    Staff::Treble
                }
            }
        }
    }

    pub(crate) fn pick_key(&mut self, keys: &[KeyOption]) -> i32 {
        let total: f64 = keys.iter().map(|k| k.weight).sum();
        if total <= 0.0 {
            return 0;
        }
        let mut roll = self.rng.next_f64_below(total);
        for key in keys {
            roll -= key.weight;
            if roll < 0.0 {
                return key.fifths;
            }
        }
        0
    }

    /// Target times (ms on the metronome clock) per sounded note, after the
    /// count-in.
    pub fn tempo_targets(&self, exercise: &Exercise) -> Vec<TempoExpected> {
        let unit_ms = (60_000.0 / self.tempo_bpm) / 2.0;
        let count_in_ms = self.count_in_beats as f64 * (60_000.0 / self.tempo_bpm);
        exercise
            .sounded_notes()
            .iter()
            .zip(exercise.sounded_note_start_units())
            .map(|(note, start)| TempoExpected {
                midi: note.midi.expect("sounded notes carry a pitch"),
                target_ms: count_in_ms + start as f64 * unit_ms,
            })
            .collect()
    }

    /// Adaptive training content: the generator configured from the skill
    /// models for the resolved hands.
    fn generate_training_exercise(&mut self) -> Exercise {
        self.generator.config.measures = 2;
        self.generator.config.rhythm_level = self.rhythm_level;
        self.generator.config.melody_in_bass = false;
        let hands = self.resolve_hands();
        self.generator.config.hands = hands;
        // Left-hand exercises draw pitches (and key availability) from the
        // bass model; two-hand melodies stay treble-driven.
        let model: &SkillModel = if hands == Hands::Left {
            &self.bass_skill
        } else {
            &self.skill
        };
        let keys = model.available_keys();
        let fifths = self.pick_key(&keys);
        let model: &SkillModel = if hands == Hands::Left {
            &self.bass_skill
        } else {
            &self.skill
        };
        let active_options = model.active_pitch_options_in_key(fifths);
        self.generator.config.fifths = fifths;
        self.generator.config.interval_weights = self.skill.interval_weights();
        let midis: Vec<u8> = active_options.iter().map(|p| p.midi).collect();
        self.generator.config.transition_weights = self.skill.transition_weights(&midis);
        self.generator.config.allowed_steps = self.skill.unlocked_interval_sizes();
        self.generator.config.probe_step = self.skill.next_locked_interval_size();
        // Chords are the right hand's ladder (v1), and stay out of tempo
        // mode so tempo content remains matchable (monophonic).
        let chords_eligible = hands == Hands::Right && self.mode == PacingMode::SelfPaced;
        self.generator.config.chord_shapes = if chords_eligible {
            self.skill
                .unlocked_chord_shapes()
                .iter()
                .filter_map(|name| {
                    ChordShape::by_skill_item(name)
                        .map(|shape| (shape, self.skill.chord_shape_weight(name)))
                })
                .collect()
        } else {
            Vec::new()
        };
        // Chord probes start once the first range unlock is earned — "you
        // can read the seed range; now try two notes at once".
        self.generator.config.probe_chord_shape =
            if chords_eligible && self.skill.unlocked_count() > SEED_COUNT {
                self.skill
                    .next_locked_chord_shape()
                    .and_then(ChordShape::by_skill_item)
            } else {
                None
            };
        self.generator.generate(&active_options, &mut self.rng)
    }

    pub fn next_exercise(&mut self) {
        self.stop_playback();
        self.teardown_tempo_run();
        self.inspection = None;
        self.stats_suppressed = false;
        self.storm_detector.reset();
        self.exercise_number += 1;
        self.refresh_skill();

        self.is_free_play = false;
        let exercise: Exercise = if let Some(replay) = self.pending_replay.take() {
            replay
        } else if let Some(piece) = &self.active_piece {
            piece.exercise.clone()
        } else if self.drill_remaining.is_some() {
            let staff = self.drill_staff();
            let model = if staff == Staff::Bass {
                &self.bass_skill
            } else {
                &self.skill
            };
            ExerciseGenerator::drill_note(&model.active_pitch_options(), staff, None, &mut self.rng)
        } else {
            self.generate_training_exercise()
        };
        let xml = MusicXmlEncoder::encode(&exercise);
        let rendered = self.renderer.borrow_mut().render(&xml);
        let Some(rendered) = rendered else {
            self.set_phase(Phase::Failed(
                "Engraving failed for generated exercise.".to_string(),
            ));
            return;
        };
        if !self.bind_rendered(&exercise, &rendered) {
            self.set_phase(Phase::Failed(
                "Engraving failed for generated exercise.".to_string(),
            ));
            return;
        }

        self.note_count = self.events.len();
        self.current_note_index = 0;
        self.errors_this_exercise = 0;
        self.errors_on_current_note = 0;
        self.first_try_correct = 0;
        self.latencies_ms.clear();
        self.tempo_error_indices.clear();
        self.self_verify_attempts = 0;
        self.measure_by_event = exercise.event_measure_indices();
        self.errors_by_measure = vec![0; exercise.measure_count()];
        // Octave anchoring is monophonic-only: with chords or two hands,
        // pitch-class matching is ambiguous.
        self.octave_anchor = Default::default();
        self.anchor_eligible = self.events.iter().all(|e| e.pitches.len() == 1);
        self.anchored_octaves = 0;
        // The tempo matcher is monophonic: any moment with two pitches
        // (chords, hands together) plays self-paced. A single line on
        // either staff tempo-scores fine.
        self.content_supports_tempo = self.events.iter().all(|e| e.pitches.len() == 1);
        // Drills run self-paced without overwriting the user's choice.
        // (Survival joins this guard when it lands.)
        self.active_pacing = if self.mode == PacingMode::Tempo
            && self.content_supports_tempo
            && self.drill_remaining.is_none()
        {
            PacingMode::Tempo
        } else {
            PacingMode::SelfPaced
        };
        let key_name = PitchSpelling::key_name(exercise.fifths);
        let hands = if exercise.is_bass_only() {
            " · left hand"
        } else if exercise.is_two_voice() {
            " · two hands"
        } else {
            ""
        };
        self.exercise_info = Some(format!(
            "{key_name} · {}/4 · {} notes{hands}",
            exercise.beats_per_measure,
            self.events.len()
        ));

        self.notation.borrow_mut().load_score();
        // Keyboard strip: fit the content's range; context may have changed.
        let all_pitches: Vec<u8> = self.events.iter().flat_map(|e| e.pitches.clone()).collect();
        self.keyboard_layout = KeyboardLayout::covering(
            all_pitches.iter().min().copied().unwrap_or(48),
            all_pitches.iter().max().copied().unwrap_or(84),
        );
        self.refresh_show_keys();

        match self.active_pacing {
            PacingMode::SelfPaced => {
                self.matcher = Some(SelfPacedMatcher::new(exercise.expected_sets()));
                self.tempo_matcher = None;
                self.set_current(0);
                self.current_note_start = (self.clock)();
                self.set_phase(Phase::Playing);
            }
            PacingMode::Tempo => {
                self.matcher = None;
                self.tempo_matcher = Some(TempoMatcher::new(self.tempo_targets(&exercise)));
                self.count_in_remaining = Some(self.count_in_beats);
                self.beat_in_measure = 0;
                self.set_current(0);
                self.set_phase(Phase::Playing);
                let now = (self.clock)();
                self.metronome.start(
                    self.tempo_bpm,
                    exercise.beats_per_measure,
                    now + 0.35,
                    now,
                );
                self.sweep_running = true;
            }
        }

        let now = self.now_ms();
        if let (Some(db), Some(session_id)) = (&mut self.db, self.session_id) {
            let spec = serde_json::to_string(&exercise).unwrap_or_else(|_| "{}".to_string());
            let targeted = self.skill.targeted_item_names();
            let targeted_json = serde_json::to_string(&targeted).ok();
            self.exercise_id = Some(db.create_exercise(
                session_id,
                self.exercise_number,
                &spec,
                now,
                Some(DifficultyDescriptors::compute(&exercise).json()),
                targeted_json,
            ));
        }
        self.exercise = Some(exercise);
        agg_gui::animation::request_draw();
    }
}
