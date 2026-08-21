//! Survival mode (OQ-25): the scripted demo's survival act (DemoDriver
//! Act 8, driven through the ported driver) plus the unit coverage the
//! Swift suite never had — life budget, the seamless window swap, the
//! swap's guards, neutral chunk bias, and the Hear It lockout.

use std::rc::Rc;

use super::{engine, note_on, test_clock, AcceptingAudio, NOW};
use crate::engine::default_backend_factory;
use crate::engine::demo::{DemoDriver, SURVIVAL_TARGET_NOTES};
use crate::engine::session::{HandMode, PacingMode, Phase, SessionEngine};
use crate::engine::SurvivalPolicy;
use crate::notation::NoteState;
use crate::persistence::{AppDatabase, MemoryStorage};
use crate::score::Hands;

/// DemoDriver Act 8: the ported driver's `survivalStep` loop strikes every
/// member of the expected set per onset until the target, then injects
/// wrong notes until the run dies; its scripted clock ticks the engine so
/// the deferred window swap fires mid-run like the Swift dispatch timer.
#[test]
fn demo_survival_act() {
    let (mut engine, time) = engine();
    {
        let mut driver = DemoDriver::new(&mut engine, Rc::clone(&time));
        assert_eq!(driver.act8_survival(), Ok(()), "log: {:#?}", driver.log);
        assert!(driver
            .log
            .iter()
            .any(|line| line.starts_with("demo: survival started — ")));
        assert_eq!(
            driver.log.last().map(String::as_str),
            Some("demo: OK — unlock, tempo, repertoire, free play, drill, playback, self-verify, and survival all verified")
        );
    }
    let Phase::Summary(summary) = engine.phase().clone() else {
        panic!("expected the run summary, got {:?}", engine.phase());
    };
    let survival = summary
        .survival
        .as_ref()
        .expect("survival ended with a survival report");
    assert!(
        survival.notes >= SURVIVAL_TARGET_NOTES,
        "notes {}",
        survival.notes
    );
    assert!(survival.score > 0);
    assert!(survival.is_new_best);
    assert_eq!(engine.survival_best(), survival.score);
    assert_eq!(survival.best, survival.score);
    assert!(engine.survival_window_gen() >= 2);
    assert!(!engine.is_survival());
    assert_eq!(summary.note_count, survival.notes);
    assert_eq!(summary.first_try_correct, survival.notes);
    assert_eq!(summary.error_count, SurvivalPolicy::START_LIVES as usize);
    assert!(!summary.drill && !summary.self_verified);
    assert!(survival.notes_per_minute > 0.0);

    // The best persists per user and reloads.
    let snapshot = engine.db_document().expect("db present");
    let (_, clock) = test_clock();
    let mut reloaded = SessionEngine::new(
        Some(AppDatabase::open(
            Box::new(MemoryStorage::with_contents(snapshot)),
            NOW + 60_000,
        )),
        Rc::new(crate::audio::NullAudioOut),
        clock,
        default_backend_factory(),
        42,
    );
    reloaded.start();
    assert_eq!(reloaded.survival_best(), survival.score);
}

#[test]
fn survival_life_budget_is_three_then_summary() {
    let (mut engine, time) = engine();
    engine.enter_survival();
    assert_eq!(engine.survival_lives(), 3);
    let at = *time.borrow();
    for life in (0..3).rev() {
        let wrong = engine.current_expected_midis().iter().max().unwrap() + 1;
        engine.handle(note_on(wrong, at));
        assert_eq!(engine.survival_lives(), life);
    }
    assert!(!engine.is_survival());
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected the run summary, got {:?}", engine.phase());
    };
    let survival = summary.survival.as_ref().expect("survival report");
    assert_eq!(survival.notes, 0);
    assert_eq!(survival.score, 0);
    assert!(!survival.is_new_best, "a zero-note run is never a best");
    assert_eq!(engine.survival_best(), 0);
    assert_eq!(summary.error_count, 3);
    assert!(engine.current_expected_midis().is_empty());
    // A second End Run is a no-op.
    let before = engine.phase().clone();
    engine.end_survival_run();
    assert_eq!(*engine.phase(), before);
}

#[test]
fn survival_seam_swap_preserves_played_into_the_new_window() {
    let (mut engine, time) = engine();
    engine.enter_survival();
    let gen_before = engine.survival_window_gen();
    assert_eq!(gen_before, 1);
    let seam = engine.survival_seam_events;
    assert!(seam > 0);
    let window_events = engine.events.len();
    assert!(window_events > seam + 2, "three chunks make the window");
    assert_eq!(engine.survival_upcoming.len(), 2);

    // Cross the seam, then play two notes into the next line — all before
    // the swap delay elapses.
    for _ in 0..seam + 2 {
        let mut expected: Vec<u8> = engine.current_expected_midis().iter().copied().collect();
        expected.sort_unstable();
        *time.borrow_mut() += 0.05;
        let at = *time.borrow();
        for midi in expected {
            engine.handle(note_on(midi, at));
        }
    }
    engine.tick();
    assert_eq!(
        engine.survival_window_gen(),
        gen_before,
        "swap waits for the slide"
    );
    assert_eq!(engine.current_note_index(), seam + 2);

    *time.borrow_mut() += 0.5;
    engine.tick();
    assert_eq!(engine.survival_window_gen(), gen_before + 1);
    let matcher = engine.matcher.as_ref().expect("self-paced survival");
    assert_eq!(matcher.index(), 2, "played notes carry into the new window");
    assert_eq!(engine.current_note_index(), 2);
    assert_eq!(engine.note_count(), engine.events.len());
    assert_eq!(engine.survival_upcoming.len(), 2);
    assert_eq!(engine.survival_notes(), seam + 2);
    let notation = engine.notation.borrow();
    for ids in engine.event_ids.iter().take(2) {
        for id in ids {
            assert_eq!(notation.state_of(id), Some(NoteState::Correct));
        }
    }
    for id in &engine.event_ids[2] {
        assert_eq!(notation.state_of(id), Some(NoteState::Current));
    }
    drop(notation);
    assert!(engine.notation.borrow().follow_top());
    assert_eq!(
        engine.consumed_positions[0].len(),
        engine.events[0].pitches.len()
    );
    assert!(engine.consumed_positions[2].is_empty());
    // The run keeps counting across the swap.
    let mut expected: Vec<u8> = engine.current_expected_midis().iter().copied().collect();
    expected.sort_unstable();
    let at = *time.borrow() + 0.1;
    for midi in expected {
        engine.handle(note_on(midi, at));
    }
    assert_eq!(engine.survival_notes(), seam + 3);
    assert_eq!(*engine.phase(), Phase::Playing);
}

#[test]
fn survival_advance_guards_leave_state_untouched() {
    let (mut engine, time) = engine();
    engine.enter_survival();
    let generation = engine.survival_window_gen();
    let snapshot = |engine: &SessionEngine| {
        (
            engine.survival_window_gen(),
            engine.events.len(),
            engine.survival_seam_events,
            engine.survival_upcoming.len(),
            engine.exercise_id,
            engine.exercise_number,
            engine.current_note_index(),
            engine.survival_difficulties.len(),
        )
    };
    let before = snapshot(&engine);

    // A stale generation (a swap scheduled for a previous window).
    engine.advance_survival_window(generation - 1);
    assert_eq!(snapshot(&engine), before);
    engine.advance_survival_window(generation + 1);
    assert_eq!(snapshot(&engine), before);

    // No lookahead left.
    let upcoming = std::mem::take(&mut engine.survival_upcoming);
    engine.advance_survival_window(generation);
    engine.survival_upcoming = upcoming;
    assert_eq!(snapshot(&engine), before);

    // Not playing: the summary screen.
    let at = *time.borrow();
    for _ in 0..3 {
        let wrong = engine.current_expected_midis().iter().max().unwrap() + 1;
        engine.handle(note_on(wrong, at));
    }
    assert!(matches!(engine.phase(), Phase::Summary(_)));
    let after_death = snapshot(&engine);
    engine.advance_survival_window(generation);
    assert_eq!(snapshot(&engine), after_death);
}

#[test]
fn survival_chunks_are_neutral_bias_and_auto_is_two_hands() {
    let (mut engine, _time) = engine();
    engine.set_hand_mode(HandMode::Auto);
    engine.enter_survival();
    let config = &engine.generator.config;
    assert_eq!(config.measures, 2);
    assert_eq!(config.hands, Hands::Both);
    assert!(config.interval_weights.is_empty());
    assert!(config.transition_weights.is_empty());
    assert_eq!(config.probe_step, None);
    assert_eq!(config.probe_chord_shape, None);
    assert!(config.chord_shapes.is_empty(), "chords are right-hand only");
    assert_eq!(config.fifths, engine.survival_fifths);
    assert_eq!(
        engine.survival_difficulties.len(),
        3,
        "one index per chunk served"
    );
    assert!(engine.exercise().unwrap().is_two_voice());
    // Survival is self-paced even when the user chose tempo.
    engine.set_hand_mode(HandMode::Right);
    engine.set_mode(PacingMode::Tempo);
    engine.enter_survival();
    assert_eq!(engine.mode(), PacingMode::Tempo);
    assert_eq!(engine.active_pacing(), PacingMode::SelfPaced);
    assert_eq!(engine.generator.config.hands, Hands::Right);
    assert!(engine.exercise().unwrap().bass_notes.is_empty());
    assert_eq!(engine.generator.config.fifths, engine.survival_fifths);
}

#[test]
fn survival_disables_hear_it_and_resume_training_leaves_it() {
    let (time, clock) = test_clock();
    let mut engine = SessionEngine::new(
        Some(AppDatabase::in_memory(NOW)),
        Rc::new(AcceptingAudio),
        clock,
        default_backend_factory(),
        42,
    );
    engine.start();
    assert!(engine.can_playback());
    engine.enter_survival();
    assert!(!engine.can_playback());
    engine.toggle_playback();
    assert!(!engine.is_playing_back());
    assert!(engine.notation.borrow().follow_top());

    engine.resume_training();
    assert!(!engine.is_survival());
    assert!(!engine.is_diverted());
    assert!(engine.can_playback());
    assert!(!engine.notation.borrow().follow_top());
    let _ = time;
}

#[test]
fn survival_chunk_completion_chains_without_a_summary() {
    let (mut engine, time) = engine();
    engine.enter_survival();
    let completed_before = engine.exercises_completed();
    // Play the whole three-chunk window without ever ticking: the swap
    // never fires, so the window completes as one exercise.
    let total = engine.events.len();
    for _ in 0..total {
        let mut expected: Vec<u8> = engine.current_expected_midis().iter().copied().collect();
        expected.sort_unstable();
        *time.borrow_mut() += 0.05;
        let at = *time.borrow();
        for midi in expected {
            engine.handle(note_on(midi, at));
        }
    }
    assert_eq!(*engine.phase(), Phase::Playing, "no summary between chunks");
    assert!(engine.is_survival());
    assert_eq!(engine.exercises_completed(), completed_before + 1);
    assert_eq!(engine.survival_notes(), total);
    assert_eq!(
        engine.current_note_index(),
        0,
        "a fresh window starts at the top"
    );
    assert_eq!(engine.survival_window_gen(), 2);
}
