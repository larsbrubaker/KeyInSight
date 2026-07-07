//! Ports `Tests/KeyInSightTests/GeneratorTests.swift`.

use crate::core::SplitMix64;
use crate::score::{Exercise, ExerciseGenerator, NoteDuration, PitchOption};

const SEED_PITCHES: [u8; 5] = [60, 62, 64, 65, 67];

fn options() -> Vec<PitchOption> {
    SEED_PITCHES.iter().map(|&m| PitchOption::new(m)).collect()
}

fn generate_with(seed: u64, measures: i32, rhythm_level: i32, pitches: &[PitchOption]) -> Exercise {
    let mut rng = SplitMix64::new(seed);
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = measures;
    generator.config.rhythm_level = rhythm_level;
    generator.generate(pitches, &mut rng)
}

fn generate(seed: u64) -> Exercise {
    generate_with(seed, 2, 0, &options())
}

#[test]
fn measures_are_exactly_full() {
    for level in 0..=3 {
        for seed in 1..=50u64 {
            let exercise = generate_with(seed, 3, level, &options());
            let measures = exercise.measures();
            assert_eq!(measures.len(), 3, "seed {seed} level {level}");
            for measure in &measures {
                assert_eq!(
                    measure.iter().map(|n| n.duration.units()).sum::<i32>(),
                    exercise.units_per_measure(),
                    "seed {seed} level {level}"
                );
            }
        }
    }
}

#[test]
fn pitches_stay_in_active_set() {
    for seed in 1..=50u64 {
        for note in generate(seed).sounded_notes() {
            assert!(
                SEED_PITCHES.contains(&note.midi.unwrap()),
                "seed {seed}: {}",
                note.midi.unwrap()
            );
        }
    }
}

#[test]
fn rhythm_vocabulary_gated_by_level() {
    let mut saw_dotted = false;
    let mut saw_eighth = false;
    let mut saw_rest = false;
    for seed in 1..=80u64 {
        // Level 0: quarters/halves/wholes only, no rests.
        for note in &generate_with(seed, 2, 0, &options()).notes {
            assert!(!note.is_rest(), "seed {seed}");
            assert!(
                [
                    NoteDuration::Quarter,
                    NoteDuration::Half,
                    NoteDuration::Whole
                ]
                .contains(&note.duration),
                "seed {seed}"
            );
        }
        // Level 3: full vocabulary appears across seeds.
        let notes = generate_with(seed, 4, 3, &options()).notes;
        if notes.iter().any(|n| n.duration == NoteDuration::DottedHalf) {
            saw_dotted = true;
        }
        if notes.iter().any(|n| n.duration == NoteDuration::Eighth) {
            saw_eighth = true;
        }
        if notes.iter().any(|n| n.is_rest()) {
            saw_rest = true;
        }
    }
    assert!(saw_dotted && saw_eighth && saw_rest);
}

#[test]
fn rest_constraints() {
    for seed in 1..=80u64 {
        let exercise = generate_with(seed, 3, 3, &options());
        assert!(!exercise.notes[0].is_rest(), "seed {seed}: rest can't open");
        let measures = exercise.measures();
        let rest_in_final_measure = measures.last().unwrap().iter().any(|n| n.is_rest());
        assert!(
            !rest_in_final_measure,
            "seed {seed}: no rest in the final measure"
        );
        for measure in &measures {
            let rest_count = measure.iter().filter(|n| n.is_rest()).count();
            assert!(rest_count <= 1, "seed {seed}: ≤1 rest per measure");
        }
    }
}

#[test]
fn moves_are_bounded_and_leaps_recover() {
    for seed in 1..=100u64 {
        let notes = generate(seed).sounded_notes();
        let positions: Vec<i32> = notes
            .iter()
            .map(|n| {
                SEED_PITCHES
                    .iter()
                    .position(|&p| p == n.midi.unwrap())
                    .unwrap() as i32
            })
            .collect();
        let mut previous_delta: i32 = 0;
        for (i, pair) in positions.windows(2).enumerate() {
            let delta = pair[1] - pair[0];
            let is_cadence = i == positions.len() - 2;
            assert!(delta.abs() <= 3, "seed {seed}: leap of {delta} positions");
            // The cadence is exempt: it may take a small jump home.
            if previous_delta.abs() >= 2 && !is_cadence {
                assert!(delta.abs() <= 1, "seed {seed}: no recovery after leap");
            }
            previous_delta = delta;
        }
    }
}

#[test]
fn motion_is_step_dominant() {
    let mut steps = 0;
    let mut total = 0;
    for seed in 1..=100u64 {
        let notes = generate(seed).sounded_notes();
        let positions: Vec<i32> = notes
            .iter()
            .map(|n| {
                SEED_PITCHES
                    .iter()
                    .position(|&p| p == n.midi.unwrap())
                    .unwrap() as i32
            })
            .collect();
        for pair in positions.windows(2) {
            total += 1;
            if (pair[1] - pair[0]).abs() <= 1 {
                steps += 1;
            }
        }
    }
    assert!(steps as f64 / total as f64 > 0.7);
}

#[test]
fn deterministic_for_same_seed() {
    assert_eq!(generate(42), generate(42));
}

#[test]
fn ends_on_do_or_sol() {
    // With C4–G4 active, a C or G is always within cadence reach.
    for seed in 1..=50u64 {
        let binding = generate(seed);
        let last = binding.sounded_notes();
        let last = last.last().copied().unwrap();
        let pc = last.midi.unwrap() % 12;
        assert!(
            pc == 0 || pc == 7,
            "seed {seed}: ended on {}",
            crate::core::PitchSpelling::name(last.midi.unwrap())
        );
    }
}

#[test]
fn interval_weights_shift_motion_shape() {
    let mut default_leaps = 0;
    let mut biased_leaps = 0;
    let mut default_total = 0;
    let mut biased_total = 0;
    fn leap_count(exercise: &Exercise) -> (i32, i32) {
        let positions: Vec<i32> = exercise
            .sounded_notes()
            .iter()
            .map(|n| {
                SEED_PITCHES
                    .iter()
                    .position(|&p| p == n.midi.unwrap())
                    .unwrap() as i32
            })
            .collect();
        let deltas: Vec<i32> = positions.windows(2).map(|p| p[1] - p[0]).collect();
        (
            deltas.iter().filter(|d| d.abs() >= 2).count() as i32,
            deltas.len() as i32,
        )
    }
    for seed in 1..=200u64 {
        let plain = leap_count(&generate(seed));
        default_leaps += plain.0;
        default_total += plain.1;

        let mut rng = SplitMix64::new(seed);
        let mut generator = ExerciseGenerator::default();
        generator.config.interval_weights = [(2, 5.0), (-2, 5.0)].into_iter().collect();
        let biased = leap_count(&generator.generate(&options(), &mut rng));
        biased_leaps += biased.0;
        biased_total += biased.1;
    }
    let default_rate = default_leaps as f64 / default_total as f64;
    let biased_rate = biased_leaps as f64 / biased_total as f64;
    assert!(
        biased_rate > default_rate * 1.5,
        "default {default_rate}, biased {biased_rate}"
    );
}

#[test]
fn generated_exercise_carries_configured_key() {
    let mut rng = SplitMix64::new(7);
    let mut generator = ExerciseGenerator::default();
    generator.config.fifths = 1;
    // G-major diatonic subset around the staff.
    let pitches: [u8; 5] = [62, 64, 66, 67, 69];
    let exercise = generator.generate(
        &pitches.iter().map(|&m| PitchOption::new(m)).collect::<Vec<_>>(),
        &mut rng,
    );
    assert_eq!(exercise.fifths, 1);
    for note in exercise.sounded_notes() {
        assert!(pitches.contains(&note.midi.unwrap()));
    }
}

#[test]
fn weak_item_bias_increases_frequency() {
    let target: u8 = 65; // F4
    let mut equal_count = 0;
    let mut biased_count = 0;
    let mut equal_total = 0;
    let mut biased_total = 0;
    for seed in 1..=200u64 {
        let equal = generate(seed).sounded_notes();
        equal_total += equal.len();
        equal_count += equal.iter().filter(|n| n.midi == Some(target)).count();

        let biased_pitches: Vec<PitchOption> = SEED_PITCHES
            .iter()
            .map(|&m| PitchOption::weighted(m, if m == target { 4.0 } else { 1.0 }))
            .collect();
        let biased = generate_with(seed, 2, 0, &biased_pitches).sounded_notes();
        biased_total += biased.len();
        biased_count += biased.iter().filter(|n| n.midi == Some(target)).count();
    }
    let equal_share = equal_count as f64 / equal_total as f64;
    let biased_share = biased_count as f64 / biased_total as f64;
    assert!(
        biased_share > equal_share * 1.3,
        "equal {equal_share}, biased {biased_share}"
    );
}

#[test]
fn drill_note_is_single_whole_note_from_set() {
    let mut rng = SplitMix64::new(3);
    let exercise = ExerciseGenerator::drill_note(&options(), &mut rng);
    assert_eq!(exercise.notes.len(), 1);
    assert_eq!(exercise.notes[0].duration, NoteDuration::Whole);
    assert!(SEED_PITCHES.contains(&exercise.notes[0].midi.unwrap()));
}
