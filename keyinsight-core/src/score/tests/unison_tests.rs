//! Ports `UnisonCollisionTests` from
//! `Tests/KeyInSightTests/GeneratorTests.swift`: a melody onset never
//! doubles the accompaniment long tone sounding at that moment — two
//! noteheads, one physical key isn't real notation.

use std::collections::HashSet;

use crate::core::SplitMix64;
use crate::score::{ExerciseGenerator, Hands, PitchOption};

fn assert_no_doubled_keys(generator: &ExerciseGenerator, midis: &[u8]) {
    let pitches: Vec<PitchOption> = midis.iter().map(|&m| PitchOption::new(m)).collect();
    for seed in 1..=80u64 {
        let mut rng = SplitMix64::new(seed);
        let ex = generator.generate(&pitches, &mut rng);
        for event in ex.match_events() {
            // Letter-level: even an octave double ("C over C", struck
            // together) reads as a bug at the beginner stage. (`Both`
            // textures only — chord shapes may legitimately double a
            // letter within one hand.)
            let letters: Vec<u8> = event.pitches.iter().map(|p| p % 12).collect();
            let unique: HashSet<u8> = letters.iter().copied().collect();
            assert_eq!(
                unique.len(),
                letters.len(),
                "seed {seed}: letter doubled across staves {:?}",
                event.pitches
            );
        }
    }
}

#[test]
fn melody_never_doubles_the_bass_long_tone() {
    // G major with the melody range reaching down to the G3 tonic tone —
    // the exact collision the fix targets.
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 4;
    generator.config.hands = Hands::Both;
    generator.config.fifths = 1;
    assert_no_doubled_keys(&generator, &[55, 57, 59, 62, 64, 66, 67]);
}

#[test]
fn bass_walk_never_doubles_the_treble_long_tone() {
    // melody_in_bass in C: the bass walk can reach C4, the RH holds C4.
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 4;
    generator.config.hands = Hands::Both;
    generator.config.melody_in_bass = true;
    assert_no_doubled_keys(&generator, &[48, 50, 52, 53, 55, 57, 59, 60]);
}
