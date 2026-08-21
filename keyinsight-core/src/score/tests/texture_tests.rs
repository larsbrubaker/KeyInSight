//! Ports `SurvivalTextureTests` from
//! `Tests/KeyInSightTests/GeneratorTests.swift`: the melody-hand flip and
//! two-bar lines. `rendererHonorsEncodedBreaks` (and the
//! `SystemLayoutProbe` / `BarlineAlignmentProbe` /
//! `RenderOptionStabilityProbe` suites) need the renderer's per-render
//! feed-layout flag and arrive with the notation step.

use std::collections::HashSet;

use crate::core::SplitMix64;
use crate::score::{
    ExerciseGenerator, Hands, MusicXmlEncoder, MusicXmlImporter, NoteDuration, PitchOption, Staff,
};

#[test]
fn melody_in_bass_flips_the_both_texture() {
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 4;
    generator.config.hands = Hands::Both;
    generator.config.melody_in_bass = true;
    let mut rng = SplitMix64::new(11);
    let bass_seed: Vec<PitchOption> = [48, 50, 52, 53, 55]
        .iter()
        .map(|&m| PitchOption::new(m))
        .collect();
    let ex = generator.generate(&bass_seed, &mut rng);
    // The walk lives in the bass voice, drawn from the bass set.
    assert!(ex.is_two_voice());
    assert!(ex.bass_notes.iter().all(|n| n.staff == Staff::Bass));
    let allowed: HashSet<u8> = bass_seed.iter().map(|p| p.midi).collect();
    for note in ex.bass_notes.iter().filter(|n| !n.is_rest()) {
        assert!(allowed.contains(&note.midi.unwrap()));
    }
    let distinct: HashSet<u8> = ex.bass_notes.iter().filter_map(|n| n.midi).collect();
    assert!(distinct.len() > 1, "bass voice should be the moving line");
    // The right hand holds I/V long tones in the treble staff (C4/G4).
    assert_eq!(ex.notes.len(), 4);
    assert!(ex
        .notes
        .iter()
        .all(|n| n.staff == Staff::Treble && n.duration == NoteDuration::Whole));
    assert_eq!(
        ex.notes.iter().map(|n| n.midi.unwrap()).collect::<Vec<_>>(),
        [60, 67, 60, 60]
    );
}

#[test]
fn encoder_emits_system_breaks_every_two_measures() {
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 8;
    let mut rng = SplitMix64::new(3);
    let pitches: Vec<PitchOption> = [60, 62, 64, 65, 67]
        .iter()
        .map(|&m| PitchOption::new(m))
        .collect();
    let ex = generator.generate(&pitches, &mut rng);
    let plain = MusicXmlEncoder::encode(&ex);
    let broken = MusicXmlEncoder::encode_with_breaks(&ex, Some(2));
    assert!(!plain.contains("<print new-system=\"yes\"/>"));
    // Breaks open measures 3, 5, 7 — never the first.
    assert_eq!(broken.matches("<print new-system=\"yes\"/>").count(), 3);
    // The break rides inside the measure, importer-transparent.
    let piece = MusicXmlImporter::parse(broken.as_bytes(), "rt").ok();
    assert_eq!(piece.map(|p| p.exercise), Some(ex));
}
