//! The pitch walk: move-distance priors, leap recovery, phrase arc, the
//! interval ladder gate, pair-level backoff, cadence homes, and the
//! cross-staff unison rule for `Both` textures.

use std::collections::HashMap;

use crate::core::{transition_key, Rng64};
use crate::score::NoteDuration;

use super::{ExerciseGenerator, Hands, PitchOption};

/// Base likelihood per move distance (in active-set positions).
/// Steps dominate; repeats are common; leaps are salt — the wide sizes
/// (5th+) only participate once the interval ladder unlocks them.
fn move_weight(distance: i32) -> Option<f64> {
    match distance {
        0 => Some(0.9),
        1 => Some(3.0),
        2 => Some(0.5),
        3 => Some(0.25),
        4 => Some(0.15),
        5 => Some(0.08),
        6 => Some(0.05),
        7 => Some(0.1),
        _ => None,
    }
}

impl ExerciseGenerator {
    pub(super) fn next_position(
        &self,
        position: usize,
        previous_delta: i32,
        phrase_progress: f64,
        options: &[PitchOption],
        forbidden_midi: Option<u8>,
        rng: &mut impl Rng64,
    ) -> usize {
        let after_leap = previous_delta.abs() >= 2;
        let preferred_direction: i32 = if phrase_progress < 0.5 { 1 } else { -1 };

        let mut candidates: Vec<usize> = Vec::new();
        let mut weights: Vec<f64> = Vec::new();
        for delta in -7i32..=7 {
            let candidate = position as i32 + delta;
            if candidate < 0 || candidate >= options.len() as i32 {
                continue;
            }
            let candidate = candidate as usize;
            if let Some(forbidden) = forbidden_midi {
                if options[candidate].midi % 12 == forbidden % 12 {
                    continue;
                }
            }
            if delta != 0 && !self.config.allowed_steps.contains(&delta.abs()) {
                continue;
            }
            let Some(base) = move_weight(delta.abs()) else {
                continue;
            };
            if after_leap && delta.abs() > 1 {
                continue;
            }

            let mut weight = base * options[candidate].weight;
            if after_leap && delta != 0 && (delta > 0) != (previous_delta > 0) {
                weight *= 1.6; // recover a leap by stepping back
            }
            if delta != 0 && (delta > 0) == (preferred_direction > 0) {
                weight *= 1.35; // phrase arc: up, then down
            }
            // Interval-item bias: weak shapes ("down a 3rd") get drilled.
            // Note: delta is in active-set positions, which for a diatonic
            // set is exactly the signed diatonic interval.
            if let Some(interval_weight) = self.config.interval_weights.get(&delta) {
                weight *= interval_weight;
            }
            // Pair-level backoff: a specific weak transition ("F#4 to B4")
            // corrects the shape prior once it has data.
            if let Some(pair_weight) = self.config.transition_weights.get(&transition_key(
                options[position].midi,
                options[candidate].midi,
            )) {
                weight *= pair_weight;
            }
            candidates.push(candidate);
            weights.push(weight);
        }
        if candidates.is_empty() {
            // Cornered (tiny set, every move forbidden): accept the unison
            // rather than crash — the constraint is best-effort.
            return self.next_position(
                position,
                previous_delta,
                phrase_progress,
                options,
                None,
                rng,
            );
        }
        candidates[sample(&weights, rng)]
    }

    /// Nearby do/sol ending, if one is reachable without a big closing leap.
    pub(super) fn cadence_position(
        &self,
        position: usize,
        options: &[PitchOption],
        forbidden_midi: Option<u8>,
    ) -> Option<usize> {
        let homes: Vec<usize> = (0..options.len())
            .filter(|&i| {
                let pc = options[i].midi % 12;
                (pc == 0 || pc == 7)
                    && (i as i32 - position as i32).abs() <= 3
                    && forbidden_midi.map(|f| f % 12 != pc) != Some(false)
            })
            .collect();
        homes
            .into_iter()
            .min_by_key(|&i| (i as i32 - position as i32).abs())
    }

    /// Sounded-note index → the accompaniment pitch sounding at that exact
    /// onset (measure starts only; mid-measure melody over a held tone is
    /// ordinary music and stays allowed).
    pub(super) fn measure_start_forbidden(
        &self,
        rhythm: &[Option<NoteDuration>],
    ) -> HashMap<usize, u8> {
        let mut result: HashMap<usize, u8> = HashMap::new();
        if self.config.hands != Hands::Both {
            return result;
        }
        let units_per_measure = self.config.beats_per_measure * 2;
        let mut units = 0;
        let mut sounded_index = 0usize;
        for duration in rhythm {
            let Some(duration) = duration else {
                units += 2; // quarter rest
                continue;
            };
            if units % units_per_measure == 0 {
                result.insert(
                    sounded_index,
                    self.accompaniment_pitch(units / units_per_measure),
                );
            }
            units += duration.units();
            sounded_index += 1;
        }
        result
    }
}

pub(super) fn sample(weights: &[f64], rng: &mut impl Rng64) -> usize {
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return 0;
    }
    let mut roll = rng.next_f64_below(total);
    for (i, weight) in weights.iter().enumerate() {
        roll -= weight;
        if roll < 0.0 {
            return i;
        }
    }
    weights.len() - 1
}
