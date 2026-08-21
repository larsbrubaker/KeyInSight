//! Ports `SurvivalTextureTests` from
//! `Tests/KeyInSightTests/GeneratorTests.swift`: the melody-hand flip and
//! two-bar lines. The `SystemLayoutProbe` / `BarlineAlignmentProbe` /
//! `RenderOptionStabilityProbe` suites live in `notation/tests.rs`.

use std::collections::HashSet;

use crate::core::SplitMix64;
use crate::notation::NotationRenderer;
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

#[test]
fn renderer_honors_encoded_breaks() {
    let mut renderer = NotationRenderer::new();
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 4;
    let mut rng = SplitMix64::new(5);
    let pitches: Vec<PitchOption> = [60, 62, 64, 65, 67]
        .iter()
        .map(|&m| PitchOption::new(m))
        .collect();
    let ex = generator.generate(&pitches, &mut rng);
    let xml = MusicXmlEncoder::encode_with_breaks(&ex, Some(2));
    renderer.render_with(&xml, true).expect("feed render");
    let systems = renderer.toolkit().current_layout().unwrap().systems.len();
    assert_eq!(systems, 2, "expected 2 two-bar systems, got {systems}");
    // And the sticky option resets: auto layout keeps 4 bars on one line.
    renderer
        .render(&MusicXmlEncoder::encode(&ex))
        .expect("auto render");
    let auto_systems = renderer.toolkit().current_layout().unwrap().systems.len();
    assert_eq!(
        auto_systems, 1,
        "auto breaks regressed to {auto_systems} systems"
    );
}
