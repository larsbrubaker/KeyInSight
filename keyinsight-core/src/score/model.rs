//! Internal score model: durations, notes/rests/chords/ties, and the
//! [`Exercise`] container with its measure chunking and combined
//! (grand-staff) event stream.
//!
//! Ports `Score/Score.swift`. Serde attributes mirror the Swift `Codable`
//! behavior, including the lenient decoding of pre-grand-staff stored specs
//! (missing `staff` / `chordWithPrevious` / `tiedFromPrevious` /
//! `bassNotes` / `fifths` keys default).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Durations measured in eighth-note units so every Phase 3 value (eighths,
/// dotted notes) is an exact integer. String serialization keeps stored
/// exercise specs readable and stable across refactors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteDuration {
    Eighth,
    Quarter,
    DottedQuarter,
    Half,
    DottedHalf,
    Whole,
}

impl NoteDuration {
    pub const ALL: [NoteDuration; 6] = [
        NoteDuration::Eighth,
        NoteDuration::Quarter,
        NoteDuration::DottedQuarter,
        NoteDuration::Half,
        NoteDuration::DottedHalf,
        NoteDuration::Whole,
    ];

    /// Length in eighth-note units.
    pub fn units(self) -> i32 {
        match self {
            NoteDuration::Eighth => 1,
            NoteDuration::Quarter => 2,
            NoteDuration::DottedQuarter => 3,
            NoteDuration::Half => 4,
            NoteDuration::DottedHalf => 6,
            NoteDuration::Whole => 8,
        }
    }

    pub fn is_dotted(self) -> bool {
        matches!(self, NoteDuration::DottedQuarter | NoteDuration::DottedHalf)
    }

    pub fn music_xml_type(self) -> &'static str {
        match self {
            NoteDuration::Eighth => "eighth",
            NoteDuration::Quarter | NoteDuration::DottedQuarter => "quarter",
            NoteDuration::Half | NoteDuration::DottedHalf => "half",
            NoteDuration::Whole => "whole",
        }
    }
}

/// Which staff of the grand staff a note lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Staff {
    Treble,
    Bass,
}

/// A note or (with `midi == None`) a rest. Chords follow the MusicXML
/// model: the first pitch is the anchor and each further member sets
/// `chord_with_previous` — so every chord member stays an individually
/// addressable sounded note (per-note ids, item stats, matcher sets).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScoreNote {
    pub midi: Option<u8>,
    pub duration: NoteDuration,
    #[serde(default = "default_staff", rename = "staff")]
    pub staff: Staff,
    /// Sounds together with the preceding note (a chord member, not an onset).
    #[serde(default, rename = "chordWithPrevious")]
    pub chord_with_previous: bool,
    /// Tie continuation: extends the previous event's same pitch — renders
    /// with a tie, adds playback length, but is NOT a new onset to play.
    #[serde(default, rename = "tiedFromPrevious")]
    pub tied_from_previous: bool,
}

fn default_staff() -> Staff {
    Staff::Treble
}

impl ScoreNote {
    pub fn new(midi: Option<u8>, duration: NoteDuration) -> Self {
        Self {
            midi,
            duration,
            staff: Staff::Treble,
            chord_with_previous: false,
            tied_from_previous: false,
        }
    }

    pub fn note(midi: u8, duration: NoteDuration) -> Self {
        Self::new(Some(midi), duration)
    }

    pub fn rest(duration: NoteDuration) -> Self {
        Self::new(None, duration)
    }

    pub fn with_staff(mut self, staff: Staff) -> Self {
        self.staff = staff;
        self
    }

    pub fn with_chord(mut self, chord_with_previous: bool) -> Self {
        self.chord_with_previous = chord_with_previous;
        self
    }

    pub fn with_tie(mut self, tied_from_previous: bool) -> Self {
        self.tied_from_previous = tied_from_previous;
        self
    }

    pub fn is_rest(&self) -> bool {
        self.midi.is_none()
    }
}

/// One matchable moment: every pitch sounding at an onset, across both
/// voices. `pitches`/`staves` are in notation document order — treble
/// voice first, then bass (pinned by the grand-staff tests).
#[derive(Debug, Clone, PartialEq)]
pub struct MatchEvent {
    pub start_units: i32,
    pub pitches: Vec<u8>,
    pub staves: Vec<Staff>,
    pub durations: Vec<NoteDuration>,
}

/// (start, length, midi, staff, duration) for one PLAYED note of a voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteSpan {
    pub start_units: i32,
    pub length_units: i32,
    pub midi: u8,
    pub staff: Staff,
    pub duration: NoteDuration,
}

/// Per-note tie role for the encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TieRole {
    pub start: bool,
    pub stop: bool,
}

/// Internal score model: a treble voice plus an optional independent bass
/// voice — real two-handed music, not just cross-staff chords.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exercise {
    /// Staff-1 voice (right hand). `staff` on these notes still routes
    /// individual noteheads (free-play cross-staff chords).
    pub notes: Vec<ScoreNote>,
    /// Staff-2 voice (left hand), independent rhythm. Empty = single voice.
    #[serde(default, rename = "bassNotes")]
    pub bass_notes: Vec<ScoreNote>,
    /// Quarter-note beats per measure (4 = 4/4).
    #[serde(rename = "beatsPerMeasure")]
    pub beats_per_measure: i32,
    /// Key signature: 0 = C, 1 = G, 2 = D.
    #[serde(default)]
    pub fifths: i32,
}

impl Exercise {
    pub fn new(notes: Vec<ScoreNote>, beats_per_measure: i32) -> Self {
        Self {
            notes,
            bass_notes: Vec::new(),
            beats_per_measure,
            fifths: 0,
        }
    }

    pub fn with_bass(mut self, bass_notes: Vec<ScoreNote>) -> Self {
        self.bass_notes = bass_notes;
        self
    }

    pub fn with_fifths(mut self, fifths: i32) -> Self {
        self.fifths = fifths;
        self
    }

    pub fn is_two_voice(&self) -> bool {
        !self.bass_notes.is_empty()
    }

    pub fn units_per_measure(&self) -> i32 {
        self.beats_per_measure * 2
    }

    /// Treble-voice notes chunked into measures by accumulated units. Chord
    /// members contribute no time and never split from their anchor.
    pub fn measures(&self) -> Vec<Vec<ScoreNote>> {
        Self::measures_of(&self.notes, self.units_per_measure())
    }

    /// Bass-voice measures (empty for single-voice scores).
    pub fn bass_measures(&self) -> Vec<Vec<ScoreNote>> {
        Self::measures_of(&self.bass_notes, self.units_per_measure())
    }

    /// Measure count across both voices.
    pub fn measure_count(&self) -> usize {
        self.measures().len().max(self.bass_measures().len())
    }

    pub fn measures_of(voice: &[ScoreNote], units_per_measure: i32) -> Vec<Vec<ScoreNote>> {
        let mut result: Vec<Vec<ScoreNote>> = Vec::new();
        let mut current: Vec<ScoreNote> = Vec::new();
        let mut units = 0;
        for (index, note) in voice.iter().enumerate() {
            current.push(*note);
            if !note.chord_with_previous {
                units += note.duration.units();
            }
            let next_is_chord_member =
                index + 1 < voice.len() && voice[index + 1].chord_with_previous;
            if units >= units_per_measure && !next_is_chord_member {
                result.push(std::mem::take(&mut current));
                units = 0;
            }
        }
        if !current.is_empty() {
            result.push(current);
        }
        result
    }

    /// The treble-voice notes that actually sound (rests excluded).
    pub fn sounded_notes(&self) -> Vec<ScoreNote> {
        self.notes.iter().copied().filter(|n| !n.is_rest()).collect()
    }

    /// Every sounded note across both voices (stats, self-verify grading).
    pub fn all_sounded_notes(&self) -> Vec<ScoreNote> {
        let mut all = self.sounded_notes();
        all.extend(self.bass_notes.iter().copied().filter(|n| !n.is_rest()));
        all
    }

    // --- Combined event stream (both voices) ---

    pub fn match_events(&self) -> Vec<MatchEvent> {
        let mut treble: BTreeMap<i32, Vec<(u8, Staff, NoteDuration)>> = BTreeMap::new();
        let mut bass: BTreeMap<i32, Vec<(u8, Staff, NoteDuration)>> = BTreeMap::new();
        for span in Self::voice_note_spans(&self.notes) {
            treble
                .entry(span.start_units)
                .or_default()
                .push((span.midi, span.staff, span.duration));
        }
        for span in Self::voice_note_spans(&self.bass_notes) {
            bass.entry(span.start_units)
                .or_default()
                .push((span.midi, Staff::Bass, span.duration));
        }
        let mut onsets: Vec<i32> = treble.keys().chain(bass.keys()).copied().collect();
        onsets.sort_unstable();
        onsets.dedup();
        onsets
            .into_iter()
            .map(|units| {
                let mut combined: Vec<(u8, Staff, NoteDuration)> = Vec::new();
                if let Some(t) = treble.get(&units) {
                    combined.extend_from_slice(t);
                }
                if let Some(b) = bass.get(&units) {
                    combined.extend_from_slice(b);
                }
                MatchEvent {
                    start_units: units,
                    pitches: combined.iter().map(|c| c.0).collect(),
                    staves: combined.iter().map(|c| c.1).collect(),
                    durations: combined.iter().map(|c| c.2).collect(),
                }
            })
            .collect()
    }

    /// What the self-paced matcher consumes: the expected pitch set per
    /// combined event.
    pub fn expected_sets(&self) -> Vec<HashSet<u8>> {
        self.match_events()
            .into_iter()
            .map(|e| e.pitches.into_iter().collect())
            .collect()
    }

    /// Measure index per match event (accuracy heatmaps).
    pub fn event_measure_indices(&self) -> Vec<usize> {
        let upm = self.units_per_measure();
        self.match_events()
            .iter()
            .map(|e| (e.start_units / upm) as usize)
            .collect()
    }

    /// Spans per PLAYED note of a voice, in document order. Chord members
    /// share their anchor's onset/length; tie continuations extend the
    /// previous event's matching span (longer playback, no new onset)
    /// instead of creating one.
    pub fn voice_note_spans(voice: &[ScoreNote]) -> Vec<NoteSpan> {
        let mut result: Vec<NoteSpan> = Vec::new();
        let mut units = 0;
        let mut anchor_start = 0;
        let mut anchor_length = 0;
        // midi → result index, for the previous and current event.
        let mut prev_event_spans: HashMap<u8, usize> = HashMap::new();
        let mut current_event_spans: HashMap<u8, usize> = HashMap::new();
        for note in voice {
            if !note.chord_with_previous {
                anchor_start = units;
                anchor_length = note.duration.units();
                prev_event_spans = std::mem::take(&mut current_event_spans);
            }
            let start = if note.chord_with_previous {
                anchor_start
            } else {
                units
            };
            let length = if note.chord_with_previous {
                anchor_length
            } else {
                note.duration.units()
            };
            if let Some(midi) = note.midi {
                if note.tied_from_previous {
                    if let Some(&index) = prev_event_spans.get(&midi) {
                        result[index].length_units += note.duration.units();
                        current_event_spans.insert(midi, index);
                    } else {
                        result.push(NoteSpan {
                            start_units: start,
                            length_units: length,
                            midi,
                            staff: note.staff,
                            duration: note.duration,
                        });
                        current_event_spans.insert(midi, result.len() - 1);
                    }
                } else {
                    result.push(NoteSpan {
                        start_units: start,
                        length_units: length,
                        midi,
                        staff: note.staff,
                        duration: note.duration,
                    });
                    current_event_spans.insert(midi, result.len() - 1);
                }
            }
            if !note.chord_with_previous {
                units += note.duration.units();
            }
        }
        result
    }

    /// Per-note tie roles for a voice (encoder): `start` when the next
    /// event continues this pitch, `stop` when this note continues the
    /// previous.
    pub fn tie_roles(voice: &[ScoreNote]) -> Vec<TieRole> {
        // Group into events (anchor + chord members).
        let mut events: Vec<Vec<usize>> = Vec::new();
        for (index, note) in voice.iter().enumerate() {
            if note.chord_with_previous && !events.is_empty() {
                events.last_mut().unwrap().push(index);
            } else {
                events.push(vec![index]);
            }
        }
        let mut roles: Vec<TieRole> = voice
            .iter()
            .map(|n| TieRole {
                start: false,
                stop: n.tied_from_previous,
            })
            .collect();
        for (e, indices) in events.iter().enumerate() {
            if e + 1 >= events.len() {
                continue;
            }
            let next_tied_pitches: HashSet<u8> = events[e + 1]
                .iter()
                .filter_map(|&i| {
                    if voice[i].tied_from_previous {
                        voice[i].midi
                    } else {
                        None
                    }
                })
                .collect();
            for &i in indices {
                if voice[i]
                    .midi
                    .map(|m| next_tied_pitches.contains(&m))
                    .unwrap_or(false)
                {
                    roles[i].start = true;
                }
            }
        }
        roles
    }

    /// Target onset in eighth-units from the exercise start, per sounded
    /// note (treble voice). Chord members share their anchor's onset.
    pub fn sounded_note_start_units(&self) -> Vec<i32> {
        let mut starts: Vec<i32> = Vec::new();
        let mut units = 0;
        for note in &self.notes {
            if note.chord_with_previous {
                if !note.is_rest() {
                    starts.push(starts.last().copied().unwrap_or(0));
                }
                continue;
            }
            if !note.is_rest() {
                starts.push(units);
            }
            units += note.duration.units();
        }
        starts
    }

    /// Measure index per sounded note (for per-measure accuracy heatmaps).
    pub fn sounded_note_measure_indices(&self) -> Vec<usize> {
        let upm = self.units_per_measure();
        self.sounded_note_start_units()
            .iter()
            .map(|&u| (u / upm) as usize)
            .collect()
    }
}