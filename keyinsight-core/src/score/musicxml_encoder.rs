//! Encodes the internal score model to MusicXML (round-trips through the
//! importer; feeds notation tooling and export).
//! Subset: single part, treble or grand staff, key signatures, one meter,
//! rests, dotted values, eighths, chords, ties.
//!
//! Ports `Score/MusicXMLEncoder.swift`, preserving the exact output shape
//! (whitespace included) so encoder tests can pin strings.

use crate::core::PitchSpelling;
use crate::score::{Exercise, ScoreNote, Staff, TieRole};

pub struct MusicXmlEncoder;

const SHARP_ORDER: [&str; 7] = ["F", "C", "G", "D", "A", "E", "B"];
const FLAT_ORDER: [&str; 7] = ["B", "E", "A", "D", "G", "C", "F"];

impl MusicXmlEncoder {
    /// divisions = eighth-note units per quarter.
    pub const DIVISIONS: i32 = 2;

    pub fn encode(exercise: &Exercise) -> String {
        // Any bass-staff note (or a bass voice) promotes the whole score to
        // a grand staff (brace, both clefs, per-note staff routing).
        let grand_staff = exercise.is_two_voice()
            || exercise.notes.iter().any(|n| n.staff == Staff::Bass);
        let treble_measures = exercise.measures();
        let bass_measures = exercise.bass_measures();
        let treble_ties = Exercise::tie_roles(&exercise.notes);
        let bass_ties = Exercise::tie_roles(&exercise.bass_notes);
        let mut treble_index = 0;
        let mut bass_index = 0;
        let mut measures_xml = String::new();
        for i in 0..exercise.measure_count() {
            let clefs = if grand_staff {
                "\n        <staves>2</staves>\n        <clef number=\"1\"><sign>G</sign><line>2</line></clef>\n        <clef number=\"2\"><sign>F</sign><line>4</line></clef>"
            } else {
                "\n        <clef><sign>G</sign><line>2</line></clef>"
            };
            let attributes = if i == 0 {
                format!(
                    "\n      <attributes>\n        <divisions>{}</divisions>\n        <key><fifths>{}</fifths></key>\n        <time><beats>{}</beats><beat-type>4</beat-type></time>{}\n      </attributes>",
                    Self::DIVISIONS,
                    exercise.fifths,
                    exercise.beats_per_measure,
                    clefs
                )
            } else {
                String::new()
            };
            let empty: Vec<ScoreNote> = Vec::new();
            let treble = treble_measures.get(i).unwrap_or(&empty);
            let mut notes = String::new();
            for note in treble {
                notes += &note_xml(
                    note,
                    exercise.fifths,
                    grand_staff,
                    1,
                    treble_ties[treble_index],
                );
                treble_index += 1;
            }
            // Independent bass voice: rewind to the top of the measure, then
            // write voice 2 on staff 2.
            if let Some(bass) = bass_measures.get(i) {
                if !bass.is_empty() {
                    let treble_units: i32 = treble
                        .iter()
                        .filter(|n| !n.chord_with_previous)
                        .map(|n| n.duration.units())
                        .sum();
                    notes +=
                        &format!("\n      <backup><duration>{treble_units}</duration></backup>");
                    for note in bass {
                        let routed = ScoreNote {
                            midi: note.midi,
                            duration: note.duration,
                            staff: Staff::Bass,
                            chord_with_previous: note.chord_with_previous,
                            tied_from_previous: note.tied_from_previous,
                        };
                        notes += &note_xml(
                            &routed,
                            exercise.fifths,
                            grand_staff,
                            2,
                            bass_ties[bass_index],
                        );
                        bass_index += 1;
                    }
                }
            }
            measures_xml += &format!(
                "\n    <measure number=\"{}\">{}{}\n    </measure>",
                i + 1,
                attributes,
                notes
            );
        }
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<score-partwise version=\"4.0\">\n  <part-list><score-part id=\"P1\"><part-name print-object=\"no\">Piano</part-name></score-part></part-list>\n  <part id=\"P1\">{measures_xml}\n  </part>\n</score-partwise>"
        )
    }

    /// The alteration a key signature applies to a letter (+1/−1/0).
    pub fn key_alteration(step: &str, fifths: i32) -> i32 {
        if fifths > 0 && SHARP_ORDER[..(fifths.min(7) as usize)].contains(&step) {
            return 1;
        }
        if fifths < 0 && FLAT_ORDER[..((-fifths).min(7) as usize)].contains(&step) {
            return -1;
        }
        0
    }
}

fn note_xml(note: &ScoreNote, fifths: i32, grand_staff: bool, voice: i32, tie: TieRole) -> String {
    let dot = if note.duration.is_dotted() { "<dot/>" } else { "" };
    let voice_xml = if grand_staff {
        format!("<voice>{voice}</voice>")
    } else {
        String::new()
    };
    // MusicXML puts <staff> after type/dot/accidental.
    let staff = if grand_staff {
        format!(
            "<staff>{}</staff>",
            if note.staff == Staff::Bass { 2 } else { 1 }
        )
    } else {
        String::new()
    };
    let Some(midi) = note.midi else {
        return format!(
            "\n      <note>\n        <rest/>\n        <duration>{}</duration>\n        {}<type>{}</type>{}{}\n      </note>",
            note.duration.units(),
            voice_xml,
            note.duration.music_xml_type(),
            dot,
            staff
        );
    };
    // <chord/> members sound with the preceding note.
    let chord = if note.chord_with_previous { "<chord/>" } else { "" };
    // Flat keys spell black keys as flats.
    let s = PitchSpelling::spell(midi, fifths < 0);
    let alter = if s.alter != 0 {
        format!("<alter>{}</alter>", s.alter)
    } else {
        String::new()
    };
    // The key signature carries in-key alterations — a glyph appears only
    // when the note contradicts it (accidental or natural).
    let mut accidental = "";
    if s.alter != MusicXmlEncoder::key_alteration(s.step, fifths) {
        accidental = [
            "<accidental>flat</accidental>",
            "<accidental>natural</accidental>",
            "<accidental>sharp</accidental>",
        ][(s.alter + 1) as usize];
    }
    // Ties: sound element(s) after duration, notation after staff.
    let mut tie_xml = String::new();
    let mut tied_notation = String::new();
    if tie.stop {
        tie_xml += "<tie type=\"stop\"/>";
        tied_notation += "<tied type=\"stop\"/>";
    }
    if tie.start {
        tie_xml += "<tie type=\"start\"/>";
        tied_notation += "<tied type=\"start\"/>";
    }
    let notations = if tied_notation.is_empty() {
        String::new()
    } else {
        format!("<notations>{tied_notation}</notations>")
    };
    format!(
        "\n      <note>\n        {}<pitch><step>{}</step>{}<octave>{}</octave></pitch>\n        <duration>{}</duration>{}\n        {}<type>{}</type>{}{}{}{}\n      </note>",
        chord,
        s.step,
        alter,
        s.octave,
        note.duration.units(),
        tie_xml,
        voice_xml,
        note.duration.music_xml_type(),
        dot,
        accidental,
        staff,
        notations
    )
}
