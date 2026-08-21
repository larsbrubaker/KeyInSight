//! Growing walk notes into chords (shape-classes drawn from the active
//! set) and the accompanying hand's long tones for `Both` textures.

use crate::core::Rng64;
use crate::score::{NoteDuration, ScoreNote, Staff};

use super::walk::sample;
use super::{ChordShape, ExerciseGenerator, PitchOption};

impl ExerciseGenerator {
    /// Notes from the walk, some grown into chords (shape-classes): members
    /// sit diatonic steps below the melody note, drawn from the same active
    /// set, emitted bottom-up (anchor lowest). Chords land only on
    /// quarter-or-longer values — dyads on eighths read as clutter.
    pub(super) fn build_walk(
        &self,
        rhythm: &[Option<NoteDuration>],
        positions: &[usize],
        options: &[PitchOption],
        staff: Staff,
        rng: &mut impl Rng64,
    ) -> Vec<ScoreNote> {
        let sounded_count = positions.len();
        let chord_probe_index: Option<usize> = if self.config.probe_chord_shape.is_some()
            && sounded_count >= 2
            && rng.next_f64_below(1.0) < Self::PROBE_CHANCE
        {
            Some(rng.next_below(sounded_count))
        } else {
            None
        };

        let mut walk: Vec<ScoreNote> = Vec::new();
        let mut sounded_index = 0usize;
        let mut chords_placed = 0usize;
        for duration in rhythm {
            let Some(duration) = *duration else {
                walk.push(ScoreNote::rest(NoteDuration::Quarter).with_staff(staff));
                continue;
            };
            let position = positions[sounded_index];
            let shape = self.chord_shape(
                sounded_index,
                position,
                duration,
                chord_probe_index,
                chords_placed,
                rng,
            );
            if let Some(shape) = shape {
                chords_placed += 1;
                // Bottom-up: deepest member anchors, the melody note rides
                // on top as a <chord/> member.
                let mut steps: Vec<i32> = shape.member_steps_below.to_vec();
                steps.sort_unstable_by(|a, b| b.cmp(a));
                steps.push(0);
                for (offset, step) in steps.into_iter().enumerate() {
                    walk.push(
                        ScoreNote::note(options[position - step as usize].midi, duration)
                            .with_staff(staff)
                            .with_chord(offset > 0),
                    );
                }
            } else {
                walk.push(ScoreNote::note(options[position].midi, duration).with_staff(staff));
            }
            sounded_index += 1;
        }
        walk
    }

    /// The shape for this note, if any: the probe shape at its chosen spot,
    /// else an unlocked shape by weakness weight — always bounded by the
    /// active set (an octave dyad needs eight positions of room, which
    /// naturally gates wide shapes to wide ranges).
    fn chord_shape(
        &self,
        index: usize,
        position: usize,
        duration: NoteDuration,
        probe_index: Option<usize>,
        chords_placed: usize,
        rng: &mut impl Rng64,
    ) -> Option<ChordShape> {
        if duration.units() < 2 || chords_placed >= Self::MAX_CHORDS_PER_EXERCISE {
            return None;
        }
        let fits = |shape: &ChordShape| position as i32 - shape.depth() >= 0;
        if Some(index) == probe_index {
            if let Some(probe) = self.config.probe_chord_shape {
                if fits(&probe) {
                    return Some(probe);
                }
            }
        }
        if self.config.chord_shapes.is_empty() || rng.next_f64_below(1.0) >= Self::CHORD_CHANCE {
            return None;
        }
        let eligible: Vec<&(ChordShape, f64)> = self
            .config
            .chord_shapes
            .iter()
            .filter(|(shape, _)| fits(shape))
            .collect();
        if eligible.is_empty() {
            return None;
        }
        let weights: Vec<f64> = eligible.iter().map(|(_, weight)| *weight).collect();
        Some(eligible[sample(&weights, rng)].0)
    }

    /// The accompanying hand's long tone for a measure of a `Both` texture:
    /// I/V alternation ending on the tonic. Melody in treble puts the tones
    /// low (C3/G3/D3 tonics, dominant a 4th below); melody in bass flips
    /// them up to the treble staff (C4/G4/D4, dominant a 5th above) so the
    /// hands never cross.
    pub(super) fn accompaniment_pitch(&self, measure: i32) -> u8 {
        let key = self.config.fifths.clamp(0, 2) as usize;
        let tonic: u8 = if self.config.melody_in_bass {
            [60, 67, 62][key]
        } else {
            [48, 55, 50][key]
        };
        let dominant = (tonic as i32 + if self.config.melody_in_bass { 7 } else { -5 }) as u8;
        let is_last = measure == self.config.measures - 1;
        if is_last || measure % 2 == 0 {
            tonic
        } else {
            dominant
        }
    }

    pub(super) fn accompaniment_line(&self) -> Vec<ScoreNote> {
        let duration = if self.config.beats_per_measure == 3 {
            NoteDuration::DottedHalf
        } else {
            NoteDuration::Whole
        };
        let staff = if self.config.melody_in_bass {
            Staff::Treble
        } else {
            Staff::Bass
        };
        (0..self.config.measures)
            .map(|measure| {
                ScoreNote::note(self.accompaniment_pitch(measure), duration).with_staff(staff)
            })
            .collect()
    }
}
