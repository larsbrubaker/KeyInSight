//! Hand modes, the active pacing, the octave scaffold, the display sleep
//! guard, and the progress report additions.

use std::cell::RefCell;
use std::rc::Rc;

use super::{engine, note_on, play_through, test_clock, NOW};
use crate::audio::NullAudioOut;
use crate::engine::default_backend_factory;
use crate::engine::session::{HandMode, PacingMode, Phase, SessionEngine};
use crate::persistence::AppDatabase;
use crate::score::{Exercise, NoteDuration, ScoreNote, Staff};

/// One 4/4 measure of C4 over C3 — every event a two-pitch set.
fn chord_exercise_json() -> String {
    let treble: Vec<ScoreNote> = (0..4)
        .map(|_| ScoreNote::note(60, NoteDuration::Quarter))
        .collect();
    let bass: Vec<ScoreNote> = (0..4)
        .map(|_| ScoreNote::note(48, NoteDuration::Quarter).with_staff(Staff::Bass))
        .collect();
    serde_json::to_string(&Exercise::new(treble, 4).with_bass(bass)).unwrap()
}

/// Tempo mode survives content the tempo matcher can't score: the
/// exercise runs self-paced while the user's choice stays Tempo.
#[test]
fn active_pacing_does_not_clobber_mode() {
    let (mut engine, _time) = engine();
    engine.set_mode(PacingMode::Tempo);
    assert_eq!(engine.active_pacing(), PacingMode::Tempo);

    engine.practice_exercise(&chord_exercise_json());
    assert_eq!(*engine.phase(), Phase::Playing);
    assert!(!engine.content_supports_tempo());
    assert_eq!(engine.active_pacing(), PacingMode::SelfPaced);
    assert_eq!(engine.mode(), PacingMode::Tempo, "the user's choice is untouched");
    assert!(engine.count_in_remaining().is_none(), "no count-in: self-paced run");
    assert!(engine.can_playback(), "Hear It is available off the tempo clock");

    // The next monophonic exercise tempo-scores again.
    engine.next_exercise();
    assert!(engine.content_supports_tempo());
    assert_eq!(engine.active_pacing(), PacingMode::Tempo);
}

/// Pre-hand-mode profiles kept their "Two hands" toggle choice.
#[test]
fn hand_mode_migrates_from_two_handed_setting() {
    let (_, clock) = test_clock();
    let mut db = AppDatabase::in_memory(NOW);
    db.set_setting("two_handed", "1", NOW);
    let mut engine = SessionEngine::new(
        Some(db),
        Rc::new(NullAudioOut),
        Rc::clone(&clock),
        default_backend_factory(),
        42,
    );
    engine.start();
    assert_eq!(engine.hand_mode(), HandMode::Both);
    assert!(engine.exercise().unwrap().is_two_voice());
    assert!(engine.exercise_info().unwrap().ends_with("· two hands"));

    // An explicit hand_mode wins over the legacy toggle.
    let mut db = AppDatabase::in_memory(NOW);
    db.set_setting("two_handed", "1", NOW);
    db.set_setting("hand_mode", "Left", NOW);
    let mut engine = SessionEngine::new(
        Some(db),
        Rc::new(NullAudioOut),
        clock,
        default_backend_factory(),
        42,
    );
    engine.start();
    assert_eq!(engine.hand_mode(), HandMode::Left);
}

#[test]
fn left_hand_mode_yields_bass_only_exercises() {
    let (mut engine, time) = engine();
    assert_eq!(engine.hand_mode(), HandMode::Right);
    engine.set_hand_mode(HandMode::Left);
    assert_eq!(engine.hand_mode(), HandMode::Left);
    let exercise = engine.exercise().expect("regenerated");
    assert!(exercise.is_bass_only());
    assert!(engine.exercise_info().unwrap().ends_with("· left hand"));
    // Bass seed range C3–G3.
    let expected = engine.current_expected_midi().unwrap();
    assert!((48..=55).contains(&expected), "bass model pitch, got {expected}");
    assert_eq!(
        engine.db.as_ref().unwrap().setting("hand_mode").as_deref(),
        Some("Left")
    );
    // Monophonic bass lines tempo-score too.
    assert!(engine.content_supports_tempo());

    // A clean left-hand exercise records bass-staff items.
    play_through(&mut engine, &time);
    let stats = engine.db.as_ref().unwrap().item_stats();
    assert!(stats.iter().any(|s| s.item.starts_with("bass:")), "{stats:?}");
    assert!(stats.iter().all(|s| !s.item.starts_with("treble:")), "{stats:?}");
}

#[test]
fn both_hands_mode_is_two_voice_and_persists() {
    let (mut engine, _time) = engine();
    engine.set_hand_mode(HandMode::Both);
    assert!(engine.exercise().unwrap().is_two_voice());
    assert!(engine.exercise_info().unwrap().ends_with("· two hands"));
    let snapshot = engine.db_document().unwrap();
    let (_, clock) = test_clock();
    let mut reloaded = SessionEngine::new(
        Some(AppDatabase::open(
            Box::new(crate::persistence::MemoryStorage::with_contents(snapshot)),
            NOW + 1000,
        )),
        Rc::new(NullAudioOut),
        clock,
        default_backend_factory(),
        42,
    );
    reloaded.start();
    assert_eq!(reloaded.hand_mode(), HandMode::Both);
}

/// With the scaffold off, the written octave is required.
#[test]
fn follow_octave_off_requires_the_written_octave() {
    let (mut engine, time) = engine();
    assert!(engine.follow_octave());
    engine.set_follow_octave(false);
    assert!(!engine.follow_octave());
    assert_eq!(
        engine.db.as_ref().unwrap().setting("follow_octave").as_deref(),
        Some("0")
    );
    let expected = engine.current_expected_midi().unwrap();
    *time.borrow_mut() += 0.2;
    let at = *time.borrow();
    engine.handle(note_on(expected - 12, at));
    assert_eq!(engine.errors_this_exercise(), 1);
    assert_eq!(engine.anchored_octaves(), 0);
}

/// The display sleep guard follows `phase`: awake while playing, released
/// at the summary — and only told about changes.
#[test]
fn display_awake_follows_the_phase() {
    let (time, clock) = test_clock();
    let mut engine = SessionEngine::new(
        Some(AppDatabase::in_memory(NOW)),
        Rc::new(NullAudioOut),
        clock,
        default_backend_factory(),
        42,
    );
    let calls: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&calls);
    engine.display_awake = Some(Box::new(move |active| sink.borrow_mut().push(active)));
    engine.start();
    assert_eq!(*calls.borrow(), vec![true]);
    play_through(&mut engine, &time);
    assert_eq!(*calls.borrow(), vec![true, false]);
    engine.next_exercise();
    assert_eq!(*calls.borrow(), vec![true, false, true]);
}

#[test]
fn chord_shape_entries_walk_the_ladder() {
    let (engine, _) = engine();
    let entries = engine.chord_shape_entries();
    assert_eq!(entries.len(), crate::skill::CHORD_SHAPE_LADDER.len());
    assert_eq!(entries[0].name, "chord:harm-5th");
    assert_eq!(entries[0].label, "harmonic 5ths");
    assert_eq!(entries[0].status, "probing", "the next locked shape probes");
    assert!(entries[1..].iter().all(|e| e.status == "locked"));
    assert!(entries.iter().all(|e| e.attempts == 0 && e.error_percent.is_none()));
}

#[test]
fn trouble_transitions_report_weak_specific_moves() {
    let (mut engine, _) = engine();
    assert!(engine.trouble_transitions(8).is_empty());
    {
        let db = engine.db.as_mut().unwrap();
        // Four attempts is the floor; all errors makes it a trouble spot.
        for _ in 0..4 {
            db.record_item_attempt("move:F#4>B4", true, None, NOW);
        }
        // Too few attempts to count.
        db.record_item_attempt("move:C4>D4", true, None, NOW);
        // Enough attempts, but clean.
        for _ in 0..4 {
            db.record_item_attempt("move:E4>G4", false, Some(300.0), NOW);
        }
    }
    let entries = engine.trouble_transitions(8);
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].label, "F#4 → B4");
    assert_eq!(entries[0].attempts, 4);
    assert!(entries[0].error_percent > 15);
    assert!(engine.trouble_transitions(0).is_empty());
}

#[test]
fn progress_entries_per_staff() {
    let (mut engine, _) = engine();
    let treble = engine.progress_entries(Staff::Treble);
    let bass = engine.progress_entries(Staff::Bass);
    assert_eq!(treble.len(), 20);
    assert_eq!(bass.len(), 20);
    assert!(treble.iter().any(|e| e.midi == 60 && e.unlocked));
    assert!(bass.iter().any(|e| e.midi == 48 && e.unlocked));
    assert!(bass.iter().all(|e| e.midi <= 60));
    // Ascending pitch order on both.
    assert!(bass.windows(2).all(|w| w[0].midi < w[1].midi));
    // The heat staff renders one note per entry on either clef, and the
    // seed items carry their heat state.
    let renderer = Rc::new(RefCell::new(crate::notation::NotationRenderer::new()));
    let mut controller = crate::notation::NotationController::new(Rc::clone(&renderer));
    engine.render_progress_staff(&mut controller, Staff::Bass);
    let staff_exercise = Exercise::new(Vec::new(), 4).with_bass(
        bass.iter()
            .map(|e| ScoreNote::note(e.midi, NoteDuration::Quarter).with_staff(Staff::Bass))
            .collect(),
    );
    let rendered = controller
        .render(&crate::score::MusicXmlEncoder::encode(&staff_exercise))
        .expect("bass heat staff engraves");
    assert_eq!(rendered.note_ids.len(), 20);
    let seed_index = bass.iter().position(|e| e.midi == 48).unwrap();
    assert_eq!(
        controller.state_of(&rendered.note_ids[seed_index]),
        Some(crate::notation::NoteState::Learning)
    );
}
