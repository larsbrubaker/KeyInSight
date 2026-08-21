//! Session lifecycle: start, per-user state, input-source wiring, and the
//! per-frame tick that replaces the Swift timers/dispatch queues. Exercise
//! generation lives in `generation`, completion in `completion`.

use crate::core::NoteEvent;
use crate::input::SimulatedKeyboardBackend;
use crate::engine::session::{Deferred, HandMode, InputSource, PacingMode, Phase, SessionEngine};
use crate::engine::{RhythmPolicy, TempoPolicy};
use crate::notation::NoteState;

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
        self.bass_skill.set_unlocked_count(
            db.setting("bass_unlocked_count")
                .and_then(|s| s.parse::<usize>().ok())
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
        self.rhythm_clean_streak = db
            .setting("rhythm_clean_streak")
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        self.skill.set_interval_unlocked_count(
            db.setting("interval_unlocked_count")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0),
        );
        self.skill.set_chord_unlocked_count(
            db.setting("chord_unlocked_count")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0),
        );
        self.input_latency_ms = db
            .setting("input_latency_ms")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        // Pre-hand-mode profiles kept their "Two hands" toggle choice.
        self.hand_mode = db
            .setting("hand_mode")
            .and_then(|s| HandMode::from_raw_value(&s))
            .unwrap_or(if db.setting("two_handed").as_deref() == Some("1") {
                HandMode::Both
            } else {
                HandMode::Right
            });
        self.keys_user_default = db.setting("beginner_keys_default").as_deref() == Some("1");
        self.follow_octave = db.setting("follow_octave").as_deref() != Some("0");
        self.survival_best = db
            .setting("survival_best")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
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

    pub fn set_hand_mode(&mut self, new_mode: HandMode) {
        if new_mode == self.hand_mode {
            return;
        }
        self.hand_mode = new_mode;
        let now = self.now_ms();
        if let Some(db) = &mut self.db {
            db.set_setting("hand_mode", new_mode.raw_value(), now);
        }
        // Only regenerate when it changes the next thing we'd show.
        if self.active_piece.is_none() && !self.drill_active && !self.is_free_play {
            self.next_exercise();
        }
    }

    /// Octave-following on/off (player profile); applies from the next
    /// note — mid-exercise anchors reset with the next exercise.
    pub fn set_follow_octave(&mut self, on: bool) {
        if on == self.follow_octave {
            return;
        }
        self.follow_octave = on;
        let now = self.now_ms();
        if let Some(db) = &mut self.db {
            db.set_setting("follow_octave", if on { "1" } else { "0" }, now);
        }
        if !on {
            self.octave_anchor = Default::default();
            self.anchored_octaves = 0;
        }
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

    // --- The frame tick (replaces Swift timers + dispatch queues) ---

    /// Drive the engine: drain queued input events, run deferred actions,
    /// pump the metronome scheduler + sweep. Shells call this every frame.
    pub fn tick(&mut self) {
        self.process_mic_input();
        self.process_midi_input();
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
            Deferred::SurvivalWindowSwap { generation } => self.advance_survival_window(generation),
        }
    }
}
