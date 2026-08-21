//! The progress report: per-item and per-interval entries, and the item
//! heat-map staff rendering.

use crate::core::PitchSpelling;
use crate::engine::session::SessionEngine;
use crate::notation::{NotationController, NoteState};
use crate::score::{Exercise, MusicXmlEncoder, NoteDuration, ScoreNote};
use crate::skill::{ItemState, SkillModel, Thresholds};

#[derive(Debug, Clone)]
pub struct ProgressEntry {
    pub midi: u8,
    pub name: String,
    pub unlocked: bool,
    pub mastered: bool,
    pub attempts: i64,
    pub error_percent: Option<i64>,
    pub latency_ms: Option<f64>,
    pub heat: NoteState,
}

#[derive(Debug, Clone)]
pub struct IntervalEntry {
    pub delta: i32,
    pub label: String,
    pub attempts: i64,
    pub error_percent: Option<i64>,
    pub latency_ms: Option<f64>,
}

impl SessionEngine {
    /// Item states in staff (ascending pitch) order, stats freshly loaded.
    pub fn progress_entries(&mut self) -> Vec<ProgressEntry> {
        self.refresh_skill();
        let mut states: Vec<ItemState> = self.skill.states.clone();
        states.sort_by_key(|s| s.midi);
        states
            .into_iter()
            .map(|state| ProgressEntry {
                midi: state.midi,
                name: PitchSpelling::name(state.midi),
                unlocked: state.unlocked,
                mastered: state.mastered,
                attempts: state.stat.as_ref().map(|s| s.attempts).unwrap_or(0),
                error_percent: state
                    .stat
                    .as_ref()
                    .map(|s| (s.ewma_error * 100.0).round() as i64),
                latency_ms: state.stat.as_ref().and_then(|s| s.ewma_latency_ms),
                heat: heat_for(&state),
            })
            .collect()
    }

    pub fn interval_entries(&self) -> Vec<IntervalEntry> {
        self.skill
            .interval_states
            .iter()
            .map(|state| {
                let size = SkillModel::interval_size_display_name(state.delta.abs());
                let arrow = if state.delta == 0 {
                    ""
                } else if state.delta > 0 {
                    " ↑"
                } else {
                    " ↓"
                };
                IntervalEntry {
                    delta: state.delta,
                    label: format!("{size}{arrow}"),
                    attempts: state.stat.as_ref().map(|s| s.attempts).unwrap_or(0),
                    error_percent: state
                        .stat
                        .as_ref()
                        .map(|s| (s.ewma_error * 100.0).round() as i64),
                    latency_ms: state.stat.as_ref().and_then(|s| s.ewma_latency_ms),
                }
            })
            .collect()
    }

    /// Render the item heat map (every item as a quarter note, ascending)
    /// into the given controller.
    pub fn render_progress_staff(&mut self, controller: &mut NotationController) {
        let entries = self.progress_entries();
        let staff_exercise = Exercise::new(
            entries
                .iter()
                .map(|e| ScoreNote::note(e.midi, NoteDuration::Quarter))
                .collect(),
            4,
        );
        let rendered = controller.render(&MusicXmlEncoder::encode(&staff_exercise));
        let Some(rendered) = rendered else { return };
        if rendered.note_ids.len() != entries.len() {
            return;
        }
        controller.load_score();
        for (id, entry) in rendered.note_ids.iter().zip(&entries) {
            controller.set_state(id, Some(entry.heat));
        }
    }
}

fn heat_for(state: &ItemState) -> NoteState {
    if !state.unlocked {
        return NoteState::Locked;
    }
    if state.mastered {
        return NoteState::Mastered;
    }
    if let Some(stat) = &state.stat {
        if stat.attempts >= Thresholds::MIN_ATTEMPTS && stat.ewma_error > 0.35 {
            return NoteState::Weak;
        }
    }
    NoteState::Learning
}
