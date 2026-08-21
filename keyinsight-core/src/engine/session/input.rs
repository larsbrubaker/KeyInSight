//! Input handling: routing NoteEvents through the octave anchor into the
//! self-paced or tempo matcher, feedback coloring, and the miss sweep.

use crate::core::{NoteEvent, NoteEventKind, PitchSpelling};
use crate::engine::session::{
    Deferred, InputSource, PacingMode, Phase, SessionEngine, TempoDebug, CONFIDENCE_THRESHOLD,
    LATENCY_OUTLIER_MS, SURVIVAL_SWAP_DELAY,
};
use crate::engine::{SelfPacedOutcome, TempoOutcome, Timing};
use crate::notation::NoteState;
use crate::score::Staff;

impl SessionEngine {
    /// Drive the microphone backend (called from `tick`): drain captured
    /// audio, detect the exercise's candidate notes, and surface the
    /// input level for the side panel meter.
    pub(crate) fn process_mic_input(&mut self) {
        // Candidates: the notes worth listening for right now. Free play
        // has no expectations, so listen across the whole keyboard.
        let candidates: Vec<u8> = if self.is_free_play {
            (21..=108).collect()
        } else {
            self.current_expected_midis.iter().copied().collect()
        };
        let now = (self.clock)();
        let Some(mic) = self
            .backend
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<crate::input::MicBackend>())
        else {
            return;
        };
        mic.process(now, &candidates);
        let level = mic.level();
        *self.mic_level.borrow_mut() = level;
    }

    /// Drive the MIDI backend (called from `tick`): drain the ports'
    /// captured packets into note events (stamped with their packet
    /// times) and keep the hot-plug rescan ticking.
    pub(crate) fn process_midi_input(&mut self) {
        let now = (self.clock)();
        let Some(midi) = self
            .backend
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<crate::input::MidiBackend>())
        else {
            return;
        };
        midi.process(now);
    }

    pub fn handle(&mut self, event: NoteEvent) {
        if let Some(calibration_tap) = &self.calibration_tap {
            if event.kind == NoteEventKind::On {
                calibration_tap(event.timestamp);
            }
            return;
        }
        if self.phase != Phase::Playing {
            return;
        }
        // Confidence gating (mic): uncertain hearings give gentle feedback,
        // never a wrong mark.
        if event.kind == NoteEventKind::On && event.confidence < CONFIDENCE_THRESHOLD {
            self.heard_uncertain = true;
            self.defer_action(1.2, Deferred::ClearHeardUncertain);
            agg_gui::animation::request_draw();
            return;
        }
        if self.is_free_play {
            self.handle_free_play(&event);
            return;
        }
        let anchored = self.anchor(event);
        match self.active_pacing {
            PacingMode::SelfPaced => self.handle_self_paced(anchored),
            PacingMode::Tempo => self.handle_tempo(anchored),
        }
    }

    /// Apply the per-exercise octave anchor (monophonic practice on exact
    /// input sources): the first pitch-class match sets the octave; free
    /// play and mic input are untouched.
    fn anchor(&mut self, event: NoteEvent) -> NoteEvent {
        if !self.follow_octave || !self.anchor_eligible || !self.input_source.supports_timing() {
            return event;
        }
        let midi = if event.kind == NoteEventKind::On {
            let expected = self.current_expected_midi();
            let midi = self.octave_anchor.process_note_on(event.midi, expected);
            self.anchored_octaves = self.octave_anchor.user_octaves();
            midi
        } else {
            self.octave_anchor.apply(event.midi)
        };
        if midi == event.midi {
            return event;
        }
        NoteEvent { midi, ..event }
    }

    fn handle_self_paced(&mut self, event: NoteEvent) {
        let Some(matcher) = &mut self.matcher else { return };

        if event.kind == NoteEventKind::Off {
            let index = matcher.index();
            self.log(&event, "off", Some(index), None);
            return;
        }

        match matcher.consume_note_on_at(event.midi, event.timestamp) {
            SelfPacedOutcome::Matched {
                index,
                set_complete,
                exercise_complete,
            } => {
                self.log(&event, "correct", Some(index), None);
                self.notation.borrow_mut().clear_ghost();
                self.color_matched(event.midi, index);

                let was_error = self.errors_on_current_note > 0;
                // A latency past the outlier bar is a BREAK, not slowness —
                // don't let a coffee poison the item's speed EWMA.
                let raw_latency_ms = (event.timestamp - self.current_note_start) * 1000.0;
                let latency_ms: Option<f64> = if raw_latency_ms > LATENCY_OUTLIER_MS {
                    None
                } else {
                    Some(raw_latency_ms)
                };
                let clean_latency = if was_error { None } else { latency_ms };
                let staff = self.staff_for(event.midi, index);
                self.record_attempt(event.midi, staff, was_error, clean_latency);
                self.record_interval_attempt(index, was_error, clean_latency);
                if !set_complete {
                    return;
                }

                // Event-level bookkeeping happens when the full set lands.
                if let Some(latency_ms) = latency_ms {
                    self.latencies_ms.push(latency_ms);
                }
                if !was_error {
                    self.first_try_correct += 1;
                    self.streak += 1;
                    // A clean event means intentional play: recording resumes.
                    if self.stats_suppressed {
                        self.stats_suppressed = false;
                        self.storm_detector.reset();
                    }
                }
                self.record_chord_shape_attempt(index, was_error, clean_latency);
                self.errors_on_current_note = 0;
                if self.is_survival {
                    self.survival_notes += 1;
                }
                if exercise_complete {
                    self.finish_exercise();
                } else {
                    self.current_note_index = index + 1;
                    self.set_current(index + 1);
                    self.current_note_start = (self.clock)();
                    // Crossing the survival seam: the next line is already
                    // on screen and slides up normally; swap the window
                    // once that slide settles.
                    if self.is_survival && index + 1 == self.survival_seam_events {
                        let generation = self.survival_window_gen;
                        self.defer_action(
                            SURVIVAL_SWAP_DELAY,
                            Deferred::SurvivalWindowSwap { generation },
                        );
                    }
                }
            }
            SelfPacedOutcome::Restarted { index, played } => {
                // The chord broke apart (a member landed outside the
                // window): error, and only the late strike carries into the
                // new attempt.
                self.log(&event, "chord_break", Some(index), None);
                self.errors_on_current_note += 1;
                self.errors_this_exercise += 1;
                self.record_measure_error(index);
                self.streak = 0;
                let staff = self.staff_for(played, index);
                self.record_attempt(played, staff, true, None);
                self.reset_event_marks(index, played);
                self.survival_life_lost();
            }
            SelfPacedOutcome::Wrong { index, played } => {
                self.log(&event, "wrong", Some(index), None);
                self.errors_on_current_note += 1;
                self.errors_this_exercise += 1;
                self.record_measure_error(index);
                self.streak = 0;
                self.mark_wrong(index);
                self.show_ghost(played, index);
                self.flash_wrong_key(played);
                // Mash guard: a burst of wrong strikes is noise, not practice.
                if self.storm_detector.record_wrong(event.timestamp) {
                    self.stats_suppressed = true;
                }
                // Drill correction: reveal the keyboard with the right key
                // lit and name both notes — then require the right key to
                // move on.
                if self.drill_active {
                    if let Some(&expected) = self.events[index].pitches.first() {
                        self.drill_hint_keys = true;
                        self.inspection = Some(format!(
                            "That's {} — the card is {}",
                            PitchSpelling::name(played),
                            PitchSpelling::name(expected)
                        ));
                    }
                }
                self.survival_life_lost();
            }
            SelfPacedOutcome::Ignored => {
                let index = self.matcher.as_ref().map(|m| m.index());
                self.log(&event, "ignored", index, None);
            }
        }
        agg_gui::animation::request_draw();
    }

    /// After a broken chord attempt: only `kept` stays marked; the other
    /// members return to "play me" state.
    fn reset_event_marks(&mut self, index: usize, kept: u8) {
        let event = &self.events[index];
        self.consumed_positions[index] = event
            .pitches
            .iter()
            .position(|&p| p == kept)
            .into_iter()
            .collect();
        {
            let mut notation = self.notation.borrow_mut();
            for (pos, id) in self.event_ids[index].iter().enumerate() {
                let state = if self.consumed_positions[index].contains(&pos) {
                    NoteState::Correct
                } else {
                    NoteState::Current
                };
                notation.set_state(id, Some(state));
            }
        }
        self.refresh_expected_from_unconsumed(index);
    }

    /// Color the notehead(s) whose pitch was just played. A pitch doubled
    /// across staves (cross-staff unison in imported scores) is one
    /// physical key — the single press satisfies every matching notehead.
    fn color_matched(&mut self, pitch: u8, index: usize) {
        let event = &self.events[index];
        let positions: Vec<usize> = (0..event.pitches.len())
            .filter(|&p| {
                event.pitches[p] == pitch && !self.consumed_positions[index].contains(&p)
            })
            .collect();
        {
            let mut notation = self.notation.borrow_mut();
            for pos in positions {
                self.consumed_positions[index].insert(pos);
                notation.set_state(&self.event_ids[index][pos], Some(NoteState::Correct));
            }
        }
        self.refresh_expected_from_unconsumed(index);
    }

    /// Keyboard strip: only the still-unplayed members stay highlighted.
    fn refresh_expected_from_unconsumed(&mut self, index: usize) {
        let event = &self.events[index];
        self.current_expected_midis = (0..event.pitches.len())
            .filter(|p| !self.consumed_positions[index].contains(p))
            .map(|p| event.pitches[p])
            .collect();
    }

    pub(crate) fn staff_for(&self, pitch: u8, index: usize) -> Staff {
        let event = &self.events[index];
        event
            .pitches
            .iter()
            .position(|&p| p == pitch)
            .map(|p| event.staves[p])
            .unwrap_or(Staff::Treble)
    }

    /// Wrong-note flash: unconsumed members of the current event go red;
    /// already-matched members stay green.
    fn mark_wrong(&mut self, index: usize) {
        let mut notation = self.notation.borrow_mut();
        for (pos, id) in self.event_ids[index].iter().enumerate() {
            if !self.consumed_positions[index].contains(&pos) {
                notation.set_state(id, Some(NoteState::Wrong));
            }
        }
    }

    pub(crate) fn record_measure_error(&mut self, event_index: usize) {
        if event_index >= self.measure_by_event.len() {
            return;
        }
        self.errors_by_measure[self.measure_by_event[event_index]] += 1;
    }

    fn handle_tempo(&mut self, event: NoteEvent) {
        if self.tempo_matcher.is_none() {
            return;
        }
        let now_ms = self.metronome.milliseconds_since_start(event.timestamp) - self.input_latency_ms;

        if event.kind == NoteEventKind::Off {
            self.log(&event, "off", None, None);
            return;
        }

        let outcome = self
            .tempo_matcher
            .as_mut()
            .expect("checked above")
            .consume_note_on(event.midi, now_ms);
        match outcome {
            TempoOutcome::Hit {
                index,
                timing,
                offset_ms,
                exercise_complete,
            } => {
                let classification = match timing {
                    Timing::OnTime => "hit_onTime",
                    Timing::Early => "hit_early",
                    Timing::Late => "hit_late",
                };
                self.log(&event, classification, Some(index), Some(offset_ms));
                {
                    let mut notation = self.notation.borrow_mut();
                    notation.clear_ghost();
                    notation.set_state(&self.note_ids[index], Some(NoteState::Correct));
                    if timing != Timing::OnTime {
                        notation.add_tick(&self.note_ids[index], timing == Timing::Early);
                    }
                }
                let was_error = self.tempo_error_indices.contains(&index);
                if !was_error {
                    self.first_try_correct += 1;
                    self.streak += 1;
                }
                let midi = self.events[index].pitches[0];
                let staff = self.events[index].staves[0];
                self.record_attempt(midi, staff, was_error, None);
                self.record_interval_attempt(index, was_error, None);
                self.advance_tempo_cursor();
                if exercise_complete {
                    self.schedule_tempo_finish();
                }
            }
            TempoOutcome::Wrong {
                nearest_index,
                played,
            } => {
                self.log(&event, "wrong", Some(nearest_index), None);
                self.errors_this_exercise += 1;
                self.record_measure_error(nearest_index);
                self.streak = 0;
                self.tempo_error_indices.insert(nearest_index);
                self.flash_wrong(nearest_index);
                self.show_ghost(played, nearest_index);
                self.flash_wrong_key(played);
            }
            TempoOutcome::Ignored => {
                self.log(&event, "ignored", None, None);
            }
        }
        agg_gui::animation::request_draw();
    }

    /// Ghost anchors to the expected pitch nearest what was played.
    fn show_ghost(&mut self, played: u8, index: usize) {
        let event = &self.events[index];
        let Some(pos) = (0..event.pitches.len()).min_by_key(|&p| {
            (event.pitches[p] as i32 - played as i32).abs()
        }) else {
            return;
        };
        let offset = PitchSpelling::diatonic_index(played)
            - PitchSpelling::diatonic_index(event.pitches[pos]);
        self.notation
            .borrow_mut()
            .show_ghost(&self.event_ids[index][pos], offset);
    }

    /// Wrong-pitch flash in tempo mode: red now, back to current if the
    /// note is still pending shortly after.
    fn flash_wrong(&mut self, index: usize) {
        self.notation
            .borrow_mut()
            .set_state(&self.note_ids[index], Some(NoteState::Wrong));
        self.defer_action(0.25, Deferred::RestoreTempoCurrent { index });
    }

    pub(crate) fn flash_wrong_key(&mut self, midi: u8) {
        self.wrong_key_flash = Some(midi);
        self.defer_action(0.6, Deferred::ClearWrongKeyFlash { midi });
    }

    // --- Tempo run plumbing ---

    pub(crate) fn sweep_tick(&mut self) {
        if self.phase != Phase::Playing || self.active_pacing != PacingMode::Tempo {
            return;
        }
        let Some(exercise_beats) = self.exercise.as_ref().map(|e| e.beats_per_measure) else {
            return;
        };
        if self.tempo_matcher.is_none() {
            return;
        }

        let now = (self.clock)();
        let beat = self.metronome.beat_index(now);
        if beat < self.count_in_beats {
            self.count_in_remaining = Some(if beat < 0 {
                self.count_in_beats
            } else {
                self.count_in_beats - beat
            });
        } else {
            self.count_in_remaining = None;
            self.beat_in_measure =
                (beat - self.count_in_beats + self.start_beat_offset) % exercise_beats;
        }

        let now_ms = self.metronome.milliseconds_since_start(now) - self.input_latency_ms;
        let missed = self
            .tempo_matcher
            .as_mut()
            .expect("checked above")
            .sweep(now_ms);
        if !missed.is_empty() {
            for &index in &missed {
                self.notation
                    .borrow_mut()
                    .set_state(&self.note_ids[index], Some(NoteState::Missed));
                self.errors_this_exercise += 1;
                self.record_measure_error(index);
                self.streak = 0;
                let midi = self.events[index].pitches[0];
                let staff = self.events[index].staves[0];
                self.record_attempt(midi, staff, true, None);
                self.record_interval_attempt(index, true, None);
            }
            self.advance_tempo_cursor();
            if self.tempo_matcher.as_ref().is_some_and(|m| m.is_complete()) {
                self.schedule_tempo_finish();
            }
            agg_gui::animation::request_draw();
        }
    }

    fn advance_tempo_cursor(&mut self) {
        let Some(tempo_matcher) = &self.tempo_matcher else { return };
        if let Some(next) = tempo_matcher.first_unresolved_index() {
            self.current_note_index = next;
            self.notation
                .borrow_mut()
                .set_state(&self.note_ids[next], Some(NoteState::Current));
            self.current_expected_midis = self.events[next].pitches.iter().copied().collect();
        } else {
            self.current_note_index = self.note_count.saturating_sub(1);
            self.current_expected_midis.clear();
        }
    }

    fn schedule_tempo_finish(&mut self) {
        if self.tempo_finish_scheduled {
            return;
        }
        self.tempo_finish_scheduled = true;
        self.defer_action(0.4, Deferred::TempoFinish);
    }

    // --- Demo / UI observability ---

    pub fn current_expected_midi(&self) -> Option<u8> {
        if self.phase != Phase::Playing {
            return None;
        }
        match self.active_pacing {
            PacingMode::SelfPaced => {
                let matcher = self.matcher.as_ref()?;
                if matcher.is_complete() || self.events[matcher.index()].pitches.len() != 1 {
                    return None;
                }
                Some(self.events[matcher.index()].pitches[0])
            }
            PacingMode::Tempo => {
                let tempo_matcher = self.tempo_matcher.as_ref()?;
                let index = tempo_matcher.first_unresolved_index()?;
                Some(tempo_matcher.expected[index].midi)
            }
        }
    }

    /// Tempo-run observability for the scripted demo: the targets, the
    /// metronome clock now, and the resolutions — None outside a running
    /// tempo exercise (the Swift `tempoDebug`).
    pub fn tempo_debug(&self) -> Option<TempoDebug> {
        if self.active_pacing != PacingMode::Tempo || !self.metronome.is_running() {
            return None;
        }
        let tempo_matcher = self.tempo_matcher.as_ref()?;
        Some(TempoDebug {
            targets: tempo_matcher.expected.clone(),
            now_ms: self.metronome.milliseconds_since_start((self.clock)()),
            resolutions: tempo_matcher.resolutions.clone(),
        })
    }

    /// The vocabulary hover entry point (the notation controller's
    /// on_inspect routes here through the app).
    pub fn inspect(&mut self, kind: &str, id: &str) {
        // Empty kind = hover ended.
        if kind.is_empty() {
            self.inspection = None;
            agg_gui::animation::request_draw();
            return;
        }
        let text = if kind == "note" {
            self.note_by_id
                .get(id)
                .map(crate::notation::NotationVocabulary::describe_note)
        } else {
            crate::notation::NotationVocabulary::describe(
                kind,
                self.exercise.as_ref().map(|e| e.fifths).unwrap_or(0),
                self.exercise
                    .as_ref()
                    .map(|e| e.beats_per_measure)
                    .unwrap_or(4),
            )
        };
        if let Some(text) = text {
            self.inspection = Some(text);
            agg_gui::animation::request_draw();
        }
    }

    /// Unplugged mode: one self-graded pass through the exercise. Nailed It
    /// records a clean attempt per item and completes; Try Again records an
    /// error attempt per item and keeps the same exercise up (Anki-style).
    pub fn self_verify_grade(&mut self, nailed_it: bool) {
        if self.input_source != InputSource::SelfVerify
            || self.phase != Phase::Playing
            || self.is_free_play
            || self.exercise.is_none()
        {
            return;
        }
        let events = self.events.clone();
        for (index, event) in events.iter().enumerate().skip(self.start_event_index) {
            for (pos, &midi) in event.pitches.iter().enumerate() {
                self.record_attempt(midi, event.staves[pos], !nailed_it, None);
            }
            self.record_interval_attempt(index, !nailed_it, None);
            self.record_chord_shape_attempt(index, !nailed_it, None);
        }
        if nailed_it {
            {
                // Deliberate deviation: Swift marks every notehead
                // .correct here; the practice-from-here prefix stays
                // Locked (it was never part of this pass) so the score
                // keeps showing where the section started.
                let mut notation = self.notation.borrow_mut();
                for ids in self.event_ids.iter().skip(self.start_event_index) {
                    for id in ids {
                        notation.set_state(id, Some(NoteState::Correct));
                    }
                }
            }
            let first_try = self.self_verify_attempts == 0;
            self.first_try_correct = if first_try { self.note_count } else { 0 };
            self.streak = if first_try {
                self.streak + self.note_count as i64
            } else {
                0
            };
            self.errors_this_exercise = self.self_verify_attempts;
            self.finish_exercise();
        } else {
            self.self_verify_attempts += 1;
            self.errors_this_exercise = self.self_verify_attempts;
            self.streak = 0;
        }
        agg_gui::animation::request_draw();
    }
}
