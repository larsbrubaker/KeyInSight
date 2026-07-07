//! Ports `Tests/KeyInSightTests/SkillModelTests.swift` (including the
//! DifficultyDescriptorTests suite, which lives in the same Swift file).

use crate::persistence::PitchItemStat;
use crate::score::{DifficultyDescriptors, Exercise, NoteDuration, ScoreNote};
use crate::skill::{SkillModel, INTERVAL_DELTAS, SEED_COUNT, UNLOCK_ORDER};

const NOW: i64 = 1_700_000_000_000;

fn stat(midi: u8, attempts: i64, ewma_error: f64, latency: Option<f64>) -> PitchItemStat {
    PitchItemStat {
        item: SkillModel::item_name(midi),
        attempts,
        errors: (attempts as f64 * ewma_error) as i64,
        ewma_error,
        ewma_latency_ms: latency,
        last_seen_at_ms: NOW,
    }
}

/// Stats that satisfy every mastery threshold.
fn mastered_stats(midis: &[u8]) -> Vec<PitchItemStat> {
    midis
        .iter()
        .map(|&m| stat(m, 6, 0.05, Some(900.0)))
        .collect()
}

#[test]
fn starts_with_seed_set() {
    let model = SkillModel::default();
    assert_eq!(model.unlocked_count(), 5);
    assert_eq!(
        model.active_states().iter().map(|s| s.midi).collect::<Vec<_>>(),
        [60, 62, 64, 65, 67]
    );
    assert_eq!(model.next_locked_midi(), Some(69));
}

#[test]
fn mastery_thresholds() {
    assert!(SkillModel::is_mastered(Some(&stat(60, 6, 0.1, Some(900.0)))));
    // Too few attempts.
    assert!(!SkillModel::is_mastered(Some(&stat(60, 3, 0.0, Some(900.0)))));
    // Too many errors.
    assert!(!SkillModel::is_mastered(Some(&stat(60, 6, 0.3, Some(900.0)))));
    // Too slow.
    assert!(!SkillModel::is_mastered(Some(&stat(60, 6, 0.1, Some(2500.0)))));
    // Never seen.
    assert!(!SkillModel::is_mastered(None));
}

#[test]
fn weakness_weights() {
    // Unseen items are the frontier: strongly biased.
    assert_eq!(SkillModel::weight(None), 2.5);
    // Clean fast item ≈ neutral.
    let clean = SkillModel::weight(Some(&stat(60, 10, 0.0, Some(600.0))));
    assert!((clean - 1.0).abs() < 0.01);
    // Error-prone item weighted up.
    let weak = SkillModel::weight(Some(&stat(60, 10, 0.5, Some(600.0))));
    assert!(weak > 2.0);
    // Slow item weighted up even without errors.
    let slow = SkillModel::weight(Some(&stat(60, 10, 0.0, Some(2800.0))));
    assert!(slow > 1.5);
}

#[test]
fn unlock_requires_all_active_mastered() {
    let mut model = SkillModel::default();
    // Four mastered, one weak: no unlock.
    let mut stats = mastered_stats(&[60, 62, 64, 65]);
    stats.push(stat(67, 6, 0.5, Some(900.0)));
    model.refresh(&stats);
    assert!(!model.all_active_mastered());
    assert_eq!(model.unlock_if_earned(), None);
    assert_eq!(model.unlocked_count(), 5);

    // All five mastered: unlock A4.
    model.refresh(&mastered_stats(&[60, 62, 64, 65, 67]));
    assert!(model.all_active_mastered());
    assert_eq!(model.unlock_if_earned(), Some(69));
    assert_eq!(model.unlocked_count(), 6);

    // Post-unlock: the new item is unseen, so the set is no longer mastered.
    model.refresh(&mastered_stats(&[60, 62, 64, 65, 67]));
    assert!(!model.all_active_mastered());
    let new_item = model
        .active_states()
        .into_iter()
        .find(|s| s.midi == 69)
        .unwrap()
        .weight;
    assert_eq!(new_item, 2.5);
}

#[test]
fn unlock_order_expands_outward_then_sharps() {
    assert_eq!(&UNLOCK_ORDER[..5], [60, 62, 64, 65, 67]);
    assert_eq!(UNLOCK_ORDER[5], 69); // A4 first expansion
    assert_eq!(UNLOCK_ORDER[6], 59); // then B3 below
    assert_eq!(&UNLOCK_ORDER[15..], [66, 61, 68, 63, 70]); // sharps last
    let unique: std::collections::HashSet<u8> = UNLOCK_ORDER.into_iter().collect();
    assert_eq!(unique.len(), UNLOCK_ORDER.len());
}

#[test]
fn targeted_items_are_weakest() {
    let mut model = SkillModel::default();
    let mut stats = mastered_stats(&[60, 62]);
    stats.push(stat(64, 6, 0.6, Some(900.0))); // weakest seen
    stats.push(stat(65, 6, 0.4, Some(900.0)));
    // 67 unseen → weight 2.5
    model.refresh(&stats);
    let targeted = model.targeted_item_names();
    assert_eq!(targeted.len(), 3);
    assert_eq!(targeted[0], SkillModel::item_name(64));
    assert!(targeted.contains(&SkillModel::item_name(67)));
}

#[test]
fn interval_item_names() {
    assert_eq!(SkillModel::interval_item_name(0), "interval:unison");
    assert_eq!(SkillModel::interval_item_name(1), "interval:2nd-up");
    assert_eq!(SkillModel::interval_item_name(-2), "interval:3rd-down");
    assert_eq!(SkillModel::interval_item_name(3), "interval:4th-up");
}

#[test]
fn interval_weights_neutral_until_seen() {
    let mut model = SkillModel::default();
    model.refresh(&[PitchItemStat {
        item: "interval:3rd-down".to_string(),
        attempts: 8,
        errors: 4,
        ewma_error: 0.5,
        ewma_latency_ms: Some(700.0),
        last_seen_at_ms: NOW,
    }]);
    let weights = model.interval_weights();
    // Unseen shapes are neutral (not frontier-boosted like unseen pitches).
    assert_eq!(weights[&1], 1.0);
    assert_eq!(weights[&0], 1.0);
    // Weak shape weighted up.
    assert!(weights[&-2] > 2.0);
    // Every tracked delta is present.
    assert_eq!(weights.len(), INTERVAL_DELTAS.len());
}

#[test]
fn diatonic_pitch_classes() {
    assert!(SkillModel::diatonic_pitch_classes(0).contains(&5)); // F in C
    assert!(!SkillModel::diatonic_pitch_classes(1).contains(&5)); // no F in G
    assert!(SkillModel::diatonic_pitch_classes(1).contains(&6)); // F# in G
    assert!(SkillModel::diatonic_pitch_classes(2).contains(&1)); // C# in D
    assert!(!SkillModel::diatonic_pitch_classes(2).contains(&0)); // no C in D
}

#[test]
fn keys_unlock_with_their_sharps() {
    let mut model = SkillModel::default();
    assert_eq!(
        model.available_keys().iter().map(|k| k.fifths).collect::<Vec<_>>(),
        [0]
    );

    // Unlock through F#4 (index 15 in UNLOCK_ORDER → count 16).
    model.set_unlocked_count(16);
    model.refresh(&[]);
    assert_eq!(
        model.available_keys().iter().map(|k| k.fifths).collect::<Vec<_>>(),
        [0, 1]
    );

    // Through C#4 (index 16 → count 17): D major joins.
    model.set_unlocked_count(17);
    model.refresh(&[]);
    let keys = model.available_keys();
    assert_eq!(keys.iter().map(|k| k.fifths).collect::<Vec<_>>(), [0, 1, 2]);
    // New sharps are weak/unseen → sharp keys outweigh C.
    assert!(keys[1].weight > keys[0].weight);
}

#[test]
fn key_filters_active_pitches() {
    let mut model = SkillModel::default();
    model.set_unlocked_count(16); // naturals + F#4
    model.refresh(&[]);
    let g_major: Vec<u8> = model
        .active_pitch_options_in_key(1)
        .iter()
        .map(|o| o.midi)
        .collect();
    assert!(!g_major.contains(&65)); // F4 out
    assert!(!g_major.contains(&77)); // F5 out
    assert!(g_major.contains(&66)); // F#4 in
    assert!(g_major.contains(&60)); // C4 still in
}

#[test]
fn set_unlocked_count_clamps() {
    let mut model = SkillModel::default();
    model.set_unlocked_count(2);
    assert_eq!(model.unlocked_count(), SEED_COUNT);
    model.set_unlocked_count(999);
    assert_eq!(model.unlocked_count(), UNLOCK_ORDER.len());
}

// --- DifficultyDescriptorTests ---

#[test]
fn ascending_scale() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::note(62, NoteDuration::Quarter),
            ScoreNote::note(64, NoteDuration::Quarter),
            ScoreNote::note(65, NoteDuration::Quarter),
            ScoreNote::note(67, NoteDuration::Whole),
        ],
        4,
    );
    let d = DifficultyDescriptors::compute(&exercise);
    assert_eq!(d.range_semitones, 7);
    assert!((d.pitch_entropy_bits - (5.0f64).log2()).abs() < 1e-9);
    assert!((d.notes_per_measure - 2.5).abs() < 1e-9);
    assert_eq!(d.leap_ratio, 0.0); // all stepwise
    assert_eq!(d.repetitiveness, 0.0); // no repeated bigram
}

#[test]
fn repetitive_pattern() {
    let notes: Vec<ScoreNote> = [60, 62, 60, 62, 60]
        .iter()
        .map(|&m| ScoreNote::note(m, NoteDuration::Quarter))
        .collect();
    let d = DifficultyDescriptors::compute(&Exercise::new(notes, 4));
    // 4 intervals, 2 unique bigrams (C→D, D→C).
    assert!((d.repetitiveness - 0.5).abs() < 1e-9);
    assert_eq!(d.leap_ratio, 0.0);
}

#[test]
fn arpeggio_is_all_leaps() {
    let notes: Vec<ScoreNote> = [60, 64, 67]
        .iter()
        .map(|&m| ScoreNote::note(m, NoteDuration::Quarter))
        .collect();
    let d = DifficultyDescriptors::compute(&Exercise::new(notes, 4));
    assert_eq!(d.leap_ratio, 1.0);
}

#[test]
fn json_round_trips() {
    let d = DifficultyDescriptors::compute(&Exercise::new(
        vec![ScoreNote::note(60, NoteDuration::Whole)],
        4,
    ));
    let decoded: DifficultyDescriptors = serde_json::from_str(&d.json()).unwrap();
    assert_eq!(decoded, d);
}
