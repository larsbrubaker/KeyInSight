//! The progress report: per-item, per-interval, chord-shape and trouble
//! transition entries, and the item heat-map staff rendering (one staff
//! per hand).

use crate::core::PitchSpelling;
use crate::engine::session::SessionEngine;
use crate::notation::{NotationController, NoteState};
use crate::score::{Exercise, MusicXmlEncoder, NoteDuration, ScoreNote, Staff};
use crate::skill::{ItemState, SkillModel, Thresholds, CHORD_SHAPE_LADDER};

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

/// One rung of the chord-shape ladder with its status:
/// "unlocked" | "probing" | "locked".
#[derive(Debug, Clone)]
pub struct ChordEntry {
    pub name: String,
    pub label: String,
    pub status: String,
    pub attempts: i64,
    pub error_percent: Option<i64>,
}

/// A specific trouble transition ("F#4 → B4").
#[derive(Debug, Clone)]
pub struct TransitionEntry {
    pub label: String,
    pub attempts: i64,
    pub error_percent: i64,
}

impl SessionEngine {
    /// Item states in staff (ascending pitch) order, stats freshly loaded.
    pub fn progress_entries(&mut self, staff: Staff) -> Vec<ProgressEntry> {
        self.refresh_skill();
        let model = if staff == Staff::Bass {
            &self.bass_skill
        } else {
            &self.skill
        };
        let mut states: Vec<ItemState> = model.states.clone();
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

    /// The chord-shape ladder with per-shape status (unlocked / probing /
    /// locked) and stats.
    pub fn chord_shape_entries(&self) -> Vec<ChordEntry> {
        let unlocked = self.skill.unlocked_chord_shapes();
        let probing = self.skill.next_locked_chord_shape();
        CHORD_SHAPE_LADDER
            .iter()
            .map(|&name| {
                let stat = self.skill.chord_shape_stat(name);
                let status = if unlocked.contains(&name) {
                    "unlocked"
                } else if Some(name) == probing {
                    "probing"
                } else {
                    "locked"
                };
                ChordEntry {
                    name: name.to_string(),
                    label: SkillModel::chord_shape_display_name(name),
                    status: status.to_string(),
                    attempts: stat.map(|s| s.attempts).unwrap_or(0),
                    error_percent: stat.map(|s| (s.ewma_error * 100.0).round() as i64),
                }
            })
            .collect()
    }

    /// Worst specific transitions (e.g. "F#4 → B4") with enough data to
    /// mean something — the trouble spots interval shapes wash out.
    pub fn trouble_transitions(&self, limit: usize) -> Vec<TransitionEntry> {
        let mut stats = self
            .db
            .as_ref()
            .map(|db| db.item_stats())
            .unwrap_or_default();
        stats.retain(|s| s.item.starts_with("move:") && s.attempts >= 4 && s.ewma_error > 0.15);
        stats.sort_by(|a, b| {
            b.ewma_error
                .partial_cmp(&a.ewma_error)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        stats
            .iter()
            .take(limit)
            .map(|stat| TransitionEntry {
                label: stat.item["move:".len()..].replace('>', " → "),
                attempts: stat.attempts,
                error_percent: (stat.ewma_error * 100.0).round() as i64,
            })
            .collect()
    }

    /// Render the item heat map (every item as a quarter note, ascending)
    /// into the given controller — one staff per hand.
    pub fn render_progress_staff(&mut self, controller: &mut NotationController, staff: Staff) {
        let entries = self.progress_entries(staff);
        let notes: Vec<ScoreNote> = entries
            .iter()
            .map(|e| ScoreNote::note(e.midi, NoteDuration::Quarter).with_staff(staff))
            .collect();
        let staff_exercise = if staff == Staff::Bass {
            Exercise::new(Vec::new(), 4).with_bass(notes)
        } else {
            Exercise::new(notes, 4)
        };
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
