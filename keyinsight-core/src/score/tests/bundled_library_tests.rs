//! Ports `RepertoireLibraryTests` from
//! `Tests/KeyInSightTests/RepertoireTests.swift`: the bundled library
//! loads, pairs its editions, and every piece engraves.

use std::collections::HashSet;

use crate::notation::NotationRenderer;
use crate::score::{MusicXmlEncoder, RepertoireLibrary, RepertoirePiece};

fn piece<'a>(pieces: &'a [RepertoirePiece], slug: &str) -> &'a RepertoirePiece {
    pieces
        .iter()
        .find(|p| p.slug == slug)
        .unwrap_or_else(|| panic!("bundled piece {slug} missing"))
}

#[test]
fn bundled_pieces_load() {
    let pieces = RepertoireLibrary::bundled();
    // Every bundled file must parse — a piece that fails the importer is
    // silently dropped by bundled(), so pin the exact count.
    assert_eq!(pieces.len(), 61);
    let slugs: HashSet<&str> = pieces.iter().map(|p| p.slug.as_str()).collect();
    assert_eq!(slugs.len(), pieces.len());
    // Every two-hand edition sits alongside its one-hand base (the library
    // window pairs them onto one row).
    for piece in &pieces {
        if let Some(base) = piece.slug.strip_suffix("-two-hands") {
            assert!(slugs.contains(base), "{} has no base edition", piece.slug);
            assert!(
                piece.exercise.is_two_voice(),
                "{} has no bass voice",
                piece.slug
            );
        }
    }

    let minuet = piece(&pieces, "minuet-in-g");
    assert_eq!(minuet.title, "Minuet in G");
    assert_eq!(minuet.exercise.fifths, 1);
    assert_eq!(minuet.exercise.beats_per_measure, 3);
    assert_eq!(minuet.exercise.measures().len(), 8);
    // Every measure exactly full (last one is a dotted half = 6 units).
    for measure in minuet.exercise.measures() {
        assert_eq!(measure.iter().map(|n| n.duration.units()).sum::<i32>(), 6);
    }
    // Twinkle is the easiest of the set; Solace is the hardest melody.
    let twinkle = piece(&pieces, "twinkle-twinkle");
    let solace = piece(&pieces, "solace");
    assert!(twinkle.difficulty_index() < minuet.difficulty_index());
    assert!(twinkle.difficulty_index() < solace.difficulty_index());
}

/// The 2026-07-21 library batch: 20 songs in both editions, plus the full
/// Minuet A section and a two-hand Twinkle.
#[test]
fn new_library_batch_loads() {
    let pieces = RepertoireLibrary::bundled();
    // Für Elise: 3/4 adaptation with accidentals (D#5, G#4) and ties.
    let elise = piece(&pieces, "fur-elise");
    assert_eq!(elise.exercise.beats_per_measure, 3);
    let elise_sounded = elise.exercise.sounded_notes();
    assert!(elise_sounded.iter().any(|n| n.midi == Some(75))); // D#5
    assert!(elise_sounded.iter().any(|n| n.midi == Some(68))); // G#4
    assert!(elise.exercise.notes.iter().any(|n| n.tied_from_previous));
    // Silent Night: 24 bars of 3/4; each "peace" holds across a tie.
    let night = piece(&pieces, "silent-night");
    assert_eq!(night.exercise.measures().len(), 24);
    assert_eq!(
        night
            .exercise
            .notes
            .iter()
            .filter(|n| n.tied_from_previous)
            .count(),
        2
    );
    // Canon in D: two sharps; the two-hand edition walks the ground bass.
    let canon = piece(&pieces, "canon-in-d-two-hands");
    assert_eq!(canon.exercise.fifths, 2);
    assert_eq!(canon.exercise.bass_notes[0].midi, Some(50)); // D3
    // Amazing Grace: rest-padded pickup — play starts on the G3 upbeat.
    let grace = piece(&pieces, "amazing-grace");
    assert!(grace.exercise.notes[0].is_rest());
    assert_eq!(grace.exercise.sounded_notes()[0].midi, Some(55));
    // Minuet in G (full): 16 bars whose first 8 match the short edition.
    let full = piece(&pieces, "minuet-in-g-full");
    let short = piece(&pieces, "minuet-in-g");
    assert_eq!(full.exercise.measures().len(), 16);
    assert_eq!(full.exercise.measures()[..8], short.exercise.measures()[..]);
    // Tetris: an A-natural-minor line inside the C signature.
    let tetris = piece(&pieces, "korobeiniki");
    assert_eq!(tetris.exercise.fifths, 0);
    assert_eq!(
        tetris.exercise.sounded_notes().last().and_then(|n| n.midi),
        Some(57)
    ); // ends on A3
}

/// Import is half the pipeline — every bundled piece must also engrave (a
/// piece that imports but fails Verovio would die at play time).
#[test]
fn all_bundled_pieces_engrave() {
    let mut renderer = NotationRenderer::new();
    for piece in RepertoireLibrary::bundled() {
        let rendered = renderer.render(&MusicXmlEncoder::encode(&piece.exercise));
        let rendered = rendered.unwrap_or_else(|| panic!("{} failed to engrave", piece.slug));
        assert!(
            !rendered.note_ids.is_empty(),
            "{} engraved no notes",
            piece.slug
        );
    }
}

#[test]
fn measure_indices_for_sounded_notes() {
    let pieces = RepertoireLibrary::bundled();
    let ode = piece(&pieces, "ode-to-joy");
    let indices = ode.exercise.sounded_note_measure_indices();
    assert_eq!(indices.len(), ode.exercise.sounded_notes().len());
    assert_eq!(indices.first(), Some(&0));
    assert_eq!(indices.last(), Some(&7));
    // Monotonically non-decreasing.
    for pair in indices.windows(2) {
        assert!(pair[0] <= pair[1]);
    }
}
