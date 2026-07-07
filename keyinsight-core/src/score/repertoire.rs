//! Bundled starter library: short public-domain melodies conforming to the
//! import subset, shipped as MusicXML resources compiled into the binary
//! (both targets need them without filesystem access).
//!
//! Ports `Score/RepertoireLibrary.swift`. Swift enumerated
//! `Bundle.module` resources at runtime; here the pieces are a static
//! manifest (already in the sorted order the Swift code produced).

use crate::score::{DifficultyDescriptors, Exercise, MusicXmlImporter};

#[derive(Debug, Clone, PartialEq)]
pub struct RepertoirePiece {
    pub slug: String,
    pub title: String,
    pub exercise: Exercise,
}

impl RepertoirePiece {
    pub fn id(&self) -> &str {
        &self.slug
    }

    /// Interpretable difficulty index from the descriptors — a rough sort
    /// key until the scale is calibrated against syllabus lists.
    pub fn difficulty_index(&self) -> f64 {
        let d = DifficultyDescriptors::compute(&self.exercise);
        d.pitch_entropy_bits
            + 3.0 * d.leap_ratio
            + d.notes_per_measure / 2.0
            + d.range_semitones as f64 / 12.0
    }
}

/// (slug, bytes) for every bundled piece, sorted by filename — the same
/// order the Swift `Bundle.module` enumeration produced.
const BUNDLED_PIECES: &[(&str, &[u8])] = &[
    (
        "camptown-races-two-hands",
        include_bytes!("../../assets/pieces/camptown-races-two-hands.musicxml"),
    ),
    (
        "camptown-races",
        include_bytes!("../../assets/pieces/camptown-races.musicxml"),
    ),
    (
        "friska-two-hands",
        include_bytes!("../../assets/pieces/friska-two-hands.musicxml"),
    ),
    ("friska", include_bytes!("../../assets/pieces/friska.musicxml")),
    (
        "gymnopedie-1",
        include_bytes!("../../assets/pieces/gymnopedie-1.musicxml"),
    ),
    (
        "happy-birthday-two-hands",
        include_bytes!("../../assets/pieces/happy-birthday-two-hands.musicxml"),
    ),
    (
        "happy-birthday",
        include_bytes!("../../assets/pieces/happy-birthday.musicxml"),
    ),
    (
        "jingle-bells-two-hands",
        include_bytes!("../../assets/pieces/jingle-bells-two-hands.musicxml"),
    ),
    (
        "jingle-bells",
        include_bytes!("../../assets/pieces/jingle-bells.musicxml"),
    ),
    (
        "minuet-in-g",
        include_bytes!("../../assets/pieces/minuet-in-g.musicxml"),
    ),
    (
        "moonlight-opening",
        include_bytes!("../../assets/pieces/moonlight-opening.musicxml"),
    ),
    (
        "ode-to-joy-full",
        include_bytes!("../../assets/pieces/ode-to-joy-full.musicxml"),
    ),
    (
        "ode-to-joy-two-hands",
        include_bytes!("../../assets/pieces/ode-to-joy-two-hands.musicxml"),
    ),
    (
        "ode-to-joy",
        include_bytes!("../../assets/pieces/ode-to-joy.musicxml"),
    ),
    (
        "sheep-may-safely-graze",
        include_bytes!("../../assets/pieces/sheep-may-safely-graze.musicxml"),
    ),
    ("solace", include_bytes!("../../assets/pieces/solace.musicxml")),
    (
        "twinkle-twinkle-g",
        include_bytes!("../../assets/pieces/twinkle-twinkle-g.musicxml"),
    ),
    (
        "twinkle-twinkle",
        include_bytes!("../../assets/pieces/twinkle-twinkle.musicxml"),
    ),
];

pub struct RepertoireLibrary;

impl RepertoireLibrary {
    /// Parse every bundled piece; pieces that fail the import subset are
    /// skipped (mirrors the Swift `compactMap` + NSLog behavior).
    pub fn bundled() -> Vec<RepertoirePiece> {
        BUNDLED_PIECES
            .iter()
            .filter_map(|(slug, bytes)| match MusicXmlImporter::parse(bytes, slug) {
                Ok(imported) => Some(RepertoirePiece {
                    slug: (*slug).to_string(),
                    title: imported.title,
                    exercise: imported.exercise,
                }),
                Err(err) => {
                    // The Swift app logged and skipped; keep that behavior.
                    eprintln!("KeyInSight: bundled piece {slug} failed to parse: {err}");
                    None
                }
            })
            .collect()
    }
}
