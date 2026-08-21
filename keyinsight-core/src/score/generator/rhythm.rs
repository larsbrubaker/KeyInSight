//! The rhythm vocabulary: one entry per score event, gated by
//! `GeneratorConfig::rhythm_level`.

use crate::core::Rng64;
use crate::score::NoteDuration;

use super::ExerciseGenerator;

impl ExerciseGenerator {
    /// One entry per score event: a duration for a note, or None for a
    /// quarter rest. Rests never open the exercise, never appear in the
    /// last measure, and cap at one per measure.
    pub(super) fn make_rhythm(&self, rng: &mut impl Rng64) -> Vec<Option<NoteDuration>> {
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
}
