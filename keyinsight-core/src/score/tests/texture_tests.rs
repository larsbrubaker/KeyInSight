//! Ports the encoder half of `SurvivalTextureTests` from
//! `Tests/KeyInSightTests/GeneratorTests.swift`. The texture test proper
//! (`melody_in_bass_flips_the_both_texture`) arrives with the generator's
//! hands modes; the renderer test is notation-owned.

use crate::core::SplitMix64;
use crate::score::{ExerciseGenerator, MusicXmlEncoder, MusicXmlImporter, PitchOption};

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
