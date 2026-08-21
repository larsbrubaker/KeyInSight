//! Encodes an Exercise as an in-memory Standard MIDI File (format 0) for
//! reference playback. Pure and deterministic — the timing math is
//! unit-tested here rather than trusted to ad-hoc timers.
//!
//! Ports `Audio/MIDIFileEncoder.swift`.

use crate::score::Exercise;

/// One captured free-play note, times in seconds from recording start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecordedNote {
    pub midi: u8,
    pub start_seconds: f64,
    pub duration_seconds: f64,
}

pub struct MidiFileEncoder;

impl MidiFileEncoder {
    pub const TICKS_PER_QUARTER: i32 = 480;
    /// Eighth-note units → ticks.
    pub const TICKS_PER_UNIT: i32 = Self::TICKS_PER_QUARTER / 2;
    /// Notes sound for this share of their value — slightly detached reads
    /// clearer than full legato for reference playback.
    pub const GATE_RATIO: f64 = 0.92;

    pub fn encode(exercise: &Exercise, bpm: f64, program: u8) -> Vec<u8> {
        // Span-based: chords share onsets and the bass voice has its own
        // timeline — a sequential walk would arpeggiate both.
        let mut events: Vec<(i32, [u8; 3])> = Vec::new();
        let mut tick = 0;
        for voice in [&exercise.notes, &exercise.bass_notes] {
            let mut voice_end = 0;
            for span in Exercise::voice_note_spans(voice) {
                let start = span.start_units * Self::TICKS_PER_UNIT;
                let length = span.length_units * Self::TICKS_PER_UNIT;
                events.push((start, [0x90, span.midi, 80]));
                events.push((
                    start + (length as f64 * Self::GATE_RATIO) as i32,
                    [0x80, span.midi, 64],
                ));
                voice_end = voice_end.max(start + length);
            }
            let voice_units: i32 = voice
                .iter()
                .filter(|n| !n.chord_with_previous)
                .map(|n| n.duration.units())
                .sum();
            tick = tick.max(voice_end.max(voice_units * Self::TICKS_PER_UNIT));
        }
        // Note-offs before note-ons at equal ticks (0x80 < 0x90).
        events.sort_by_key(|(tick, bytes)| (*tick, bytes[0]));

        let mut track: Vec<u8> = Vec::new();
        let microseconds_per_quarter = (60_000_000.0 / bpm) as u32;
        track.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03]);
        track.extend_from_slice(&[
            ((microseconds_per_quarter >> 16) & 0xFF) as u8,
            ((microseconds_per_quarter >> 8) & 0xFF) as u8,
            (microseconds_per_quarter & 0xFF) as u8,
        ]);
        track.extend_from_slice(&[0x00, 0xC0, program]);

        let mut last_tick = 0;
        for (event_tick, bytes) in &events {
            track.extend(Self::variable_length((event_tick - last_tick) as usize));
            track.extend_from_slice(bytes);
            last_tick = *event_tick;
        }
        track.extend(Self::variable_length((tick - last_tick).max(0) as usize));
        track.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        Self::wrap_track(track)
    }

    /// SMF format-0 container around a finished track chunk.
    fn wrap_track(track: Vec<u8>) -> Vec<u8> {
        let mut data: Vec<u8> = Vec::new();
        // MThd, fmt 0, 1 trk.
        data.extend_from_slice(&[0x4D, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 0, 0, 1]);
        data.extend_from_slice(&[
            (Self::TICKS_PER_QUARTER >> 8) as u8,
            (Self::TICKS_PER_QUARTER & 0xFF) as u8,
        ]);
        data.extend_from_slice(&[0x4D, 0x54, 0x72, 0x6B]); // MTrk
        let length = track.len() as u32;
        data.extend_from_slice(&[
            ((length >> 24) & 0xFF) as u8,
            ((length >> 16) & 0xFF) as u8,
            ((length >> 8) & 0xFF) as u8,
            (length & 0xFF) as u8,
        ]);
        data.extend(track);
        data
    }

    /// Encodes a free-play recording at its real timing (fixed 60 BPM
    /// grid: one quarter = one second, so ticks are just seconds × 480).
    pub fn encode_recording(recording: &[RecordedNote], program: u8) -> Vec<u8> {
        let mut events: Vec<(i32, [u8; 3])> = Vec::new();
        for note in recording {
            let start = (note.start_seconds * Self::TICKS_PER_QUARTER as f64) as i32;
            let end = start + ((note.duration_seconds * Self::TICKS_PER_QUARTER as f64) as i32).max(1);
            events.push((start, [0x90, note.midi, 80]));
            events.push((end, [0x80, note.midi, 64]));
        }
        // Note-offs before note-ons at equal ticks (0x80 < 0x90).
        events.sort_by_key(|(tick, bytes)| (*tick, bytes[0]));

        let mut track: Vec<u8> = Vec::new();
        track.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03]); // tempo: 60 BPM
        track.extend_from_slice(&[
            ((1_000_000u32 >> 16) & 0xFF) as u8,
            ((1_000_000u32 >> 8) & 0xFF) as u8,
            (1_000_000u32 & 0xFF) as u8,
        ]);
        track.extend_from_slice(&[0x00, 0xC0, program]);
        let mut last_tick = 0;
        for (event_tick, bytes) in &events {
            // A note captured before the take's zero (negative start) must
            // not wrap the delta: clamp like `encode`.
            track.extend(Self::variable_length((event_tick - last_tick).max(0) as usize));
            track.extend_from_slice(bytes);
            last_tick = *event_tick;
        }
        track.extend(Self::variable_length(0));
        track.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        Self::wrap_track(track)
    }

    /// Wall-clock length of a recording, seconds.
    pub fn recording_duration(recording: &[RecordedNote]) -> f64 {
        recording
            .iter()
            .map(|n| n.start_seconds + n.duration_seconds)
            .fold(0.0, f64::max)
    }

    /// Wall-clock length of the exercise at a tempo (longest voice wins),
    /// seconds.
    pub fn duration(exercise: &Exercise, bpm: f64) -> f64 {
        let units = [&exercise.notes, &exercise.bass_notes]
            .iter()
            .map(|voice| {
                voice
                    .iter()
                    .filter(|n| !n.chord_with_previous)
                    .map(|n| n.duration.units())
                    .sum::<i32>()
            })
            .max()
            .unwrap_or(0);
        units as f64 * (60.0 / bpm) / 2.0
    }

    /// Start time of each sounded treble note, seconds from playback start.
    pub fn sounded_note_start_seconds(exercise: &Exercise, bpm: f64) -> Vec<f64> {
        let unit_seconds = (60.0 / bpm) / 2.0;
        exercise
            .sounded_note_start_units()
            .iter()
            .map(|&u| u as f64 * unit_seconds)
            .collect()
    }

    /// Start time of each combined match event (follow cursor).
    pub fn event_start_seconds(exercise: &Exercise, bpm: f64) -> Vec<f64> {
        let unit_seconds = (60.0 / bpm) / 2.0;
        exercise
            .match_events()
            .iter()
            .map(|e| e.start_units as f64 * unit_seconds)
            .collect()
    }

    pub fn variable_length(value: usize) -> Vec<u8> {
        let mut buffer = vec![(value & 0x7F) as u8];
        let mut remaining = value >> 7;
        while remaining > 0 {
            buffer.push((remaining & 0x7F) as u8 | 0x80);
            remaining >>= 7;
        }
        buffer.reverse();
        buffer
    }
}
