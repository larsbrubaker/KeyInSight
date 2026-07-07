//! Adaptive generator: a weighted random walk over the active pitch set.
//! Weak items (higher weight) pull the walk toward them, while musicality
//! constraints keep the output phrase-like rather than random:
//!
//! - step-dominant motion (repeats and leaps up to a 4th allowed, rare)
//! - leap recovery: after a leap, only stepwise motion, preferring the
//!   opposite direction (classic contour rule)
//! - phrase arc: rising bias in the first half, falling in the second
//! - cadence: the final note prefers a nearby C or G (do/sol) and a longer
//!   value
//!
//! The rhythm vocabulary expands with `GeneratorConfig::rhythm_level`
//! (0: quarter/half/whole · 1: +dotted half · 2: +eighth pairs · 3: +rests).
//!
//! Ports `Score/ExerciseGenerator.swift`.

use std::collections::HashMap;

use crate::core::Rng64;
use crate::score::{Exercise, NoteDuration, ScoreNote, Staff};

#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    pub measures: i32,
    pub beats_per_measure: i32,
    pub rhythm_level: i32,
    /// Key signature for generated exercises (0 = C, 1 = G, 2 = D).
    pub fifths: i32,
    /// Skill-model bias per signed diatonic move (interval items);
    /// missing deltas count as neutral.
    pub interval_weights: HashMap<i32, f64>,
    /// Add a left-hand bass voice: one long tone per measure alternating
    /// tonic and dominant, ending on the tonic — the classic first
    /// hands-together texture (opt-in until bass items unlock).
    pub two_handed: bool,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            measures: 2,
            beats_per_measure: 4,
            rhythm_level: 0,
            fifths: 0,
            interval_weights: HashMap::new(),
            two_handed: false,
        }
    }
}

/// One active pitch with its skill-model weakness weight (1.0 = neutral).
#[derive(Debug, Clone, Copy)]
pub struct PitchOption {
    pub midi: u8,
    pub weight: f64,
}

impl PitchOption {
    pub fn new(midi: u8) -> Self {
        Self { midi, weight: 1.0 }
    }

    pub fn weighted(midi: u8, weight: f64) -> Self {
        Self { midi, weight }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExerciseGenerator {
    pub config: GeneratorConfig,
}

/// Base likelihood per move distance (in active-set positions).
/// Steps dominate; repeats are common; leaps are salt.
fn move_weight(distance: i32) -> Option<f64> {
    match distance {
        0 => Some(0.9),
        1 => Some(3.0),
        2 => Some(0.5),
        3 => Some(0.25),
        _ => None,
    }
}

impl ExerciseGenerator {
    /// Micro-drill flash card: one whole note, weight-sampled with extra
    /// emphasis on weak items.
    pub fn drill_note(pitches: &[PitchOption], rng: &mut impl Rng64) -> Exercise {
        assert!(!pitches.is_empty(), "drill needs a non-empty active pitch set");
        let weights: Vec<f64> = pitches.iter().map(|p| p.weight.powf(1.5)).collect();
        let index = sample(&weights, rng);
        Exercise::new(
            vec![ScoreNote::note(pitches[index].midi, NoteDuration::Whole)],
            4,
        )
    }

    pub fn generate(&self, pitches: &[PitchOption], rng: &mut impl Rng64) -> Exercise {
        assert!(
            !pitches.is_empty(),
            "generator needs a non-empty active pitch set"
        );
        let mut options: Vec<PitchOption> = pitches.to_vec();
        options.sort_by_key(|p| p.midi);

        let rhythm = self.make_rhythm(rng);
        let sounded_count = rhythm.iter().filter(|d| d.is_some()).count();

        let mut positions: Vec<usize> = Vec::new();
        let mut position = sample(
            &options.iter().map(|o| o.weight).collect::<Vec<_>>(),
            rng,
        );
        positions.push(position);
        let mut previous_delta: i32 = 0;

        for i in 1..sounded_count {
            let next = if i == sounded_count - 1 {
                match self.cadence_position(position, &options) {
                    Some(cadence) => cadence,
                    None => self.next_position(
                        position,
                        previous_delta,
                        i as f64 / sounded_count as f64,
                        &options,
                        rng,
                    ),
                }
            } else {
                self.next_position(
                    position,
                    previous_delta,
                    i as f64 / sounded_count as f64,
                    &options,
                    rng,
                )
            };
            previous_delta = next as i32 - position as i32;
            position = next;
            positions.push(position);
        }

        let mut pitch_iter = positions.into_iter();
        let notes: Vec<ScoreNote> = rhythm
            .iter()
            .map(|duration| match duration {
                Some(duration) => ScoreNote::note(
                    options[pitch_iter.next().expect("one pitch per sounded event")].midi,
                    *duration,
                ),
                None => ScoreNote::rest(NoteDuration::Quarter),
            })
            .collect();

        let mut exercise = Exercise::new(notes, self.config.beats_per_measure)
            .with_fifths(self.config.fifths);
        if self.config.two_handed {
            exercise = exercise.with_bass(self.bass_line());
        }
        exercise
    }

    /// Left hand for two-handed exercises: whole-measure long tones
    /// alternating tonic and dominant (below), ending on the tonic.
    fn bass_line(&self) -> Vec<ScoreNote> {
        // Tonic in the comfortable low-bass zone per key: C3, G3, D3.
        let tonic: u8 = [48, 55, 50][self.config.fifths.clamp(0, 2) as usize];
        let dominant = tonic - 5;
        let duration = if self.config.beats_per_measure == 3 {
            NoteDuration::DottedHalf
        } else {
            NoteDuration::Whole
        };
        (0..self.config.measures)
            .map(|measure| {
                let is_last = measure == self.config.measures - 1;
                let midi = if is_last || measure % 2 == 0 {
                    tonic
                } else {
                    dominant
                };
                ScoreNote::note(midi, duration).with_staff(Staff::Bass)
            })
            .collect()
    }

    // --- Rhythm ---

    /// One entry per score event: a duration for a note, or None for a
    /// quarter rest. Rests never open the exercise, never appear in the
    /// last measure, and cap at one per measure.
    fn make_rhythm(&self, rng: &mut impl Rng64) -> Vec<Option<NoteDuration>> {
        let units_per_measure = self.config.beats_per_measure * 2;
        let mut rhythm: Vec<Option<NoteDuration>> = Vec::new();
        for measure in 0..self.config.measures {
            let is_last = measure == self.config.measures - 1;
            let mut remaining = units_per_measure;
            let mut rest_used = false;
            while remaining > 0 {
                // Cadence: close the final measure's last 2 beats with a half.
                if is_last && remaining == 4 {
                    rhythm.push(Some(NoteDuration::Half));
                    remaining = 0;
                    continue;
                }
                // (tokens, weight); an empty token slot means a rest.
                let mut choices: Vec<(Vec<Option<NoteDuration>>, i32)> = Vec::new();
                if remaining >= 2 {
                    choices.push((vec![Some(NoteDuration::Quarter)], 6));
                }
                if remaining >= 4 {
                    choices.push((vec![Some(NoteDuration::Half)], 3));
                }
                if remaining >= 8 {
                    choices.push((vec![Some(NoteDuration::Whole)], 1));
                }
                if self.config.rhythm_level >= 1 && remaining >= 6 {
                    choices.push((vec![Some(NoteDuration::DottedHalf)], 2));
                }
                if self.config.rhythm_level >= 2 && remaining >= 2 {
                    choices.push((
                        vec![Some(NoteDuration::Eighth), Some(NoteDuration::Eighth)],
                        3,
                    ));
                }
                if self.config.rhythm_level >= 3
                    && remaining >= 2
                    && !rest_used
                    && !is_last
                    && !rhythm.is_empty()
                {
                    choices.push((vec![None], 1));
                }
                let total: i32 = choices.iter().map(|c| c.1).sum();
                let mut roll = rng.next_below(total as usize) as i32;
                for (tokens, weight) in &choices {
                    roll -= weight;
                    if roll < 0 {
                        if tokens.as_slice() == [None] {
                            rest_used = true;
                        }
                        for token in tokens {
                            rhythm.push(*token);
                            remaining -= token.map(|d| d.units()).unwrap_or(2);
                        }
                        break;
                    }
                }
            }
        }
        rhythm
    }

    // --- Pitch walk ---

    fn next_position(
        &self,
        position: usize,
        previous_delta: i32,
        phrase_progress: f64,
        options: &[PitchOption],
        rng: &mut impl Rng64,
    ) -> usize {
        let after_leap = previous_delta.abs() >= 2;
        let preferred_direction: i32 = if phrase_progress < 0.5 { 1 } else { -1 };

        let mut candidates: Vec<usize> = Vec::new();
        let mut weights: Vec<f64> = Vec::new();
        for delta in -3i32..=3 {
            let candidate = position as i32 + delta;
            if candidate < 0 || candidate >= options.len() as i32 {
                continue;
            }
            let Some(base) = move_weight(delta.abs()) else {
                continue;
            };
            if after_leap && delta.abs() > 1 {
                continue;
            }

            let candidate = candidate as usize;
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
            candidates.push(candidate);
            weights.push(weight);
        }
        candidates[sample(&weights, rng)]
    }

    /// Nearby do/sol ending, if one is reachable without a big closing leap.
    fn cadence_position(&self, position: usize, options: &[PitchOption]) -> Option<usize> {
        let homes: Vec<usize> = (0..options.len())
            .filter(|&i| {
                let pc = options[i].midi % 12;
                (pc == 0 || pc == 7) && (i as i32 - position as i32).abs() <= 3
            })
            .collect();
        homes
            .into_iter()
            .min_by_key(|&i| (i as i32 - position as i32).abs())
    }
}

fn sample(weights: &[f64], rng: &mut impl Rng64) -> usize {
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
