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
//! Ports `Score/ExerciseGenerator.swift`. Split by concern: the pitch walk
//! (`walk`), chord shapes and the accompanying hand (`chords`), and the
//! rhythm vocabulary (`rhythm`). The RNG draw order follows the Swift
//! source exactly so a seed reproduces the same exercise.

mod chords;
mod rhythm;
mod walk;

use std::collections::{HashMap, HashSet};

use crate::core::Rng64;
use crate::score::{Exercise, NoteDuration, ScoreNote, Staff};

use walk::sample;

/// Which hand(s) an exercise trains: the melodic walk goes to the named
/// hand's staff; `Both` is melody plus LH long tones — the classic first
/// hands-together texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hands {
    Right,
    Left,
    Both,
}

impl Hands {
    /// Swift raw value (the persisted setting string).
    pub fn raw_value(self) -> &'static str {
        match self {
            Hands::Right => "right",
            Hands::Left => "left",
            Hands::Both => "both",
        }
    }

    pub fn from_raw_value(raw: &str) -> Option<Hands> {
        match raw {
            "right" => Some(Hands::Right),
            "left" => Some(Hands::Left),
            "both" => Some(Hands::Both),
            _ => None,
        }
    }
}

/// A harmonic shape built below a melody note (shape-classes): each member
/// sits the given diatonic steps under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChordShape {
    pub item: &'static str,
    pub member_steps_below: &'static [i32],
}

impl ChordShape {
    const BY_SKILL_ITEM: [ChordShape; 5] = [
        ChordShape {
            item: "chord:harm-3rd",
            member_steps_below: &[2],
        },
        ChordShape {
            item: "chord:harm-5th",
            member_steps_below: &[4],
        },
        ChordShape {
            item: "chord:harm-6th",
            member_steps_below: &[5],
        },
        ChordShape {
            item: "chord:harm-octave",
            member_steps_below: &[7],
        },
        ChordShape {
            item: "chord:triad",
            member_steps_below: &[2, 4],
        },
    ];

    /// The shape for a skill item name (`"chord:harm-5th"` …).
    pub fn by_skill_item(item: &str) -> Option<ChordShape> {
        Self::BY_SKILL_ITEM
            .iter()
            .copied()
            .find(|shape| shape.item == item)
    }

    /// Room the shape needs below the melody position in the active set.
    fn depth(&self) -> i32 {
        self.member_steps_below.iter().copied().max().unwrap_or(0)
    }
}

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
    /// Pair-level bias on top of the shape prior, keyed by
    /// `SkillModel::transition_key` (backoff).
    pub transition_weights: HashMap<i32, f64>,
    /// Melodic move sizes the walk may use (interval ladder).
    pub allowed_steps: HashSet<i32>,
    /// Next locked size: one sparse forced move per exercise, sometimes
    /// (readiness probe).
    pub probe_step: Option<i32>,
    /// Unlocked chord shapes with their weakness weights.
    pub chord_shapes: Vec<(ChordShape, f64)>,
    /// Next locked shape, probe-injected like intervals.
    pub probe_chord_shape: Option<ChordShape>,
    pub hands: Hands,
    /// `Both` texture flip: the melodic walk goes to the LEFT hand and the
    /// right holds the long tones (survival's Auto varies per-hand action
    /// this way; the walk pitches must then come from the bass model's
    /// active set).
    pub melody_in_bass: bool,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            measures: 2,
            beats_per_measure: 4,
            rhythm_level: 0,
            fifths: 0,
            interval_weights: HashMap::new(),
            transition_weights: HashMap::new(),
            allowed_steps: [0, 1, 2, 3].into_iter().collect(),
            probe_step: None,
            chord_shapes: Vec::new(),
            probe_chord_shape: None,
            hands: Hands::Right,
            melody_in_bass: false,
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

impl ExerciseGenerator {
    /// Chance an exercise carries its interval / chord probe.
    pub const PROBE_CHANCE: f64 = 0.4;
    /// Chance per eligible note of becoming a chord (unlocked shapes).
    pub const CHORD_CHANCE: f64 = 0.12;
    pub const MAX_CHORDS_PER_EXERCISE: usize = 3;

    /// Micro-drill flash card: one whole note, weight-sampled with extra
    /// emphasis on weak items. Cards render on the FULL grand staff (the
    /// other staff holds a whole rest) so which-clef is part of the read,
    /// not a separate puzzle. Never repeats `excluding` (the previous
    /// card) — an identical card is invisible.
    pub fn drill_note(
        pitches: &[PitchOption],
        staff: Staff,
        excluding: Option<u8>,
        rng: &mut impl Rng64,
    ) -> Exercise {
        assert!(
            !pitches.is_empty(),
            "drill needs a non-empty active pitch set"
        );
        let mut candidates: Vec<PitchOption> = pitches
            .iter()
            .copied()
            .filter(|p| Some(p.midi) != excluding)
            .collect();
        if candidates.is_empty() {
            candidates = pitches.to_vec();
        }
        let weights: Vec<f64> = candidates.iter().map(|p| p.weight.powf(1.5)).collect();
        let index = sample(&weights, rng);
        let note = ScoreNote::note(candidates[index].midi, NoteDuration::Whole).with_staff(staff);
        if staff == Staff::Bass {
            Exercise::new(vec![ScoreNote::rest(NoteDuration::Whole)], 4).with_bass(vec![note])
        } else {
            Exercise::new(vec![note], 4).with_bass(vec![
                ScoreNote::rest(NoteDuration::Whole).with_staff(Staff::Bass)
            ])
        }
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

        // Readiness probe: sometimes force ONE mid-phrase move of the next
        // locked size, so its stats accrue before it unlocks.
        let probe_index: Option<usize> = if self.config.probe_step.is_some()
            && sounded_count >= 4
            && rng.next_f64_below(1.0) < Self::PROBE_CHANCE
        {
            Some(1 + rng.next_below(sounded_count - 2))
        } else {
            None
        };

        // Where a melody onset coincides with an accompaniment long tone
        // (measure starts of `Both` textures), the walk must not double the
        // long tone's LETTER: the same key twice isn't real notation, and
        // even an octave double ("C over C", struck together) reads as a
        // bug at the beginner stage this trainer targets.
        let forbidden = self.measure_start_forbidden(&rhythm);
        let same_letter =
            |forbidden: Option<&u8>, midi: u8| forbidden.map(|f| f % 12 == midi % 12) == Some(true);

        let mut positions: Vec<usize> = Vec::new();
        let first_weights: Vec<f64> = options
            .iter()
            .map(|option| {
                if same_letter(forbidden.get(&0), option.midi) {
                    0.0
                } else {
                    option.weight
                }
            })
            .collect();
        let mut position = sample(&first_weights, rng);
        positions.push(position);
        let mut previous_delta: i32 = 0;

        for i in 1..sounded_count {
            let avoiding = forbidden.get(&i).copied();
            let mut next: Option<usize> = None;
            if i == sounded_count - 1 {
                next = self.cadence_position(position, &options, avoiding);
            }
            if next.is_none() && Some(i) == probe_index && previous_delta.abs() < 2 {
                let step = self
                    .config
                    .probe_step
                    .expect("probe index implies a probe step");
                let probes: Vec<i32> = [position as i32 + step, position as i32 - step]
                    .into_iter()
                    .filter(|&candidate| {
                        candidate >= 0
                            && candidate < options.len() as i32
                            && !same_letter(avoiding.as_ref(), options[candidate as usize].midi)
                    })
                    .collect();
                if !probes.is_empty() {
                    next = Some(probes[rng.next_below(probes.len())] as usize);
                }
            }
            let next = next.unwrap_or_else(|| {
                self.next_position(
                    position,
                    previous_delta,
                    i as f64 / sounded_count as f64,
                    &options,
                    avoiding,
                    rng,
                )
            });
            previous_delta = next as i32 - position as i32;
            position = next;
            positions.push(position);
        }

        let walk_in_bass = self.config.hands == Hands::Left
            || (self.config.hands == Hands::Both && self.config.melody_in_bass);
        let walk = self.build_walk(
            &rhythm,
            &positions,
            &options,
            if walk_in_bass {
                Staff::Bass
            } else {
                Staff::Treble
            },
            rng,
        );
        let bpm = self.config.beats_per_measure;
        let fifths = self.config.fifths;
        match self.config.hands {
            Hands::Right => Exercise::new(walk, bpm).with_fifths(fifths),
            Hands::Left => Exercise::new(vec![], bpm)
                .with_bass(walk)
                .with_fifths(fifths),
            Hands::Both if self.config.melody_in_bass => {
                Exercise::new(self.accompaniment_line(), bpm)
                    .with_bass(walk)
                    .with_fifths(fifths)
            }
            Hands::Both => Exercise::new(walk, bpm)
                .with_bass(self.accompaniment_line())
                .with_fifths(fifths),
        }
    }
}
