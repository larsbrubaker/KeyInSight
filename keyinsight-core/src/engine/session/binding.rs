//! Render → event binding and note-state painting: align the engraved
//! score's note ids with the match events, mark the current event, and
//! restore the natural per-note states after a playback follow.

use crate::engine::session::SessionEngine;
use crate::notation::NoteState;
use crate::score::{Exercise, ScoreNote};

impl SessionEngine {
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

    /// The state each note should show based on actual play progress.
    pub(crate) fn natural_state(&self, index: usize) -> Option<NoteState> {
        if index < self.start_event_index {
            return Some(NoteState::Locked); // before the start spot
        }
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
                Some(TempoResolution::Skipped) => Some(NoteState::Locked),
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

    pub(crate) fn restore_note_states(&mut self) {
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
