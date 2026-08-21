//! Ports `LadderGenerationTests` from
//! `Tests/KeyInSightTests/GeneratorTests.swift`: interval ladder + chord
//! shapes in generation.

use std::collections::HashSet;

use crate::core::{PitchSpelling, SplitMix64};
use crate::score::{ChordShape, Exercise, ExerciseGenerator, PitchOption};
use crate::skill::SkillModel;

fn wide_set() -> Vec<PitchOption> {
    [55, 57, 59, 60, 62, 64, 65, 67, 69, 71, 72]
        .iter()
        .map(|&m| PitchOption::new(m))
        .collect()
}

/// Successive diatonic differences between onset anchors (chord members
/// share the onset).
fn diatonic_deltas(exercise: &Exercise) -> Vec<i32> {
    let anchors: Vec<i32> = exercise
        .match_events()
        .iter()
        .map(|e| PitchSpelling::diatonic_index(*e.pitches.iter().max().unwrap()))
        .collect();
    anchors.windows(2).map(|w| w[1] - w[0]).collect()
}

#[test]
fn walk_never_exceeds_unlocked_sizes_without_a_probe() {
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 3;
    generator.config.allowed_steps = [0, 1, 2, 3].into_iter().collect();
    generator.config.probe_step = None;
    for seed in 1..=40u64 {
        let mut rng = SplitMix64::new(seed);
        let ex = generator.generate(&wide_set(), &mut rng);
        for delta in diatonic_deltas(&ex) {
            assert!(delta.abs() <= 3, "seed {seed}: leap of {delta}");
        }
    }
}

#[test]
fn probe_step_injects_the_next_size() {
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 3;
    generator.config.allowed_steps = [0, 1, 2, 3].into_iter().collect();
    generator.config.probe_step = Some(4);
    // Across many seeds, some exercises must carry a 5th — and nothing
    // may exceed the probe size.
    let mut probed = 0;
    for seed in 1..=60u64 {
        let mut rng = SplitMix64::new(seed);
        let ex = generator.generate(&wide_set(), &mut rng);
        let deltas = diatonic_deltas(&ex);
        for delta in &deltas {
            assert!(delta.abs() <= 4, "seed {seed}");
        }
        if deltas.iter().any(|d| d.abs() == 4) {
            probed += 1;
        }
    }
    assert!(probed > 5, "probes never landed ({probed})");
    assert!(probed < 55, "probes should be sparse ({probed})");
}

#[test]
fn unlocked_chord_shapes_produce_dyads_from_the_active_set() {
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 3;
    generator.config.chord_shapes =
        vec![(ChordShape::by_skill_item("chord:harm-5th").unwrap(), 1.0)];
    let allowed: HashSet<u8> = wide_set().iter().map(|p| p.midi).collect();
    let mut dyads = 0;
    for seed in 1..=60u64 {
        let mut rng = SplitMix64::new(seed);
        let ex = generator.generate(&wide_set(), &mut rng);
        for event in ex.match_events().iter().filter(|e| e.pitches.len() > 1) {
            dyads += 1;
            assert_eq!(
                SkillModel::chord_shape_name(&event.pitches),
                Some("chord:harm-5th")
            );
            assert!(event.pitches.iter().all(|p| allowed.contains(p)));
        }
        // Chords never land on eighths.
        for note in ex.sounded_notes().iter().filter(|n| n.chord_with_previous) {
            assert!(note.duration.units() >= 2);
        }
        // Measures stay exactly full.
        for measure in ex.measures() {
            assert_eq!(
                measure
                    .iter()
                    .filter(|n| !n.chord_with_previous)
                    .map(|n| n.duration.units())
                    .sum::<i32>(),
                ex.units_per_measure()
            );
        }
    }
    assert!(dyads > 3, "no dyads generated");
}

#[test]
fn probe_chord_shape_injects_triads() {
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 3;
    generator.config.probe_chord_shape = ChordShape::by_skill_item("chord:triad");
    let mut triads = 0;
    for seed in 1..=60u64 {
        let mut rng = SplitMix64::new(seed);
        let ex = generator.generate(&wide_set(), &mut rng);
        for event in ex.match_events().iter().filter(|e| e.pitches.len() == 3) {
            assert_eq!(
                SkillModel::chord_shape_name(&event.pitches),
                Some("chord:triad")
            );
            triads += 1;
        }
    }
    assert!(triads > 3, "no probe triads generated");
}

#[test]
fn transition_weights_steer_the_walk() {
    // A heavily weighted pair should appear more often than baseline.
    let mut biased = ExerciseGenerator::default();
    biased.config.measures = 4;
    biased.config.transition_weights = [(SkillModel::transition_key(60, 64), 8.0)]
        .into_iter()
        .collect();
    let mut neutral = biased.clone();
    neutral.config.transition_weights.clear();
    let count = |generator: &ExerciseGenerator| -> usize {
        let mut hits = 0;
        for seed in 1..=80u64 {
            let mut rng = SplitMix64::new(seed);
            let pitches: Vec<PitchOption> = [60, 62, 64, 65, 67]
                .iter()
                .map(|&m| PitchOption::new(m))
                .collect();
            let ex = generator.generate(&pitches, &mut rng);
            let midis: Vec<u8> = ex.match_events().iter().map(|e| e.pitches[0]).collect();
            hits += midis
                .windows(2)
                .filter(|w| w[0] == 60 && w[1] == 64)
                .count();
        }
        hits
    };
    assert!(count(&biased) > count(&neutral));
}
