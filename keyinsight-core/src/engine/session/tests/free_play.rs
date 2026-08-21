//! Free-play recording: note-offs close the take's entries and the take
//! replays at its played timing through the SMF path.

use std::rc::Rc;

use super::{engine, note_on, test_clock, RecordingAudio, NOW};
use crate::audio::parse_smf;
use crate::core::{NoteEvent, NoteEventKind};
use crate::engine::default_backend_factory;
use crate::engine::session::{FreePlayRecordedNote, SessionEngine};
use crate::persistence::AppDatabase;

fn note_off(midi: u8, at: f64) -> NoteEvent {
    NoteEvent {
        kind: NoteEventKind::Off,
        midi,
        velocity: None,
        timestamp: at,
        confidence: 1.0,
    }
}

#[test]
fn free_play_note_off_closes_the_recorded_note() {
    let (mut engine, time) = engine();
    engine.enter_free_play();
    let at = *time.borrow();
    engine.handle(note_on(60, at));
    engine.handle(note_off(60, at + 0.3));
    engine.handle(note_on(64, at + 1.0));
    // A stray note-off for a key that isn't held changes nothing.
    engine.handle(note_off(67, at + 1.1));
    let recording = &engine.free_play_recording;
    assert_eq!(recording.len(), 2);
    assert_eq!(recording[0].midi, 60);
    assert!((recording[0].start - 0.0).abs() < 1e-9);
    assert!((recording[0].end.expect("closed by the note-off") - 0.3).abs() < 1e-9);
    assert_eq!(
        recording[1],
        FreePlayRecordedNote { midi: 64, start: 1.0, end: None }
    );
    assert_eq!(engine.free_play_count(), 2, "note-offs are not notes");

    engine.clear_free_play();
    assert!(engine.free_play_recording.is_empty());
    assert_eq!(engine.free_play_record_start, None);
    // A new take restarts the clock from its first note.
    engine.handle(note_on(62, at + 5.0));
    assert_eq!(engine.free_play_recording[0].start, 0.0);
}

fn recording_engine() -> (SessionEngine, Rc<RecordingAudio>, Rc<std::cell::RefCell<f64>>) {
    let (time, clock) = test_clock();
    let audio = Rc::new(RecordingAudio::default());
    let mut engine = SessionEngine::new(
        Some(AppDatabase::in_memory(NOW)),
        Rc::clone(&audio) as Rc<dyn crate::audio::AudioOut>,
        clock,
        default_backend_factory(),
        42,
    );
    engine.start();
    (engine, audio, time)
}

#[test]
fn free_play_playback_replays_the_take_at_its_timing() {
    let (mut engine, audio, time) = recording_engine();
    engine.enter_free_play();
    let at = *time.borrow();
    engine.handle(note_on(60, at));
    engine.handle(note_off(60, at + 0.3));
    engine.handle(note_on(64, at + 0.5)); // still held: default length

    engine.toggle_free_play_playback();
    assert!(engine.is_playing_back());
    let smfs = audio.smfs.borrow();
    let notes = parse_smf(smfs.last().expect("playback started an SMF")).expect("valid SMF");
    drop(smfs);
    let mut played: Vec<(u8, f64, f64)> = notes
        .iter()
        .map(|n| (n.midi, n.start_seconds, n.duration_seconds))
        .collect();
    played.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(played.len(), 2, "every recorded note plays back");
    assert_eq!(played[0].0, 60);
    assert!((played[0].1 - 0.0).abs() < 1e-6);
    // SMF ticks quantize to ~2 ms (480 per second).
    assert!((played[0].2 - 0.3).abs() < 5e-3);
    assert_eq!(played[1].0, 64);
    assert!((played[1].1 - 0.5).abs() < 5e-3);
    assert!((played[1].2 - 0.5).abs() < 5e-3, "held notes get the 0.5 s default");

    // The keyboard strip shows the sounding take.
    *time.borrow_mut() += 0.1;
    assert_eq!(engine.playback_sounding_midis(), vec![60]);

    // Playback ends after the take's duration (1.0 s) + 0.25.
    *time.borrow_mut() += 1.1; // elapsed 1.2
    engine.tick();
    assert!(engine.is_playing_back(), "still within duration + 0.25");
    *time.borrow_mut() += 0.1; // elapsed 1.3
    engine.tick();
    assert!(!engine.is_playing_back(), "completion after duration + 0.25");
}

#[test]
fn free_play_playback_needs_a_take_and_stops_on_toggle() {
    let (mut engine, audio, time) = recording_engine();
    engine.enter_free_play();
    engine.toggle_free_play_playback();
    assert!(!engine.is_playing_back(), "nothing recorded yet");
    assert!(audio.smfs.borrow().is_empty());

    let at = *time.borrow();
    engine.handle(note_on(60, at));
    engine.toggle_free_play_playback();
    assert!(engine.is_playing_back());
    engine.toggle_free_play_playback();
    assert!(!engine.is_playing_back());

    // Leaving free play also stops an in-flight replay.
    engine.toggle_free_play_playback();
    assert!(engine.is_playing_back());
    engine.exit_free_play();
    assert!(!engine.is_playing_back());
    assert!(!engine.is_free_play());
}
