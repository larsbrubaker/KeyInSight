//! MusicXML import for the growing subset: single part, one or two staves
//! (grand staff with one voice per staff), chords, sharp keys up to 2
//! sharps, durations our score model knows (eighth … whole incl. dotted),
//! full measures. Anything else is rejected with a specific message — no
//! silent stripping.
//!
//! Ports `Score/MusicXMLImporter.swift`. Swift used Foundation's
//! `XMLDocument` DOM; here a minimal element tree is built with quick-xml
//! (see [`Element`]) so the traversal logic stays line-mappable.

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::score::{Exercise, NoteDuration, ScoreNote, Staff};

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedPiece {
    pub title: String,
    pub exercise: Exercise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    NotMusicXml,
    MultipleParts,
    Unsupported(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::NotMusicXml => write!(f, "Not a readable MusicXML file."),
            ImportError::MultipleParts => write!(
                f,
                "Multi-part scores aren't supported yet — single staff only."
            ),
            ImportError::Unsupported(what) => write!(f, "Unsupported for now: {what}."),
        }
    }
}

impl std::error::Error for ImportError {}

/// Minimal DOM node: name, attributes, children, text.
#[derive(Debug, Default)]
struct Element {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<Element>,
    text: String,
}

impl Element {
    fn elements<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    fn first(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn string_value(&self) -> String {
        // Concatenated descendant text, like Foundation's `stringValue`.
        let mut out = self.text.clone();
        for child in &self.children {
            out.push_str(&child.string_value());
        }
        out
    }
}

/// Parse bytes into the root element (DTD/doctype and comments skipped).
fn parse_document(data: &[u8]) -> Option<Element> {
    let mut reader = Reader::from_reader(data);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(start)) => {
                let mut element = Element {
                    name: String::from_utf8_lossy(start.name().as_ref()).into_owned(),
                    ..Default::default()
                };
                for attr in start.attributes().flatten() {
                    element.attributes.push((
                        String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
                        String::from_utf8_lossy(&attr.value).into_owned(),
                    ));
                }
                stack.push(element);
            }
            Ok(Event::Empty(start)) => {
                let mut element = Element {
                    name: String::from_utf8_lossy(start.name().as_ref()).into_owned(),
                    ..Default::default()
                };
                for attr in start.attributes().flatten() {
                    element.attributes.push((
                        String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
                        String::from_utf8_lossy(&attr.value).into_owned(),
                    ));
                }
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => root = Some(element),
                }
            }
            Ok(Event::End(_)) => {
                let element = stack.pop()?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => {
                        root = Some(element);
                        break;
                    }
                }
            }
            Ok(Event::Text(text)) => {
                if let Some(current) = stack.last_mut() {
                    if let Ok(unescaped) = text.unescape() {
                        current.text.push_str(&unescaped);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {} // comments, PIs, doctype
            Err(_) => return None,
        }
        buf.clear();
    }
    root
}

pub struct MusicXmlImporter;

impl MusicXmlImporter {
    pub fn parse(data: &[u8], fallback_title: &str) -> Result<ImportedPiece, ImportError> {
        let root = parse_document(data).ok_or(ImportError::NotMusicXml)?;
        if root.name != "score-partwise" {
            return Err(ImportError::NotMusicXml);
        }

        let parts: Vec<&Element> = root.elements("part").collect();
        if parts.is_empty() {
            return Err(ImportError::NotMusicXml);
        }
        if parts.len() > 1 {
            return Err(ImportError::MultipleParts);
        }
        let part = parts[0];

        let title = first_text(&root, &["work", "work-title"])
            .or_else(|| first_text(&root, &["movement-title"]))
            .unwrap_or_else(|| fallback_title.to_string());

        let mut divisions = 0;
        let mut fifths = 0;
        let mut beats_per_measure = 4;
        let mut staves = 1;
        let mut treble_notes: Vec<ScoreNote> = Vec::new();
        let mut bass_notes: Vec<ScoreNote> = Vec::new();
        let measures: Vec<&Element> = part.elements("measure").collect();
        if measures.is_empty() {
            return Err(ImportError::NotMusicXml);
        }

        for (measure_index, measure) in measures.iter().enumerate() {
            if let Some(attributes) = measure.first("attributes") {
                if let Some(d) = int_value(attributes, "divisions") {
                    divisions = d;
                }
                if let Some(s) = int_value(attributes, "staves") {
                    if !(1..=2).contains(&s) {
                        return Err(ImportError::Unsupported("more than two staves".into()));
                    }
                    staves = s;
                }
                if let Some(key) = attributes.first("key") {
                    if let Some(f) = int_value(key, "fifths") {
                        if !(-7..=7).contains(&f) {
                            return Err(ImportError::Unsupported(
                                "key signature out of range".into(),
                            ));
                        }
                        fifths = f;
                    }
                }
                if let Some(time) = attributes.first("time") {
                    if int_value(time, "beat-type") != Some(4) {
                        return Err(ImportError::Unsupported(
                            "time signatures not over 4".into(),
                        ));
                    }
                    beats_per_measure = int_value(time, "beats").unwrap_or(4);
                }
                for clef in attributes.elements("clef") {
                    let sign = clef.first("sign").map(|s| s.string_value());
                    let sign = sign.as_deref().map(str::trim);
                    if !(sign == Some("G") || (sign == Some("F") && staves == 2)) {
                        return Err(ImportError::Unsupported(
                            "clefs other than treble/bass".into(),
                        ));
                    }
                }
            }

            if staves == 1
                && (measure.first("backup").is_some() || measure.first("forward").is_some())
            {
                return Err(ImportError::Unsupported(
                    "multiple voices on one staff".into(),
                ));
            }
            if measure.first("forward").is_some() {
                return Err(ImportError::Unsupported(
                    "forward skips (partial voices)".into(),
                ));
            }

            let mut treble_units = 0;
            let mut bass_units = 0;
            for note_element in measure.elements("note") {
                let (note, staff_number) = parse_note(note_element, divisions)?;
                // Staff 2 (or an explicit staff element) routes to the bass
                // voice; one voice per staff.
                if staff_number == 2 {
                    bass_notes.push(ScoreNote {
                        midi: note.midi,
                        duration: note.duration,
                        staff: Staff::Bass,
                        chord_with_previous: note.chord_with_previous,
                        tied_from_previous: note.tied_from_previous,
                    });
                    if !note.chord_with_previous {
                        bass_units += note.duration.units();
                    }
                } else {
                    treble_notes.push(note);
                    if !note.chord_with_previous {
                        treble_units += note.duration.units();
                    }
                }
            }
            // Full measures per voice keep chunking (and thus barlines)
            // aligned with the source. A short final measure is fine.
            let expected_units = beats_per_measure * 2;
            for (units, name) in [(treble_units, "treble"), (bass_units, "bass")] {
                if (units > 0 || name == "treble")
                    && (units > expected_units
                        || (units < expected_units && measure_index < measures.len() - 1))
                {
                    return Err(ImportError::Unsupported(format!(
                        "incomplete {} measure {} (pickup measures)",
                        name,
                        measure_index + 1
                    )));
                }
            }
        }

        if !treble_notes.iter().any(|n| !n.is_rest()) {
            return Err(ImportError::Unsupported(
                "scores with no treble-staff notes".into(),
            ));
        }
        Ok(ImportedPiece {
            title,
            exercise: Exercise {
                notes: treble_notes,
                bass_notes,
                beats_per_measure,
                fifths,
            },
        })
    }
}

fn parse_note(element: &Element, divisions: i32) -> Result<(ScoreNote, i32), ImportError> {
    if divisions <= 0 {
        return Err(ImportError::Unsupported("missing divisions".into()));
    }
    if element.first("grace").is_some() {
        return Err(ImportError::Unsupported("grace notes".into()));
    }
    if element.first("time-modification").is_some() {
        return Err(ImportError::Unsupported("tuplets".into()));
    }
    let is_chord_member = element.first("chord").is_some();
    // A tie stop marks this note as a continuation of the previous event.
    let tied_from_previous = element
        .elements("tie")
        .any(|tie| tie.attribute("type") == Some("stop"));
    let staff_number = int_value(element, "staff").unwrap_or(1);

    let Some(raw_duration) = int_value(element, "duration") else {
        return Err(ImportError::Unsupported("notes without duration".into()));
    };
    let scaled = raw_duration * 2;
    if scaled % divisions != 0 {
        return Err(ImportError::Unsupported(
            "note values outside eighth–whole".into(),
        ));
    }
    let Some(duration) = NoteDuration::ALL
        .iter()
        .copied()
        .find(|d| d.units() == scaled / divisions)
    else {
        return Err(ImportError::Unsupported(
            "note values outside eighth–whole".into(),
        ));
    };

    if element.first("rest").is_some() {
        return Ok((ScoreNote::rest(duration), staff_number));
    }
    let pitch = element
        .first("pitch")
        .ok_or_else(|| ImportError::Unsupported("unreadable pitch".into()))?;
    let step = pitch
        .first("step")
        .map(|s| s.string_value().trim().to_string())
        .ok_or_else(|| ImportError::Unsupported("unreadable pitch".into()))?;
    let octave = int_value(pitch, "octave")
        .ok_or_else(|| ImportError::Unsupported("unreadable pitch".into()))?;
    let step_index = ["C", "D", "E", "F", "G", "A", "B"]
        .iter()
        .position(|&s| s == step)
        .ok_or_else(|| ImportError::Unsupported("unreadable pitch".into()))?;
    let alter = int_value(pitch, "alter").unwrap_or(0);
    if !(-1..=1).contains(&alter) {
        return Err(ImportError::Unsupported("double accidentals".into()));
    }
    let semitones = [0, 2, 4, 5, 7, 9, 11][step_index] + alter;
    let midi = (octave + 1) * 12 + semitones;
    if !(0..=127).contains(&midi) {
        return Err(ImportError::Unsupported("out-of-range pitch".into()));
    }
    Ok((
        ScoreNote {
            midi: Some(midi as u8),
            duration,
            staff: if staff_number == 2 {
                Staff::Bass
            } else {
                Staff::Treble
            },
            chord_with_previous: is_chord_member,
            tied_from_previous,
        },
        staff_number,
    ))
}

fn first_text(element: &Element, path: &[&str]) -> Option<String> {
    let mut current = element;
    for name in path {
        current = current.first(name)?;
    }
    let text = current.string_value().trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn int_value(element: &Element, name: &str) -> Option<i32> {
    element
        .first(name)
        .and_then(|e| e.string_value().trim().parse().ok())
}
