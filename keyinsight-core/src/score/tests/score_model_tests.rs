//! Ports `Tests/KeyInSightTests/ScoreModelTests.swift`
//! (PitchSpellingTests + MusicXMLEncoderTests).

use crate::core::PitchSpelling;
use crate::score::{Exercise, MusicXmlEncoder, NoteDuration, ScoreNote};

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

// --- PitchSpellingTests ---

#[test]
fn spelling() {
    assert_eq!(PitchSpelling::name(60), "C4");
    assert_eq!(PitchSpelling::name(61), "C#4");
    assert_eq!(PitchSpelling::name(67), "G4");
    assert_eq!(PitchSpelling::name(72), "C5");
    assert_eq!(PitchSpelling::name(59), "B3");
}

#[test]
fn diatonic_index_is_one_per_staff_position() {
    // C4 D4 E4 F4 G4 A4 B4 C5 are consecutive staff positions.
    let whites: [u8; 8] = [60, 62, 64, 65, 67, 69, 71, 72];
    let indices: Vec<i32> = whites.iter().map(|&m| PitchSpelling::diatonic_index(m)).collect();
    for pair in indices.windows(2) {
        assert_eq!(pair[1] - pair[0], 1);
    }
    // Sharps share their natural's staff position.
    assert_eq!(
        PitchSpelling::diatonic_index(61),
        PitchSpelling::diatonic_index(60)
    );
    assert_eq!(
        PitchSpelling::diatonic_index(66),
        PitchSpelling::diatonic_index(65)
    );
}

// --- MusicXMLEncoderTests ---

#[test]
fn encodes_well_formed_score() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::note(62, NoteDuration::Quarter),
            ScoreNote::note(64, NoteDuration::Half),
            ScoreNote::note(67, NoteDuration::Whole),
        ],
        4,
    );
    let xml = MusicXmlEncoder::encode(&exercise);

    assert!(xml.contains("<score-partwise"));
    assert_eq!(count_occurrences(&xml, "<measure"), 2);
    assert_eq!(count_occurrences(&xml, "<note>"), 4);
    assert!(xml.contains("<step>C</step>"));
    assert!(xml.contains("<type>whole</type>"));
    // Eighth-note divisions: a quarter spans 2 units.
    assert!(xml.contains("<divisions>2</divisions>"));
    assert!(xml.contains("<duration>2</duration>"));
    // Attributes only on the first measure.
    assert_eq!(count_occurrences(&xml, "<attributes>"), 1);
    // Parses as XML (round-trips through our importer).
    assert!(crate::score::MusicXmlImporter::parse(xml.as_bytes(), "t").is_ok());
}

#[test]
fn sharp_gets_alter_and_accidental() {
    let xml = MusicXmlEncoder::encode(&Exercise::new(
        vec![ScoreNote::note(66, NoteDuration::Whole)],
        4,
    ));
    assert!(xml.contains("<alter>1</alter>"));
    assert!(xml.contains("<accidental>sharp</accidental>"));
}

#[test]
fn key_signature_suppresses_in_key_accidentals() {
    // F#4 in G major: signature carries the sharp — alter yes, accidental no.
    let g_major = MusicXmlEncoder::encode(
        &Exercise::new(vec![ScoreNote::note(66, NoteDuration::Whole)], 4).with_fifths(1),
    );
    assert!(g_major.contains("<fifths>1</fifths>"));
    assert!(g_major.contains("<alter>1</alter>"));
    assert!(!g_major.contains("<accidental>"));

    // The same F#4 in C major still needs the accidental.
    let c_major = MusicXmlEncoder::encode(&Exercise::new(
        vec![ScoreNote::note(66, NoteDuration::Whole)],
        4,
    ));
    assert!(c_major.contains("<accidental>sharp</accidental>"));

    // G#4 in G major is NOT in the signature: accidental required.
    let g_sharp = MusicXmlEncoder::encode(
        &Exercise::new(vec![ScoreNote::note(68, NoteDuration::Whole)], 4).with_fifths(1),
    );
    assert!(g_sharp.contains("<accidental>sharp</accidental>"));
}

#[test]
fn exercise_spec_decoding_defaults_fifths() {
    // Pre-key stored specs have no fifths field.
    let old = r#"{"notes":[{"midi":60,"duration":"whole"}],"beatsPerMeasure":4}"#;
    let decoded: Exercise = serde_json::from_str(old).expect("legacy spec decodes");
    assert_eq!(decoded.fifths, 0);
}

#[test]
fn rests_and_dots_encode() {
    let xml = MusicXmlEncoder::encode(&Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::DottedHalf),
            ScoreNote::rest(NoteDuration::Quarter),
            ScoreNote::note(62, NoteDuration::Eighth),
            ScoreNote::note(64, NoteDuration::Eighth),
            ScoreNote::note(65, NoteDuration::Half),
            ScoreNote::note(67, NoteDuration::Quarter),
        ],
        4,
    ));
    assert!(xml.contains("<rest/>"));
    assert!(xml.contains("<dot/>"));
    assert!(xml.contains("<type>eighth</type>"));
    // The dotted half encodes as type "half" + dot, duration 6 units.
    assert!(xml.contains("<duration>6</duration>"));
    assert!(crate::score::MusicXmlImporter::parse(xml.as_bytes(), "t").is_ok());
}
