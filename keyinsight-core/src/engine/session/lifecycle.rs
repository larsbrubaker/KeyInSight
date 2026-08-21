//! Session lifecycle: start, input-source wiring, exercise generation +
//! binding, and the per-frame tick that replaces the Swift timers/dispatch
//! queues. Exercise completion and attempt recording live in `completion`.

use crate::core::{NoteEvent, PitchSpelling, Rng64};
use crate::input::SimulatedKeyboardBackend;
use crate::engine::session::{Deferred, InputSource, PacingMode, Phase, SessionEngine};
use crate::engine::{RhythmPolicy, SelfPacedMatcher, TempoExpected, TempoMatcher, TempoPolicy};
use crate::notation::NoteState;
use crate::score::{
    DifficultyDescriptors, Exercise, ExerciseGenerator, Hands, MusicXmlEncoder, ScoreNote, Staff,
};
use crate::skill::KeyOption;
use crate::ui::KeyboardLayout;

impl SessionEngine {
    // --- Lifecycle ---

    pub fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;

        if let Some(db) = &mut self.db {
            self.users = db.users();
            self.current_user = self
                .users
                .iter()
                .find(|u| u.id == db.active_user_id())
                .cloned();
        }
        self.load_user_state();
        if let Some(db) = &mut self.db {
            let now = ((self.clock)() * 1000.0) as i64;
            self.session_id = Some(db.create_session(now, self.backend.display_name()));
        }
        self.refresh_skill();

        // Restore the persisted input source.
        if let Some(source) = self.stored_input_source() {
            if source != self.input_source {
                self.input_source = source;
                self.backend = (self.backend_factory)(source);
            }
        }
        self.wire_and_start_backend();
        // Continue where this user left off: reopen their piece, or start
        // a fresh adaptive exercise.
        if !self.restore_active_piece() {
            self.next_exercise();
        }
    }

    pub(crate) fn wire_and_start_backend(&mut self) {
        let queue = std::rc::Rc::clone(&self.event_queue);
        self.backend
            .set_on_event(Some(Box::new(move |event: NoteEvent| {
                queue.borrow_mut().push_back(event);
            })));
        self.backend.start();
    }

    /// Switch input sources; the choice persists per user.
    pub fn set_input_source(&mut self, source: InputSource) {
        if source == self.input_source {
            return;
        }
        self.apply_input_source(source);
        let now = self.now_ms();
        if let Some(db) = &mut self.db {
            db.set_setting("input_source", source.label(), now);
        }
    }

    pub(crate) fn apply_input_source(&mut self, source: InputSource) {
        self.backend.stop();
        self.input_source = source;
        self.octave_offset = 0;
        *self.mic_level.borrow_mut() = 0.0;
        self.backend = (self.backend_factory)(source);
        self.wire_and_start_backend();
        // Mic and self-verified play are self-paced only.
        if !source.supports_timing() {
            if self.is_free_play {
                self.exit_free_play();
            }
            if self.mode == PacingMode::Tempo {
                self.set_mode(PacingMode::SelfPaced);
            }
        }
    }

    /// The user's persisted input source, if any.
    pub(crate) fn stored_input_source(&self) -> Option<InputSource> {
        self.db
            .as_ref()
            .and_then(|db| db.setting("input_source"))
            .and_then(|s| InputSource::from_label(&s))
    }

    pub fn end_session(&mut self) {
        self.stop_playback();
        self.teardown_tempo_run();
        self.backend.stop();
        let now = self.now_ms();
        if let (Some(db), Some(session_id)) = (&mut self.db, self.session_id) {
            db.end_session(session_id, now);
        }
    }

    /// Per-user state: unlocks, adaptive settings, and the lifetime
    /// exercise counter. Values not yet stored for this user reset to
    /// their defaults (a fresh profile starts from the seed range).
    pub(crate) fn load_user_state(&mut self) {
        let Some(db) = &self.db else { return };
        self.users = db.users();
        self.current_user = self
            .users
            .iter()
            .find(|u| u.id == db.active_user_id())
            .cloned();
        self.skill.set_unlocked_count(
            db.unlocked_item_count()
                .map(|c| c as usize)
                .unwrap_or(crate::skill::SEED_COUNT),
        );
        self.tempo_bpm = db
            .setting("tempo_bpm")
            .and_then(|s| s.parse::<f64>().ok())
            .map(|b| b.clamp(TempoPolicy::MIN_BPM, TempoPolicy::MAX_BPM))
            .unwrap_or(TempoPolicy::START_BPM);
        self.rhythm_level = db
            .setting("rhythm_level")
            .and_then(|s| s.parse::<i32>().ok())
            .map(|l| l.clamp(0, RhythmPolicy::MAX_LEVEL))
            .unwrap_or(0);
        self.input_latency_ms = db
            .setting("input_latency_ms")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        self.two_handed = db.setting("two_handed").as_deref() == Some("1");
        self.keys_user_default = db.setting("beginner_keys_default").as_deref() == Some("1");
        self.exercises_completed = db.lifetime_completed_exercise_count();
    }

    pub fn set_mode(&mut self, new_mode: PacingMode) {
        if new_mode == self.mode {
            return;
        }
        self.mode = new_mode;
        self.teardown_tempo_run();
        self.next_exercise();
    }

    /// Stops the exercise clock so the calibration flow can own the
    /// metronome; caller starts a fresh exercise afterwards.
    pub fn prepare_for_calibration(&mut self) {
        self.teardown_tempo_run();
    }

    pub fn set_input_latency(&mut self, ms: f64) {
        self.input_latency_ms = ms;
        let now = self.now_ms();
        if let Some(db) = &mut self.db {
            db.set_setting("input_latency_ms", &ms.to_string(), now);
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

    pub fn next_exercise(&mut self) {
        self.stop_playback();
        self.teardown_tempo_run();
        self.exercise_number += 1;
        self.refresh_skill();

        self.is_free_play = false;
        let exercise: Exercise = if let Some(replay) = self.pending_replay.take() {
            replay
        } else if let Some(piece) = &self.active_piece {
            piece.exercise.clone()
        } else if self.drill_remaining.is_some() {
            ExerciseGenerator::drill_note(
                &self.skill.active_pitch_options(),
                Staff::Treble,
                None,
                &mut self.rng,
            )
        } else {
            self.generator.config.rhythm_level = self.rhythm_level;
            // The "Two hands" toggle maps onto the generator's hands mode
            // (the full hand-mode rotation arrives with the engine port).
            self.generator.config.hands = if self.two_handed {
                Hands::Both
            } else {
                Hands::Right
            };
            let fifths = self.pick_key(&self.skill.available_keys());
            self.generator.config.fifths = fifths;
            self.generator.config.interval_weights = self.skill.interval_weights();
            let pitches = self.skill.active_pitch_options_in_key(fifths);
            self.generator.generate(&pitches, &mut self.rng)
        };
        let xml = MusicXmlEncoder::encode(&exercise);
        let rendered = self.renderer.borrow_mut().render(&xml);
        let Some(rendered) = rendered else {
            self.phase = Phase::Failed("Engraving failed for generated exercise.".to_string());
            return;
        };
        if !self.bind_rendered(&exercise, &rendered) {
            self.phase = Phase::Failed("Engraving failed for generated exercise.".to_string());
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
        // The tempo matcher is monophonic for now: two-voice or chordal
        // content plays self-paced.
        self.content_supports_tempo =
            !exercise.is_two_voice() && self.events.iter().all(|e| e.pitches.len() == 1);
        if self.mode == PacingMode::Tempo && !self.content_supports_tempo {
            self.mode = PacingMode::SelfPaced;
        }
        let key_name = PitchSpelling::key_name(exercise.fifths);
        let hands = if exercise.is_two_voice() { " · two hands" } else { "" };
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

        match self.mode {
            PacingMode::SelfPaced => {
                self.matcher = Some(SelfPacedMatcher::new(exercise.expected_sets()));
                self.tempo_matcher = None;
                self.set_current(0);
                self.current_note_start = (self.clock)();
                self.phase = Phase::Playing;
            }
            PacingMode::Tempo => {
                self.matcher = None;
                self.tempo_matcher = Some(TempoMatcher::new(self.tempo_targets(&exercise)));
                self.count_in_remaining = Some(self.count_in_beats);
                self.beat_in_measure = 0;
                self.set_current(0);
                self.phase = Phase::Playing;
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

    /// Bind a rendered score to the match events by timemap onset (qstamp).
    /// The engraver emits ids in document order — treble voice before
    /// bass; groups are matched to the model's expected onsets and any
    /// extra ids are dropped by pitch. Returns false when alignment fails.
    pub(crate) fn bind_rendered(
        &mut self,
        exercise: &Exercise,
        rendered: &crate::notation::Rendered,
    ) -> bool {
        let events = exercise.match_events();
        let mut groups_by_q: std::collections::HashMap<u64, Vec<String>> =
            std::collections::HashMap::new();
        for (qstamp, ids) in &rendered.note_groups {
            groups_by_q
                .entry(qstamp.to_bits())
                .or_default()
                .extend(ids.iter().cloned());
        }
        let mut bound_ids: Vec<Vec<String>> = Vec::new();
        for event in &events {
            let qstamp = (event.start_units as f64) / 2.0;
            let Some(mut ids) = groups_by_q.get(&qstamp.to_bits()).cloned() else {
                return false;
            };
            if ids.len() != event.pitches.len() {
                let mut remaining = event.pitches.clone();
                let renderer = self.renderer.borrow();
                ids.retain(|id| {
                    let Some(pitch) = renderer.midi_pitch(id) else {
                        return false;
                    };
                    match remaining.iter().position(|&p| p == pitch) {
                        Some(index) => {
                            remaining.remove(index);
                            true
                        }
                        None => false,
                    }
                });
            }
            if ids.len() != event.pitches.len() {
                return false;
            }
            bound_ids.push(ids);
        }
        self.events = events;
        self.event_ids = bound_ids;
        self.note_ids = self.event_ids.iter().flatten().cloned().collect();
        self.consumed_positions = vec![Default::default(); self.events.len()];
        self.note_by_id.clear();
        for (ids, event) in self.event_ids.iter().zip(&self.events) {
            for (offset, id) in ids.iter().enumerate() {
                self.note_by_id.insert(
                    id.clone(),
                    ScoreNote::note(event.pitches[offset], event.durations[offset])
                        .with_staff(event.staves[offset]),
                );
            }
        }
        true
    }

    pub(crate) fn set_current(&mut self, index: usize) {
        let mut notation = self.notation.borrow_mut();
        for id in &self.event_ids[index] {
            notation.set_state(id, Some(NoteState::Current));
        }
        drop(notation);
        self.current_expected_midis = self.events[index].pitches.iter().copied().collect();
    }

    // --- The frame tick (replaces Swift timers + dispatch queues) ---

    /// Drive the engine: drain queued input events, run deferred actions,
    /// pump the metronome scheduler + sweep. Shells call this every frame.
    pub fn tick(&mut self) {
        self.process_mic_input();
        // Input events.
        loop {
            let event = self.event_queue.borrow_mut().pop_front();
            match event {
                Some(event) => self.handle(event),
                None => break,
            }
        }

        // Deferred actions whose deadline passed.
        let now = (self.clock)();
        let mut due: Vec<Deferred> = Vec::new();
        self.deferred.retain_mut(|(deadline, action)| {
            if *deadline <= now {
                // Move the action out; the slot is dropped by retain.
                due.push(std::mem::replace(
                    action,
                    Deferred::ClearHeardUncertain,
                ));
                false
            } else {
                true
            }
        });
        for action in due {
            self.run_deferred(action);
        }

        // Tempo run: metronome click scheduling + the miss sweep
        // (the Swift 1/30 s sweep timer).
        self.metronome.schedule_ahead(now);
        if self.sweep_running {
            self.sweep_tick();
        }

        // Keep the frame loop hot while time-driven work is pending
        // (replaces the Swift timers waking the main loop).
        if self.sweep_running
            || !self.deferred.is_empty()
            || self.notation.borrow().is_following()
            || !self.event_queue.borrow().is_empty()
        {
            agg_gui::animation::request_draw();
        }
    }

    /// Forward a computer-keyboard event to the simulated backend (the
    /// Swift NSEvent monitor, re-homed to agg-gui key events). Returns true
    /// when consumed (a mapped piano key / octave shifter).
    pub fn handle_simulated_key(&mut self, ch: char, is_down: bool, is_repeat: bool) -> bool {
        let now = (self.clock)();
        let Some(sim) = self
            .backend
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<SimulatedKeyboardBackend>())
        else {
            return false;
        };
        let consumed = sim.handle_key(ch, is_down, is_repeat, now);
        self.octave_offset = sim.octave_offset();
        if consumed {
            // Deliver immediately — feedback the same frame as the press.
            self.tick();
        }
        consumed
    }

    fn run_deferred(&mut self, action: Deferred) {
        match action {
            Deferred::ClearWrongKeyFlash { midi } => {
                if self.wrong_key_flash == Some(midi) {
                    self.wrong_key_flash = None;
                    agg_gui::animation::request_draw();
                }
            }
            Deferred::ClearHeardUncertain => {
                self.heard_uncertain = false;
                agg_gui::animation::request_draw();
            }
            Deferred::RestoreTempoCurrent { index } => {
                let still_pending = self
                    .tempo_matcher
                    .as_ref()
                    .map(|m| index < m.resolutions.len() && m.resolutions[index].is_none())
                    .unwrap_or(false);
                if still_pending {
                    self.notation
                        .borrow_mut()
                        .set_state(&self.note_ids[index], Some(NoteState::Current));
                }
            }
            Deferred::TempoFinish => self.finish_exercise(),
            Deferred::AutoAdvance { generation } => {
                let in_summary = matches!(self.phase, Phase::Summary(_));
                if self.input_source == InputSource::Midi
                    && self.exercise_number == generation
                    && self.active_piece.is_none()
                    && in_summary
                {
                    self.next_exercise();
                }
            }
            Deferred::PlaybackDone { generation } => {
                if self.playback_generation == generation {
                    self.audio.stop_smf();
                    self.finish_playback();
                }
            }
        }
    }
}
