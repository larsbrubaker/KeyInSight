//! Endless micro-drill: cards chain until End Drill, never repeat
//! back-to-back, sound as they appear, and a missed card comes back for a
//! retrieval rep a few cards later.

use std::rc::Rc;

use super::{engine, note_on, test_clock, RecordingAudio, NOW};
use crate::core::PitchSpelling;
use crate::engine::default_backend_factory;
use crate::engine::session::{DrillRedo, InputSource, Phase, SessionEngine, DRILL_REDO_DELAY_CARDS};
use crate::persistence::AppDatabase;
use crate::score::Staff;

/// Answer the current card correctly; returns the card's pitch.
fn answer_card(engine: &mut SessionEngine, time: &Rc<std::cell::RefCell<f64>>) -> u8 {
    let expected = engine.current_expected_midi().expect("a drill card is one note");
    *time.borrow_mut() += 0.4;
    let at = *time.borrow();
    engine.handle(note_on(expected, at));
    expected
}

#[test]
fn drill_is_endless_until_end_drill_yields_one_summary() {
    let (mut engine, time) = engine();
    engine.start_drill();
    assert!(engine.drill_active());
    assert!(engine.is_diverted());
    for _ in 0..20 {
        answer_card(&mut engine, &time);
        assert_eq!(*engine.phase(), Phase::Playing, "no per-card summary");
    }
    assert_eq!(engine.drill_cards_done(), 20);

    engine.end_drill();
    assert!(!engine.drill_active());
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected drill summary, got {:?}", engine.phase());
    };
    assert!(summary.drill);
    assert_eq!(summary.note_count, 20);
    assert_eq!(summary.first_try_correct, 20);
    assert_eq!(summary.error_count, 0);
    assert!(summary.mean_latency_ms.is_some());
    // A second End Drill is a no-op.
    let before = engine.phase().clone();
    engine.end_drill();
    assert_eq!(*engine.phase(), before);
}

#[test]
fn drill_never_shows_the_same_card_back_to_back() {
    let (mut engine, time) = engine();
    engine.start_drill();
    let mut cards: Vec<u8> = Vec::new();
    for _ in 0..40 {
        cards.push(answer_card(&mut engine, &time));
    }
    for pair in cards.windows(2) {
        assert_ne!(pair[0], pair[1], "identical consecutive cards are invisible");
    }
    // Cards draw from the active set.
    assert!(cards.iter().all(|m| (60..=67).contains(m)));
}

#[test]
fn drill_wrong_strike_hints_and_schedules_a_redo() {
    let (mut engine, time) = engine();
    engine.start_drill();
    let card = engine.current_expected_midi().unwrap();
    let wrong = if card == 60 { 62 } else { 60 };
    *time.borrow_mut() += 0.3;
    let at = *time.borrow();
    engine.handle(note_on(wrong, at));
    assert!(engine.drill_hint_keys(), "a wrong strike reveals the keys");
    assert_eq!(
        engine.inspection(),
        Some(
            format!(
                "That's {} — the card is {}",
                PitchSpelling::name(wrong),
                PitchSpelling::name(card)
            )
            .as_str()
        )
    );
    // Still the same card until the right key lands.
    assert_eq!(engine.current_expected_midi(), Some(card));
    assert_eq!(engine.drill_cards_done(), 0);

    engine.handle(note_on(card, at + 0.3));
    assert_eq!(engine.drill_cards_done(), 1);
    assert!(!engine.drill_hint_keys(), "the hint clears with the next card");
    assert_eq!(engine.inspection(), None);
    assert_eq!(
        engine.drill_redo,
        vec![DrillRedo {
            midi: card,
            staff: Staff::Treble,
            due: 1 + DRILL_REDO_DELAY_CARDS,
        }]
    );

    // Three more cards, then the retrieval rep is due.
    let mut fourth = 0;
    for _ in 0..3 {
        fourth = answer_card(&mut engine, &time);
    }
    assert_eq!(engine.drill_cards_done(), 4);
    let fifth = engine.current_expected_midi().unwrap();
    if fourth == card {
        // Never back-to-back: the rep waits one more card.
        assert_ne!(fifth, card);
        assert!(!engine.drill_redo.is_empty());
        answer_card(&mut engine, &time);
        assert_eq!(engine.current_expected_midi(), Some(card));
    } else {
        assert_eq!(fifth, card, "the missed card returns {DRILL_REDO_DELAY_CARDS} cards later");
    }
    assert!(engine.drill_redo.is_empty(), "a served rep leaves the queue");
}

#[test]
fn drill_cards_sound_as_they_appear_except_over_the_mic() {
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
    assert!(audio.notes.borrow().is_empty(), "training exercises are silent");

    engine.start_drill();
    let first = engine.current_expected_midi().unwrap();
    assert_eq!(*audio.notes.borrow(), vec![(first, 0.8)]);
    answer_card(&mut engine, &time);
    let second = engine.current_expected_midi().unwrap();
    assert_eq!(*audio.notes.borrow(), vec![(first, 0.8), (second, 0.8)]);

    // Over the mic the app would hear itself answer.
    engine.set_input_source(InputSource::Microphone);
    answer_card(&mut engine, &time);
    assert_eq!(audio.notes.borrow().len(), 2);
}
