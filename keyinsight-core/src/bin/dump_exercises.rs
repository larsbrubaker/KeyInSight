//! Dumps a fixed, documented set of generated exercises as MusicXML for the
//! Verovio reference harness (`tools/reference-harness/`).
//!
//! The corpus under `keyinsight-core/assets/pieces/` covers repertoire;
//! this covers what the *generator* + `MusicXmlEncoder` actually emit at
//! runtime (single-staff walks, bass-only walks, grand-staff textures,
//! chords, rests, dotted values, eighths, encoded system breaks) so the
//! engraving-parity goldens exercise the same MusicXML shapes the app
//! renders. Output is fully deterministic (SplitMix64 seeds), so the files
//! are committed and only change when the generator or encoder changes.
//!
//! ```text
//! cargo run -p keyinsight-core --bin dump_exercises [OUT_DIR]
//! ```
//!
//! `OUT_DIR` defaults to `tools/reference-harness/generated/` relative to
//! the workspace root. Files are named
//! `gen-s<seed>-<hands>-<measures>m.musicxml` plus one
//! `gen-feed-8m.musicxml` (four stitched 2-measure chunks encoded with a
//! system break every 2 measures — the survival feed layout).

use std::fs;
use std::path::{Path, PathBuf};

use keyinsight_core::core::SplitMix64;
use keyinsight_core::score::{
    ChordShape, Exercise, ExerciseGenerator, Hands, MusicXmlEncoder, PitchOption,
};

/// Seeds are deliberately few and fixed: the harness goldens are keyed by
/// file name, so this list is part of the golden contract.
const SEEDS: [u64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const MEASURE_COUNTS: [i32; 2] = [2, 4];
const HANDS: [Hands; 3] = [Hands::Right, Hands::Left, Hands::Both];

/// Active pitch sets per key (fifths 0 / 1 / 2), one octave of the scale
/// that the skill ladder unlocks first for each hand.
fn treble_pitches(fifths: i32) -> Vec<PitchOption> {
    let midis: &[u8] = match fifths {
        1 => &[55, 57, 59, 60, 62, 64, 66, 67],
        2 => &[62, 64, 66, 67, 69, 71, 73, 74],
        _ => &[60, 62, 64, 65, 67, 69, 71, 72],
    };
    midis.iter().map(|&m| PitchOption::new(m)).collect()
}

fn bass_pitches(fifths: i32) -> Vec<PitchOption> {
    let midis: &[u8] = match fifths {
        1 => &[43, 45, 47, 48, 50, 52, 54, 55],
        2 => &[50, 52, 54, 55, 57, 59, 61, 62],
        _ => &[48, 50, 52, 53, 55, 57, 59, 60],
    };
    midis.iter().map(|&m| PitchOption::new(m)).collect()
}

/// Generator settings for one (seed, hands, measures) cell. The seed picks
/// the key and rhythm level: seeds 1-4 walk the rhythm ladder (levels 0-3,
/// whole / dotted-half / eighth-pair / rest vocabularies), seeds 5-8 run
/// the full vocabulary; `seed % 3` picks the key; right-hand even seeds
/// unlock chord shapes so chords appear.
fn configure(generator: &mut ExerciseGenerator, seed: u64, hands: Hands, measures: i32) {
    let config = &mut generator.config;
    config.measures = measures;
    config.beats_per_measure = 4;
    config.rhythm_level = if seed <= 4 { seed as i32 - 1 } else { 3 };
    config.fifths = (seed % 3) as i32;
    config.interval_weights.clear();
    config.transition_weights.clear();
    config.allowed_steps = [0, 1, 2, 3, 4].into_iter().collect();
    config.probe_step = None;
    config.chord_shapes = if hands == Hands::Right && seed % 2 == 0 {
        ["chord:harm-3rd", "chord:harm-5th", "chord:triad"]
            .iter()
            .filter_map(|item| ChordShape::by_skill_item(item).map(|shape| (shape, 1.0)))
            .collect()
    } else {
        Vec::new()
    };
    config.probe_chord_shape = None;
    config.hands = hands;
    config.melody_in_bass = hands == Hands::Both && seed % 4 == 3;
}

fn generate(seed: u64, hands: Hands, measures: i32) -> Exercise {
    let mut generator = ExerciseGenerator::default();
    configure(&mut generator, seed, hands, measures);
    let fifths = generator.config.fifths;
    let walk_in_bass = hands == Hands::Left || generator.config.melody_in_bass;
    let pitches = if walk_in_bass {
        bass_pitches(fifths)
    } else {
        treble_pitches(fifths)
    };
    let mut rng = SplitMix64::new(seed);
    generator.generate(&pitches, &mut rng)
}

fn hands_tag(hands: Hands) -> &'static str {
    hands.raw_value()
}

fn default_out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("keyinsight-core lives under the workspace root")
        .join("tools")
        .join("reference-harness")
        .join("generated")
}

fn main() {
    let out_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_out_dir);
    fs::create_dir_all(&out_dir)
        .unwrap_or_else(|err| panic!("create {}: {err}", out_dir.display()));

    let mut written = 0usize;
    let mut write = |name: String, xml: &str| {
        let path = out_dir.join(&name);
        fs::write(&path, xml).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
        written += 1;
    };

    for &seed in &SEEDS {
        for &hands in &HANDS {
            for &measures in &MEASURE_COUNTS {
                let exercise = generate(seed, hands, measures);
                let xml = MusicXmlEncoder::encode(&exercise);
                write(
                    format!("gen-s{seed}-{}-{measures}m.musicxml", hands_tag(hands)),
                    &xml,
                );
            }
        }
    }

    // Survival feed window: four 2-measure chunks stitched, encoded system
    // break every two measures (rendered with the feed option set).
    let chunks: Vec<Exercise> = [11u64, 12, 13, 14]
        .iter()
        .map(|&seed| generate(seed, Hands::Both, 2))
        .collect();
    let feed = Exercise::stitched(&chunks);
    write(
        "gen-feed-8m.musicxml".to_string(),
        &MusicXmlEncoder::encode_with_breaks(&feed, Some(2)),
    );

    println!("wrote {written} exercises to {}", out_dir.display());
}
