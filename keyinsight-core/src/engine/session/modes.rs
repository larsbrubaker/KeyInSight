//! Session modes off the adaptive-training path: free play, micro-drills,
//! repertoire, user profiles, exercise history, the beginner keys strip,
//! and reference playback.

use crate::core::{NoteEvent, PitchSpelling};
use crate::engine::session::{
    Deferred, DrillTotals, InputSource, PacingMode, Phase, SessionEngine, DRILL_LENGTH,
    PLAYBACK_PREVIEW_BPM,
};
use crate::audio::MidiFileEncoder;
use crate::notation::NoteState;
use crate::persistence::ExerciseRecord;
use crate::score::{Exercise, FreePlayScore, MusicXmlEncoder, RepertoirePiece};

impl SessionEngine {
    /// Per-user setting key: the open repertoire piece's slug.
    const ACTIVE_PIECE_SETTING: &'static str = "active_piece_slug";

    // --- Exercise history / diversion ---

    /// True when off the adaptive-training path (repertoire, free play, or
    /// a drill) — the bottom bar offers Resume Training.
    pub fn is_diverted(&self) -> bool {
        self.active_piece.is_some() || self.is_free_play || self.drill_remaining.is_some()
    }

    /// Back to normal adaptive exercises from any mode.
    pub fn resume_training(&mut self) {
        self.is_free_play = false;
        self.active_piece = None;
        self.drill_remaining = None;
        self.pending_replay = None;
        self.persist_active_piece(None);
        self.next_exercise();
    }

    pub fn recent_exercises(&self, limit: usize) -> Vec<ExerciseRecord> {
        self.db
            .as_ref()
            .map(|db| db.recent_exercises(limit))
            .unwrap_or_default()
    }

    /// Practice a stored exercise again (one-shot: its summary's Next
    /// Exercise returns to adaptive generation).
    pub fn practice_exercise(&mut self, spec_json: &str) {
        let Ok(exercise) = serde_json::from_str::<Exercise>(spec_json) else {
            return;
        };
        self.is_free_play = false;
        self.active_piece = None;
        self.drill_remaining = None;
        self.pending_replay = Some(exercise);
        self.next_exercise();
    }

    /// Enter repertoire mode with a piece (replayable via next_exercise).
    pub fn start_piece(&mut self, piece: RepertoirePiece) {
        self.persist_active_piece(Some(&piece.slug));
        self.active_piece = Some(piece);
        self.is_free_play = false;
        self.drill_remaining = None;
        self.next_exercise();
    }

    pub fn exit_repertoire(&mut self) {
        self.active_piece = None;
        self.persist_active_piece(None);
        self.next_exercise();
    }

    /// Remember (per user) which piece is open, so a relaunch reopens it.
    fn persist_active_piece(&mut self, slug: Option<&str>) {
        let now = self.now_ms();
        if let Some(db) = &mut self.db {
            db.set_setting(Self::ACTIVE_PIECE_SETTING, slug.unwrap_or(""), now);
        }
    }

    /// Reopen the piece this user was working on. Bundled slugs only —
    /// imported pieces aren't persisted (their MusicXML lives outside the
    /// app). Returns false when there is nothing to restore.
    pub(crate) fn restore_active_piece(&mut self) -> bool {
        let Some(slug) = self.db.as_ref().and_then(|db| db.setting(Self::ACTIVE_PIECE_SETTING))
        else {
            return false;
        };
        if slug.is_empty() {
            return false;
        }
        let Some(piece) = crate::score::RepertoireLibrary::bundled()
            .into_iter()
            .find(|piece| piece.slug == slug)
        else {
            return false;
        };
        self.start_piece(piece);
        true
    }

    pub fn piece_stats(&self, slug: &str) -> Option<(i64, f64)> {
        self.db.as_ref().and_then(|db| db.piece_stats(slug))
    }

    // --- Users ---

    /// Switch profiles: closes the current recording session, reloads all
    /// per-user state, and starts fresh on a new exercise.
    pub fn switch_user(&mut self, id: i64) {
        let now = self.now_ms();
        {
            let Some(db) = &mut self.db else { return };
            if id == db.active_user_id() {
                return;
            }
        }
        self.stop_playback();
        self.teardown_tempo_run();
        if let (Some(db), Some(session_id)) = (&mut self.db, self.session_id) {
            db.end_session(session_id, now);
        }
        if let Some(db) = &mut self.db {
            db.activate_user(id, now);
        }
        self.load_user_state();
        self.streak = 0;
        self.exercise_number = 0;
        self.is_free_play = false;
        self.active_piece = None;
        self.drill_remaining = None;
        self.pending_replay = None;
        // Restore this user's preferred input source.
        let restored = self.stored_input_source().unwrap_or(InputSource::Keyboard);
        if restored != self.input_source {
            self.apply_input_source(restored);
        }
        if let Some(db) = &mut self.db {
            self.session_id = Some(db.create_session(now, self.backend.display_name()));
        }
        self.refresh_skill();
        if !self.restore_active_piece() {
            self.next_exercise();
        }
    }

    pub fn add_user(&mut self, name: &str) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        let now = self.now_ms();
        let Some(db) = &mut self.db else { return };
        let id = db.create_user(trimmed, now);
        self.switch_user(id);
    }

    pub fn rename_user(&mut self, id: i64, name: &str) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        let Some(db) = &mut self.db else { return };
        db.rename_user(id, trimmed);
        self.users = db.users();
        self.current_user = self
            .users
            .iter()
            .find(|u| u.id == db.active_user_id())
            .cloned();
    }

    pub fn set_two_handed(&mut self, on: bool) {
        if on == self.two_handed {
            return;
        }
        self.two_handed = on;
        let now = self.now_ms();
        if let Some(db) = &mut self.db {
            db.set_setting("two_handed", if on { "1" } else { "0" }, now);
        }
        // Only regenerate when it changes the next thing we'd show.
        if self.active_piece.is_none() && self.drill_remaining.is_none() && !self.is_free_play {
            self.next_exercise();
        }
    }

    // --- Beginner keys strip ---

    /// Settings key for the current context's override.
    fn keys_context_setting(&self) -> String {
        match &self.active_piece {
            Some(piece) => format!("beginner_keys:piece:{}", piece.slug),
            None => "beginner_keys:training".to_string(),
        }
    }

    pub(crate) fn refresh_show_keys(&mut self) {
        let setting_key = self.keys_context_setting();
        let overridden = self.db.as_ref().and_then(|db| db.setting(&setting_key));
        self.show_keys = overridden
            .map(|v| v == "1")
            .unwrap_or(self.keys_user_default);
    }

    /// User-level default (setup section).
    pub fn set_keys_user_default(&mut self, on: bool) {
        if on == self.keys_user_default {
            return;
        }
        self.keys_user_default = on;
        let now = self.now_ms();
        if let Some(db) = &mut self.db {
            db.set_setting("beginner_keys_default", if on { "1" } else { "0" }, now);
        }
        self.refresh_show_keys();
    }

    /// Per-context toggle (bottom bar): persists for this piece (or for
    /// training exercises as a whole).
    pub fn toggle_keys_for_context(&mut self) {
        let new_value = !self.show_keys;
        let setting_key = self.keys_context_setting();
        let now = self.now_ms();
        if let Some(db) = &mut self.db {
            db.set_setting(&setting_key, if new_value { "1" } else { "0" }, now);
        }
        self.refresh_show_keys();
    }

    // --- Free Play mirror ---

    pub fn enter_free_play(&mut self) {
        self.stop_playback();
        self.teardown_tempo_run();
        self.active_piece = None;
        self.drill_remaining = None;
        self.is_free_play = true;
        self.current_expected_midis.clear();
        self.free_play_chords.clear();
        self.free_play_last_onset = 0.0;
        self.free_play_count = 0;
        self.last_free_play_note = None;
        self.matcher = None;
        self.tempo_matcher = None;
        self.render_free_play();
        self.phase = Phase::Playing;
        agg_gui::animation::request_draw();
    }

    pub fn exit_free_play(&mut self) {
        self.is_free_play = false;
        self.next_exercise();
    }

    pub fn clear_free_play(&mut self) {
        self.free_play_chords.clear();
        self.free_play_last_onset = 0.0;
        self.free_play_count = 0;
        self.last_free_play_note = None;
        self.render_free_play();
    }

    pub(crate) fn handle_free_play(&mut self, event: &NoteEvent) {
        if event.kind != crate::core::NoteEventKind::On {
            return;
        }
        // Notes arriving within the chord window sound together — a chord,
        // not a run (both hands landing "simultaneously" spread over tens
        // of ms on real input).
        if !self.free_play_chords.is_empty()
            && event.timestamp - self.free_play_last_onset < FreePlayScore::CHORD_WINDOW_SECONDS
        {
            let last = self.free_play_chords.last_mut().expect("checked non-empty");
            if !last.contains(&event.midi) {
                last.push(event.midi);
            }
        } else {
            self.free_play_chords.push(vec![event.midi]);
            self.free_play_last_onset = event.timestamp;
        }
        self.free_play_count += 1;
        let mut last = self.free_play_chords.last().expect("just pushed").clone();
        last.sort_unstable();
        self.last_free_play_note = Some(
            last.iter()
                .map(|&m| PitchSpelling::name(m))
                .collect::<Vec<_>>()
                .join("+"),
        );
        self.render_free_play();
        agg_gui::animation::request_draw();
    }

    pub(crate) fn render_free_play(&mut self) {
        let mirror = FreePlayScore::build(&self.free_play_chords);
        let xml = MusicXmlEncoder::encode(&mirror);
        let rendered = self.renderer.borrow_mut().render(&xml);
        let Some(rendered) = rendered else { return };
        if !self.bind_rendered(&mirror, &rendered) {
            return;
        }
        self.notation.borrow_mut().load_score();
    }

    // --- Micro-drill ---

    pub fn start_drill(&mut self) {
        self.teardown_tempo_run();
        self.active_piece = None;
        self.is_free_play = false;
        if self.mode == PacingMode::Tempo {
            self.mode = PacingMode::SelfPaced;
        }
        self.drill_remaining = Some(DRILL_LENGTH);
        self.drill_totals = DrillTotals::new();
        self.next_exercise();
    }

    // --- Reference playback ("hear it") ---

    /// Playback is available whenever an exercise is on screen and the
    /// metronome doesn't own the clock (i.e. not mid-tempo-run).
    pub fn can_playback(&self) -> bool {
        if self.exercise.is_none() || self.is_free_play {
            return false;
        }
        !(self.phase == Phase::Playing && self.mode == PacingMode::Tempo)
    }

    pub fn toggle_playback(&mut self) {
        if self.is_playing_back {
            self.stop_playback();
            return;
        }
        if !self.can_playback() {
            return;
        }
        let Some(exercise) = self.exercise.clone() else { return };
        let bpm = PLAYBACK_PREVIEW_BPM;
        let smf = MidiFileEncoder::encode(&exercise, bpm, 0);
        if !self.audio.play_smf(&smf) {
            return;
        }
        self.is_playing_back = true;
        self.playback_started_at = (self.clock)();
        self.playback_generation += 1;
        let generation = self.playback_generation;
        let duration = MidiFileEncoder::duration(&exercise, bpm);
        self.defer_action(duration + 0.25, Deferred::PlaybackDone { generation });
        // Cursor follows the sound, frame-accurately (the widget advances
        // the schedule every painted frame).
        let now = (self.clock)();
        self.notation.borrow_mut().follow_schedule(
            self.event_ids.clone(),
            MidiFileEncoder::event_start_seconds(&exercise, bpm),
            now,
        );
        agg_gui::animation::request_draw();
    }

    pub(crate) fn stop_playback(&mut self) {
        self.playback_generation += 1; // cancels the pending PlaybackDone
        self.audio.stop_smf();
        if !self.is_playing_back {
            return;
        }
        self.finish_playback();
    }

    pub(crate) fn finish_playback(&mut self) {
        self.is_playing_back = false;
        self.notation.borrow_mut().cancel_follow();
        self.restore_note_states();
        agg_gui::animation::request_draw();
    }

    /// The state each note should show based on actual play progress.
    fn natural_state(&self, index: usize) -> Option<NoteState> {
        if let Some(matcher) = &self.matcher {
            if matcher.is_complete() {
                return Some(NoteState::Correct);
            }
            if index < matcher.index() {
                return Some(NoteState::Correct);
            }
            return if index == matcher.index() {
                Some(NoteState::Current)
            } else {
                None
            };
        }
        if let Some(tempo_matcher) = &self.tempo_matcher {
            use crate::engine::TempoResolution;
            return match tempo_matcher.resolutions[index] {
                Some(TempoResolution::Hit { .. }) => Some(NoteState::Correct),
                Some(TempoResolution::Missed) => Some(NoteState::Missed),
                None => {
                    if Some(index) == tempo_matcher.first_unresolved_index() {
                        Some(NoteState::Current)
                    } else {
                        None
                    }
                }
            };
        }
        None
    }

    fn restore_note_states(&mut self) {
        let mut states: Vec<(String, Option<NoteState>)> = Vec::new();
        for (index, ids) in self.event_ids.iter().enumerate() {
            let base = self.natural_state(index);
            for (pos, id) in ids.iter().enumerate() {
                let state = if self.consumed_positions[index].contains(&pos) {
                    Some(NoteState::Correct)
                } else {
                    base
                };
                states.push((id.clone(), state));
            }
        }
        let mut notation = self.notation.borrow_mut();
        for (id, state) in states {
            notation.set_state(&id, state);
        }
    }
}
