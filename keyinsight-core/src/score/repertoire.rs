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
        DifficultyDescriptors::compute(&self.exercise).index()
    }
}

macro_rules! piece {
    ($slug:literal) => {
        (
            $slug,
            include_bytes!(concat!("../../assets/pieces/", $slug, ".musicxml")) as &[u8],
        )
    };
}

/// (slug, bytes) for every bundled piece, sorted by filename (including
/// the `.musicxml` extension, so `-two-hands` editions sort before their
/// base: `'-' < '.'`) — the same order the Swift `Bundle.module`
/// enumeration produced.
const BUNDLED_PIECES: &[(&str, &[u8])] = &[
    piece!("amazing-grace-two-hands"),
    piece!("amazing-grace"),
    piece!("au-clair-de-la-lune-two-hands"),
    piece!("au-clair-de-la-lune"),
    piece!("auld-lang-syne-two-hands"),
    piece!("auld-lang-syne"),
    piece!("baa-baa-black-sheep-two-hands"),
    piece!("baa-baa-black-sheep"),
    piece!("camptown-races-two-hands"),
    piece!("camptown-races"),
    piece!("canon-in-d-two-hands"),
    piece!("canon-in-d"),
    piece!("eine-kleine-nachtmusik-two-hands"),
    piece!("eine-kleine-nachtmusik"),
    piece!("frere-jacques-two-hands"),
    piece!("frere-jacques"),
    piece!("friska-two-hands"),
    piece!("friska"),
    piece!("fur-elise-two-hands"),
    piece!("fur-elise"),
    piece!("good-king-wenceslas-two-hands"),
    piece!("good-king-wenceslas"),
    piece!("gymnopedie-1"),
    piece!("happy-birthday-two-hands"),
    piece!("happy-birthday"),
    piece!("hot-cross-buns-two-hands"),
    piece!("hot-cross-buns"),
    piece!("jingle-bells-two-hands"),
    piece!("jingle-bells"),
    piece!("korobeiniki-two-hands"),
    piece!("korobeiniki"),
    piece!("kumbaya-two-hands"),
    piece!("kumbaya"),
    piece!("lightly-row-two-hands"),
    piece!("lightly-row"),
    piece!("london-bridge-two-hands"),
    piece!("london-bridge"),
    piece!("mary-had-a-little-lamb-two-hands"),
    piece!("mary-had-a-little-lamb"),
    piece!("minuet-in-g-full-two-hands"),
    piece!("minuet-in-g-full"),
    piece!("minuet-in-g"),
    piece!("moonlight-opening"),
    piece!("my-country-tis-of-thee-two-hands"),
    piece!("my-country-tis-of-thee"),
    piece!("ode-to-joy-full"),
    piece!("ode-to-joy-two-hands"),
    piece!("ode-to-joy"),
    piece!("old-macdonald-two-hands"),
    piece!("old-macdonald"),
    piece!("sheep-may-safely-graze"),
    piece!("silent-night-two-hands"),
    piece!("silent-night"),
    piece!("solace"),
    piece!("surprise-symphony-two-hands"),
    piece!("surprise-symphony"),
    piece!("twinkle-twinkle-g"),
    piece!("twinkle-twinkle-two-hands"),
    piece!("twinkle-twinkle"),
    piece!("yankee-doodle-two-hands"),
    piece!("yankee-doodle"),
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
