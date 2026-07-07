//! End-to-end session tests. The Swift app validated the engine through
//! the scripted `--demo` DemoDriver rather than a unit suite; these tests
//! cover the same journeys headlessly: play an adaptive exercise through
//! the simulated backend, see feedback + summary + persistence.

use std::cell::RefCell;
use std::rc::Rc;

use crate::audio::NullAudioOut;
use crate::core::{NoteEvent, NoteEventKind};
use crate::engine::session::{InputSource, Phase, SessionEngine};
use crate::engine::default_backend_factory;
use crate::persistence::AppDatabase;

const NOW: i64 = 1_700_000_000_000;

/// A manually advanced clock shared with the engine.
fn test_clock() -> (Rc<RefCell<f64>>, Rc<dyn Fn() -> f64>) {
    let time = Rc::new(RefCell::new(1000.0));
    let reader = Rc::clone(&time);
    (time, Rc::new(move || *reader.borrow()))
}

fn engine() -> (SessionEngine, Rc<RefCell<f64>>) {
    let (time, clock) = test_clock();
    let mut engine = SessionEngine::new(
        Some(AppDatabase::in_memory(NOW)),
        Rc::new(NullAudioOut),
        clock,
        default_backend_factory(),
        42,
    );
    engine.start();
    (engine, time)
}

fn note_on(midi: u8, at: f64) -> NoteEvent {
    NoteEvent {
        kind: NoteEventKind::On,
        midi,
        velocity: Some(80),
        timestamp: at,
        confidence: 1.0,
    }
}

#[test]
fn starts_into_a_playing_exercise() {
    let (engine, _time) = engine();
    assert_eq!(*engine.phase(), Phase::Playing);
    assert!(engine.note_count() > 0);
    assert!(engine.exercise_info().is_some());
    // The seed set is C4–G4: the first expected pitch is in range.
    let expected = engine.current_expected_midi().unwrap();
    assert!((60..=67).contains(&expected));
}

#[test]
fn playing_every_expected_note_completes_with_a_clean_summary() {
    let (mut engine, time) = engine();
    let mut guard = 0;
    while *engine.phase() == Phase::Playing {
        let expected = engine
            .current_expected_midi()
            .expect("single-note exercises expose the expected pitch");
        *time.borrow_mut() += 0.5;
        let at = *time.borrow();
        engine.handle(note_on(expected, at));
        guard += 1;
        assert!(guard < 100, "exercise should complete");
    }
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected a summary, got {:?}", engine.phase());
    };
    assert_eq!(summary.error_count, 0);
    assert_eq!(summary.first_try_correct, summary.note_count);
    assert_eq!(summary.accuracy_percent(), 100);
    assert!(summary.mean_latency_ms.is_some());
    assert_eq!(engine.exercises_completed(), 1);
    // The attempt log reached persistence.
    assert!(!engine.recent_exercises(5).is_empty());
}

#[test]
fn wrong_notes_mark_errors_and_reset_streak() {
    let (mut engine, time) = engine();
    let expected = engine.current_expected_midi().unwrap();
    let wrong = if expected == 60 { 62 } else { 60 };
    *time.borrow_mut() += 0.2;
    let at = *time.borrow();
    engine.handle(note_on(wrong, at));
    assert_eq!(engine.errors_this_exercise(), 1);
    assert_eq!(engine.streak(), 0);
    // The right note still advances.
    engine.handle(note_on(expected, at + 0.2));
    assert!(engine.current_note_index() >= 1 || *engine.phase() != Phase::Playing);
}

#[test]
fn octave_anchor_follows_the_players_octave() {
    let (mut engine, time) = engine();
    let expected = engine.current_expected_midi().unwrap();
    *time.borrow_mut() += 0.2;
    let at = *time.borrow();
    // Play an octave below: anchors, matches, and reports the offset.
    engine.handle(note_on(expected - 12, at));
    assert_eq!(engine.anchored_octaves(), -1);
    assert_eq!(engine.errors_this_exercise(), 0);
}

#[test]
fn free_play_mirrors_played_chords() {
    let (mut engine, time) = engine();
    engine.enter_free_play();
    assert!(engine.is_free_play());
    let at = *time.borrow();
    engine.handle(note_on(60, at));
    engine.handle(note_on(64, at + 0.01)); // within the chord window
    engine.handle(note_on(67, at + 0.5));
    assert_eq!(engine.free_play_count(), 3);
    assert_eq!(engine.last_free_play_note(), Some("G4"));
    engine.clear_free_play();
    assert_eq!(engine.free_play_count(), 0);
    engine.exit_free_play();
    assert!(!engine.is_free_play());
}

#[test]
fn drill_chains_cards_into_one_aggregated_summary() {
    let (mut engine, time) = engine();
    engine.start_drill();
    assert_eq!(engine.drill_remaining(), Some(super::DRILL_LENGTH));
    let mut guard = 0;
    while *engine.phase() == Phase::Playing {
        let expected = engine.current_expected_midi().unwrap();
        *time.borrow_mut() += 0.4;
        let at = *time.borrow();
        engine.handle(note_on(expected, at));
        guard += 1;
        assert!(guard < 200, "drill should aggregate to a summary");
    }
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected drill summary");
    };
    assert!(summary.drill);
    assert_eq!(summary.note_count, super::DRILL_LENGTH as usize);
    assert_eq!(engine.drill_remaining(), None);
}

#[test]
fn repertoire_records_piece_plays() {
    let (mut engine, time) = engine();
    let piece = crate::score::RepertoireLibrary::bundled()
        .into_iter()
        .find(|p| p.slug == "twinkle-twinkle")
        .unwrap();
    engine.start_piece(piece);
    assert!(engine.is_diverted());
    let mut guard = 0;
    while *engine.phase() == Phase::Playing {
        let Some(expected) = engine.current_expected_midi() else {
            break;
        };
        *time.borrow_mut() += 0.3;
        let at = *time.borrow();
        engine.handle(note_on(expected, at));
        guard += 1;
        assert!(guard < 200, "piece should complete");
    }
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected piece summary");
    };
    assert_eq!(summary.piece_title.as_deref(), Some("Twinkle, Twinkle, Little Star"));
    assert_eq!(engine.piece_stats("twinkle-twinkle").map(|s| s.0), Some(1));
}

#[test]
fn unplugged_self_grading_completes_and_records() {
    let (mut engine, _time) = engine();
    engine.set_input_source(InputSource::SelfVerify);
    assert_eq!(*engine.phase(), Phase::Playing);
    // First pass failed, second nailed.
    engine.self_verify_grade(false);
    assert_eq!(engine.self_verify_attempts(), 1);
    assert_eq!(*engine.phase(), Phase::Playing);
    engine.self_verify_grade(true);
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected summary after Nailed It");
    };
    assert!(summary.self_verified);
    assert_eq!(summary.error_count, 1);
}

#[test]
fn tempo_mode_counts_in_sweeps_misses_and_reports() {
    let (mut engine, time) = engine();
    // Content must be monophonic for tempo — the generated seed exercise is.
    assert!(engine.content_supports_tempo());
    engine.set_mode(crate::engine::PacingMode::Tempo);
    assert_eq!(engine.mode(), crate::engine::PacingMode::Tempo);
    assert!(engine.count_in_remaining().is_some());
    // Never play anything; advance time far past the exercise end and let
    // the sweep mark everything missed (60 BPM default: generously 60 s).
    for _ in 0..600 {
        *time.borrow_mut() += 0.1;
        engine.tick();
        if *engine.phase() != Phase::Playing {
            break;
        }
    }
    // The deferred TempoFinish needs one more tick past its deadline.
    *time.borrow_mut() += 1.0;
    engine.tick();
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected tempo summary, got {:?}", engine.phase());
    };
    let timing = summary.timing.as_ref().expect("tempo summary carries timing");
    assert_eq!(timing.missed, timing.expected_count);
    assert_eq!(summary.bpm, Some(60.0));
}

#[test]
fn user_switch_isolates_progress() {
    let (mut engine, time) = engine();
    // Complete one exercise as Player 1.
    let mut guard = 0;
    while *engine.phase() == Phase::Playing {
        let expected = engine.current_expected_midi().unwrap();
        *time.borrow_mut() += 0.4;
        let at = *time.borrow();
        engine.handle(note_on(expected, at));
        guard += 1;
        assert!(guard < 100);
    }
    assert_eq!(engine.exercises_completed(), 1);

    engine.add_user("Kid");
    assert_eq!(engine.current_user().map(|u| u.name.as_str()), Some("Kid"));
    assert_eq!(engine.exercises_completed(), 0);
    assert!(engine.recent_exercises(5).is_empty());
}

/// CalibrationSheet flow: with `calibration_tap` installed and the
/// exercise clock stopped, simulated piano keys must still reach the tap
/// callback (the sheet passes keys through to the training root).
#[test]
fn calibration_tap_receives_simulated_keys() {
    let (mut engine, time) = engine();
    engine.prepare_for_calibration();
    let taps: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&taps);
    engine.calibration_tap = Some(Box::new(move |timestamp| {
        sink.borrow_mut().push(timestamp);
    }));
    let now = *time.borrow();
    engine.metronome.start(90.0, 4, now + 0.35, now);

    *time.borrow_mut() += 1.0;
    assert!(engine.handle_simulated_key('a', true, false));
    engine.handle_simulated_key('a', false, false);
    assert_eq!(taps.borrow().len(), 1, "note-on must reach the tap hook");
}

/// Audio stub that accepts playback (NullAudioOut declines, which would
/// keep `toggle_playback` from ever starting).
struct AcceptingAudio;
impl crate::audio::AudioOut for AcceptingAudio {
    fn play_click(&self, _at: f64, _accent: bool) {}
    fn play_smf(&self, _smf: &[u8]) -> bool {
        true
    }
    fn stop_smf(&self) {}
}

/// During Hear It the keyboard strip shows the sounding keys: the first
/// expected pitch sounds at t=0 and stops by the end of its value.
#[test]
fn playback_reports_sounding_midis() {
    let (time, clock) = test_clock();
    let mut engine = SessionEngine::new(
        Some(AppDatabase::in_memory(NOW)),
        Rc::new(AcceptingAudio),
        clock,
        default_backend_factory(),
        42,
    );
    engine.start();
    assert!(engine.playback_sounding_midis().is_empty(), "silent before playback");

    let first = engine.current_expected_midi().unwrap();
    engine.toggle_playback();
    assert!(engine.is_playing_back());
    *time.borrow_mut() += 0.05; // just after the first onset
    assert!(
        engine.playback_sounding_midis().contains(&first),
        "first note sounds at playback start"
    );

    engine.toggle_playback(); // stop
    assert!(engine.playback_sounding_midis().is_empty(), "silent after stop");
}

/// Relaunching continues with the same user's open piece; leaving
/// repertoire clears the memory.
#[test]
fn active_piece_survives_reload() {
    use crate::persistence::MemoryStorage;
    use crate::score::RepertoireLibrary;

    let (_, clock) = test_clock();
    let mut engine = SessionEngine::new(
        Some(AppDatabase::in_memory(NOW)),
        Rc::new(NullAudioOut),
        Rc::clone(&clock),
        default_backend_factory(),
        42,
    );
    engine.start();
    let piece = RepertoireLibrary::bundled()
        .into_iter()
        .find(|p| p.slug == "gymnopedie-1")
        .unwrap();
    engine.start_piece(piece);
    let snapshot = engine.db_document().expect("db present");

    // Relaunch over the same stored document.
    let mut reloaded = SessionEngine::new(
        Some(AppDatabase::open(
            Box::new(MemoryStorage::with_contents(snapshot)),
            NOW + 60_000,
        )),
        Rc::new(NullAudioOut),
        Rc::clone(&clock),
        default_backend_factory(),
        42,
    );
    reloaded.start();
    assert_eq!(
        reloaded.active_piece().map(|p| p.slug.as_str()),
        Some("gymnopedie-1"),
        "the open piece reopens on launch"
    );

    // Leaving repertoire clears the persisted slug.
    reloaded.exit_repertoire();
    let snapshot = reloaded.db_document().unwrap();
    let mut fresh = SessionEngine::new(
        Some(AppDatabase::open(
            Box::new(MemoryStorage::with_contents(snapshot)),
            NOW + 120_000,
        )),
        Rc::new(NullAudioOut),
        Rc::clone(&clock),
        default_backend_factory(),
        42,
    );
    fresh.start();
    assert!(fresh.active_piece().is_none(), "training resumes after exit");
}

/// Fake platform mic: hands out queued sample chunks.
struct FakeMic {
    chunks: RefCell<std::collections::VecDeque<Vec<f32>>>,
}
impl crate::input::MicSource for FakeMic {
    fn start(&self) -> bool {
        true
    }
    fn stop(&self) {}
    fn sample_rate(&self) -> f64 {
        44_100.0
    }
    fn drain(&self, out: &mut Vec<f32>) {
        if let Some(chunk) = self.chunks.borrow_mut().pop_front() {
            out.extend_from_slice(&chunk);
        }
    }
}

/// The mic backend turns a played candidate into engine input: the
/// expected note advances the exercise like any other backend's events.
#[test]
fn mic_backend_detects_the_expected_note() {
    use crate::audio::goertzel::midi_frequency;
    use crate::input::{MicBackend, MicSource};

    // A steady piano-ish tone at the candidate pitch, chunked like a
    // capture callback delivers it.
    let f0 = midi_frequency(60);
    let tone: Vec<f32> = (0..44_100)
        .map(|i| {
            let t = i as f64 / 44_100.0;
            let mut v = 0.0;
            for (mult, gain) in [(1.0, 1.0), (2.0, 0.45), (3.0, 0.22)] {
                v += gain * (2.0 * std::f64::consts::PI * f0 * mult * t).sin();
            }
            (0.25 * v) as f32
        })
        .collect();
    let chunks: std::collections::VecDeque<Vec<f32>> =
        tone.chunks(2048).map(|c| c.to_vec()).collect();
    let mic = Rc::new(FakeMic {
        chunks: RefCell::new(chunks),
    });

    let mut backend = MicBackend::new(Rc::clone(&mic) as Rc<dyn MicSource>);
    let events: Rc<RefCell<Vec<NoteEvent>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&events);
    crate::core::InputBackend::set_on_event(
        &mut backend,
        Some(Box::new(move |event| sink.borrow_mut().push(event))),
    );
    crate::core::InputBackend::start(&mut backend);

    for frame in 0..20 {
        backend.process(frame as f64 * 0.02, &[60]);
    }
    assert!(backend.level() > 0.05, "meter shows input");
    let events = events.borrow();
    assert!(
        events
            .iter()
            .any(|e| e.kind == NoteEventKind::On && e.midi == 60 && e.confidence >= 1.0),
        "C4 note-on emitted: {events:?}"
    );
}
