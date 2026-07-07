//! Ports `Tests/KeyInSightTests/MicTests.swift` (YinPitchDetectorTests +
//! NoteGateTests) and `MIDIFileEncoderTests.swift`.

use crate::audio::{GateAction, MidiFileEncoder, NoteGate, YinPitchDetector};
use crate::core::{Rng64, SplitMix64};
use crate::score::{Exercise, NoteDuration, ScoreNote};

const SAMPLE_RATE: f64 = 44100.0;

fn sine(frequency: f64, count: usize, amplitude: f32) -> Vec<f32> {
    (0..count)
        .map(|i| {
            amplitude
                * (2.0 * std::f64::consts::PI * frequency * i as f64 / SAMPLE_RATE).sin() as f32
        })
        .collect()
}

/// Piano-ish tone: fundamental plus decaying harmonics.
fn tone(frequency: f64, count: usize) -> Vec<f32> {
    (0..count)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE;
            let value = (2.0 * std::f64::consts::PI * frequency * t).sin()
                + 0.5 * (2.0 * std::f64::consts::PI * 2.0 * frequency * t).sin()
                + 0.25 * (2.0 * std::f64::consts::PI * 3.0 * frequency * t).sin();
            (0.3 * value) as f32
        })
        .collect()
}

// --- YinPitchDetectorTests ---

#[test]
fn detects_a440() {
    let detector = YinPitchDetector::new(SAMPLE_RATE);
    let detection = detector.detect(&sine(440.0, 4096, 0.4)).unwrap();
    assert!((detection.frequency - 440.0).abs() < 2.0);
    assert_eq!(detection.midi(), Some(69));
    assert!(detection.confidence > 0.9);
}

#[test]
fn detects_middle_c_with_harmonics() {
    let detector = YinPitchDetector::new(SAMPLE_RATE);
    let detection = detector.detect(&tone(261.63, 4096));
    assert_eq!(detection.and_then(|d| d.midi()), Some(60));
}

#[test]
fn detects_low_and_high_piano_range() {
    let detector = YinPitchDetector::new(SAMPLE_RATE);
    assert_eq!(
        detector.detect(&tone(98.0, 4096)).and_then(|d| d.midi()),
        Some(43)
    ); // G2
    assert_eq!(
        detector
            .detect(&sine(880.0, 4096, 0.4))
            .and_then(|d| d.midi()),
        Some(81)
    ); // A5
}

#[test]
fn rejects_noise_and_silence() {
    let detector = YinPitchDetector::new(SAMPLE_RATE);
    let mut rng = SplitMix64::new(9);
    let noise: Vec<f32> = (0..4096)
        .map(|_| (rng.next_f64_below(0.8) - 0.4) as f32)
        .collect();
    if let Some(detection) = detector.detect(&noise) {
        assert!(detection.confidence < 0.6);
    }
    let silence = vec![0.0f32; 4096];
    assert_eq!(detector.detect(&silence), None);
}

// --- NoteGateTests ---

#[test]
fn two_consistent_frames_open_a_note() {
    let mut gate = NoteGate::default();
    assert_eq!(gate.process(Some(60), 0.9), GateAction::None);
    assert_eq!(
        gate.process(Some(60), 0.9),
        GateAction::On {
            midi: 60,
            confidence: 0.9
        }
    );
    // Sustained: no re-trigger.
    assert_eq!(gate.process(Some(60), 0.9), GateAction::None);
}

#[test]
fn flicker_does_not_trigger() {
    let mut gate = NoteGate::default();
    assert_eq!(gate.process(Some(60), 0.9), GateAction::None);
    assert_eq!(gate.process(Some(62), 0.9), GateAction::None);
    assert_eq!(gate.process(Some(64), 0.9), GateAction::None);
    assert_eq!(gate.active_midi(), None);
}

#[test]
fn two_quiet_frames_close_the_note() {
    let mut gate = NoteGate::default();
    let _ = gate.process(Some(60), 0.9);
    let _ = gate.process(Some(60), 0.9);
    assert_eq!(gate.process(None, 0.0), GateAction::None);
    assert_eq!(gate.process(None, 0.0), GateAction::Off { midi: 60 });
    assert_eq!(gate.active_midi(), None);
}

#[test]
fn low_confidence_counts_as_silence() {
    let mut gate = NoteGate::default();
    let _ = gate.process(Some(60), 0.9);
    let _ = gate.process(Some(60), 0.9);
    let _ = gate.process(Some(62), 0.3);
    assert_eq!(gate.process(Some(62), 0.3), GateAction::Off { midi: 60 });
}

#[test]
fn pitch_change_replaces_the_note() {
    let mut gate = NoteGate::default();
    let _ = gate.process(Some(60), 0.9);
    let _ = gate.process(Some(60), 0.9);
    assert_eq!(gate.process(Some(64), 0.8), GateAction::None);
    assert_eq!(
        gate.process(Some(64), 0.8),
        GateAction::Replace {
            off: 60,
            on: 64,
            confidence: 0.8
        }
    );
    assert_eq!(gate.active_midi(), Some(64));
}

// --- MIDIFileEncoderTests ---

#[test]
fn variable_length_quantities() {
    assert_eq!(MidiFileEncoder::variable_length(0), [0x00]);
    assert_eq!(MidiFileEncoder::variable_length(0x7F), [0x7F]);
    assert_eq!(MidiFileEncoder::variable_length(0x80), [0x81, 0x00]);
    assert_eq!(MidiFileEncoder::variable_length(240), [0x81, 0x70]);
    assert_eq!(MidiFileEncoder::variable_length(1766), [0x8D, 0x66]);
    assert_eq!(MidiFileEncoder::variable_length(0x4000), [0x81, 0x80, 0x00]);
}

#[test]
fn header_and_tempo_and_end_of_track() {
    let exercise = Exercise::new(vec![ScoreNote::note(60, NoteDuration::Whole)], 4);
    let bytes = MidiFileEncoder::encode(&exercise, 120.0, 0);

    // MThd, format 0, one track, 480 ticks per quarter.
    assert_eq!(
        &bytes[0..14],
        [0x4D, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 0, 0, 1, 0x01, 0xE0]
    );
    assert_eq!(&bytes[14..18], [0x4D, 0x54, 0x72, 0x6B]); // MTrk
    // Tempo meta first: 120 BPM = 500000 µs/quarter = 0x07A120.
    assert_eq!(&bytes[22..29], [0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
    // Ends with end-of-track.
    assert_eq!(&bytes[bytes.len() - 3..], [0xFF, 0x2F, 0x00]);
    // Declared track length matches actual.
    let declared = ((bytes[18] as usize) << 24)
        | ((bytes[19] as usize) << 16)
        | ((bytes[20] as usize) << 8)
        | bytes[21] as usize;
    assert_eq!(declared, bytes.len() - 22);
}

#[test]
fn note_events_and_rest_gaps() {
    // C4 quarter, quarter rest, D4 quarter → D4's note-on lands at tick 960
    // (240 ticks/unit × 4 units in).
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::rest(NoteDuration::Quarter),
            ScoreNote::note(62, NoteDuration::Quarter),
        ],
        4,
    );
    let bytes = MidiFileEncoder::encode(&exercise, 120.0, 0);

    // Walk the track: collect (absolute_tick, status, note) triples.
    let mut i = 29 + 3; // after tempo meta + program change
    let mut tick = 0usize;
    let mut note_events: Vec<(usize, u8, u8)> = Vec::new();
    while i < bytes.len() - 3 {
        let mut delta = 0usize;
        while bytes[i] & 0x80 != 0 {
            delta = (delta << 7) | (bytes[i] & 0x7F) as usize;
            i += 1;
        }
        delta = (delta << 7) | bytes[i] as usize;
        i += 1;
        tick += delta;
        let status = bytes[i];
        if status == 0x90 || status == 0x80 {
            note_events.push((tick, status, bytes[i + 1]));
            i += 3;
        } else {
            break;
        }
    }
    assert_eq!(note_events.len(), 4); // 2 notes × on+off
    assert_eq!(note_events[0], (0, 0x90, 60));
    assert_eq!(note_events[1].1, 0x80);
    assert_eq!(
        note_events[1].0,
        (480.0 * MidiFileEncoder::GATE_RATIO) as usize
    );
    assert_eq!(note_events[2], (960, 0x90, 62));
}

#[test]
fn durations_and_start_times() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::rest(NoteDuration::Quarter),
            ScoreNote::note(62, NoteDuration::Half),
        ],
        4,
    );
    // 8 units total at 60 BPM: 0.5 s per unit → 4 s.
    assert!((MidiFileEncoder::duration(&exercise, 60.0) - 4.0).abs() < 1e-9);
    let starts = MidiFileEncoder::sounded_note_start_seconds(&exercise, 60.0);
    assert_eq!(starts.len(), 2);
    assert!((starts[0] - 0.0).abs() < 1e-9);
    assert!((starts[1] - 2.0).abs() < 1e-9); // after quarter + rest
}

// --- synth (click buffers + SMF piano rendering) ---

/// The Swift `makeClick` constants: 30 ms at the sample rate, silent by
/// the end, accent louder than the beat click.
#[test]
fn click_buffers_match_metronome_constants() {
    let rate = 44_100.0;
    let click = crate::audio::click_samples(rate, false);
    let accent = crate::audio::click_samples(rate, true);
    assert_eq!(click.len(), (rate * 0.03) as usize);
    assert_eq!(accent.len(), click.len());
    let click_peak = click.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let accent_peak = accent.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(accent_peak > click_peak, "accent is louder");
    // Decayed to near silence by the end of the burst.
    assert!(click.last().unwrap().abs() < 0.01);
}

/// The parser reads back exactly what the encoder wrote: note count,
/// onsets on the eighth-note grid, and the gate-shortened durations.
#[test]
fn smf_parse_round_trips_the_encoder() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::note(64, NoteDuration::Eighth),
            ScoreNote::note(67, NoteDuration::Half),
        ],
        4,
    );
    let bpm = 90.0;
    let smf = MidiFileEncoder::encode(&exercise, bpm, 0);
    let notes = crate::audio::parse_smf(&smf).expect("encoder output parses");

    assert_eq!(notes.len(), 3);
    let unit = (60.0 / bpm) / 2.0; // eighth-note seconds
    let expected_starts = [0.0, 2.0 * unit, 3.0 * unit];
    for (note, (expected_start, midi, units)) in notes
        .iter()
        .zip([(0.0, 60u8, 2.0), (expected_starts[1], 64, 1.0), (expected_starts[2], 67, 4.0)])
    {
        let _ = expected_start;
        assert_eq!(note.midi, midi);
        let expected_duration = units * unit * MidiFileEncoder::GATE_RATIO;
        // Tick + tempo-microsecond quantization: within one MIDI tick.
        let tick = unit / MidiFileEncoder::TICKS_PER_UNIT as f64;
        assert!(
            (note.duration_seconds - expected_duration).abs() < tick,
            "gate-shortened duration for midi {midi}: {} vs {expected_duration}",
            note.duration_seconds
        );
    }
    assert!((notes[0].start_seconds - 0.0).abs() < 1e-9);
    assert!((notes[1].start_seconds - 2.0 * unit).abs() < 1e-6);
    assert!((notes[2].start_seconds - 3.0 * unit).abs() < 1e-6);
}

/// Rendering produces a clip covering the exercise plus the release
/// tail, actually contains sound, and stays inside the headroom.
#[test]
fn smf_renders_to_audible_bounded_pcm() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::note(64, NoteDuration::Quarter),
        ],
        4,
    );
    let bpm = 120.0;
    let smf = MidiFileEncoder::encode(&exercise, bpm, 0);
    let clip = crate::audio::render_smf(&smf, 44_100.0).expect("renders");

    let content_seconds = MidiFileEncoder::duration(&exercise, bpm);
    assert!(clip.duration_seconds() > content_seconds * MidiFileEncoder::GATE_RATIO);
    let peak = clip.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05, "clip is audible (peak {peak})");
    assert!(peak <= 0.9 + 1e-3, "clip stays inside headroom (peak {peak})");
}

/// Chords (shared onset) parse as overlapping notes rather than a
/// sequence.
#[test]
fn smf_parse_keeps_chords_simultaneous() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::note(64, NoteDuration::Quarter).with_chord(true),
        ],
        4,
    );
    let smf = MidiFileEncoder::encode(&exercise, 100.0, 0);
    let notes = crate::audio::parse_smf(&smf).expect("parses");
    assert_eq!(notes.len(), 2);
    assert!((notes[0].start_seconds - notes[1].start_seconds).abs() < 1e-9);
}

// --- Goertzel note detection (mic input DSP) ---

use crate::audio::goertzel::{Detected, GoertzelDetector, WINDOW_SAMPLES};
use crate::audio::{goertzel_power, midi_frequency};

const RATE: f64 = 44_100.0;

/// A piano-ish tone: fundamental plus decaying harmonics.
fn piano_tone(midis: &[u8], samples: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; samples];
    for &midi in midis {
        let f0 = midi_frequency(midi);
        for (i, slot) in out.iter_mut().enumerate() {
            let t = i as f64 / RATE;
            let body = (-t * 1.5).exp();
            let mut v = 0.0;
            for (mult, gain) in [(1.0, 1.0), (2.0, 0.45), (3.0, 0.22)] {
                v += gain * (2.0 * std::f64::consts::PI * f0 * mult * t).sin();
            }
            *slot += (0.25 * body * v) as f32;
        }
    }
    out
}

fn detect(signal: &[f32], candidates: &[u8]) -> Vec<Detected> {
    let mut detector = GoertzelDetector::new();
    let mut events = Vec::new();
    // Slide the window like the backend does (one hop per frame).
    let hop = 2048;
    let mut start = 0;
    while start + WINDOW_SAMPLES <= signal.len() {
        events.extend(detector.process(
            &signal[start..start + WINDOW_SAMPLES],
            RATE,
            candidates,
        ));
        start += hop;
    }
    events
}

#[test]
fn goertzel_power_peaks_at_the_target_frequency() {
    let signal: Vec<f32> = (0..4096)
        .map(|i| (2.0 * std::f64::consts::PI * 440.0 * i as f64 / RATE).sin() as f32)
        .collect();
    let at_target = goertzel_power(&signal, RATE, 440.0);
    let off_target = goertzel_power(&signal, RATE, 415.3); // G#4
    assert!(
        at_target > off_target * 50.0,
        "A4 bin {at_target} must dominate G#4 bin {off_target}"
    );
}

#[test]
fn detector_hears_the_played_candidate_only() {
    let signal = piano_tone(&[60], WINDOW_SAMPLES * 4);
    let events = detect(&signal, &[60, 64, 67]);
    assert!(events.contains(&Detected::On(60)), "C4 detected: {events:?}");
    assert!(!events.contains(&Detected::On(64)), "E4 silent: {events:?}");
    assert!(!events.contains(&Detected::On(67)), "G4 silent: {events:?}");
}

#[test]
fn detector_hears_every_note_of_a_chord() {
    let signal = piano_tone(&[60, 64, 67], WINDOW_SAMPLES * 4);
    let events = detect(&signal, &[60, 64, 67]);
    for midi in [60u8, 64, 67] {
        assert!(
            events.contains(&Detected::On(midi)),
            "chord tone {midi} detected: {events:?}"
        );
    }
}

#[test]
fn detector_rejects_the_octave_above() {
    // Playing C4: its 2nd harmonic sits exactly on C5 — the C5 candidate
    // must not fire.
    let signal = piano_tone(&[60], WINDOW_SAMPLES * 4);
    let events = detect(&signal, &[72]);
    assert!(
        !events.contains(&Detected::On(72)),
        "C5 candidate stays silent for a played C4: {events:?}"
    );
}

#[test]
fn detector_stays_silent_on_silence() {
    let signal = vec![0.0f32; WINDOW_SAMPLES * 4];
    assert!(detect(&signal, &[60, 64]).is_empty());
}

#[test]
fn unexplained_attack_reports_uncertain_not_wrong() {
    // Silence, then an F4 attack while listening for C4 only.
    let mut signal = vec![0.0f32; WINDOW_SAMPLES * 2];
    signal.extend(piano_tone(&[65], WINDOW_SAMPLES * 3));
    let events = detect(&signal, &[60]);
    assert!(
        events.contains(&Detected::Uncertain),
        "unexplained attack heard: {events:?}"
    );
    assert!(
        !events.contains(&Detected::On(60)),
        "C4 must not fire for a played F4: {events:?}"
    );
}
