//! Ports the score/skill half of `Tests/KeyInSightTests/LeftHandTests.swift`
//! (`LeftHandTrainingTests`): bass skill items, bass-clef encoding, and the
//! per-staff key availability. The generator-driven tests (`hands = Left`)
//! arrive with the generator's hands modes.

use std::collections::HashSet;

use crate::persistence::PitchItemStat;
use crate::score::{
    DifficultyDescriptors, Exercise, MusicXmlEncoder, NoteDuration, ScoreNote, Staff,
};
use crate::skill::{SkillModel, BASS_UNLOCK_ORDER, UNLOCK_ORDER};

const NOW: i64 = 1_700_000_000_000;
const BASS_SEED: [u8; 5] = [48, 50, 52, 53, 55];

/// A hand-built left-hand walk over the bass seed: what `hands = Left`
/// generation produces (notes empty, the bass voice carrying the line).
fn bass_only_exercise() -> Exercise {
    let bass: Vec<ScoreNote> = [
        (Some(48), NoteDuration::Quarter),
        (Some(50), NoteDuration::Quarter),
        (Some(52), NoteDuration::Half),
        (None, NoteDuration::Quarter),
        (Some(55), NoteDuration::Quarter),
        (Some(53), NoteDuration::Half),
    ]
    .iter()
    .map(|&(midi, duration)| ScoreNote::new(midi, duration).with_staff(Staff::Bass))
    .collect();
    Exercise::new(vec![], 4).with_bass(bass)
}

#[test]
fn bass_only_encodes_as_single_bass_clef_staff() {
    let exercise = bass_only_exercise();
    assert!(exercise.is_bass_only());
    let xml = MusicXmlEncoder::encode(&exercise);
    assert!(xml.contains("<clef><sign>F</sign><line>4</line></clef>"));
    assert!(!xml.contains("<staves>"));
    assert!(!xml.contains("<backup>"));
    assert!(!xml.contains("<staff>"));
}

// --- Bass skill model ---

#[test]
fn bass_model_uses_bass_order() {
    let model = SkillModel::with_staff(Staff::Bass, 5);
    assert_eq!(model.unlocked_count(), 5);
    assert_eq!(
        model
            .active_states()
            .iter()
            .map(|s| s.midi)
            .collect::<Vec<_>>(),
        [48, 50, 52, 53, 55]
    );
    assert_eq!(model.next_locked_midi(), Some(47)); // B2 first expansion (downward)
    assert_eq!(&BASS_UNLOCK_ORDER[15..], [54, 49, 56, 51, 58]); // sharps last
    let unique: HashSet<u8> = BASS_UNLOCK_ORDER.iter().copied().collect();
    assert_eq!(unique.len(), BASS_UNLOCK_ORDER.len());
}

#[test]
fn bass_mastery_reads_bass_prefixed_stats() {
    let stats: Vec<PitchItemStat> = BASS_SEED
        .iter()
        .map(|&midi| PitchItemStat {
            item: SkillModel::item_name_on(midi, Staff::Bass),
            attempts: 6,
            errors: 0,
            ewma_error: 0.05,
            ewma_latency_ms: Some(900.0),
            last_seen_at_ms: NOW,
        })
        .collect();
    let mut model = SkillModel::with_staff(Staff::Bass, 5);
    model.refresh(&stats);
    assert!(model.all_active_mastered());
    assert_eq!(model.unlock_if_earned(), Some(47));
    // The same stats do nothing for the treble model: separate skills.
    let mut treble = SkillModel::default();
    treble.refresh(&stats);
    assert!(!treble.all_active_mastered());
}

#[test]
fn bass_keys_unlock_with_bass_register_sharps() {
    let mut model = SkillModel::with_staff(Staff::Bass, 5);
    let fifths = |m: &SkillModel| {
        m.available_keys()
            .iter()
            .map(|k| k.fifths)
            .collect::<Vec<_>>()
    };
    assert_eq!(fifths(&model), [0]);
    model.set_unlocked_count(16); // through F#3
    model.refresh(&[]);
    assert_eq!(fifths(&model), [0, 1]);
    model.set_unlocked_count(17); // + C#3
    model.refresh(&[]);
    assert_eq!(fifths(&model), [0, 1, 2]);
}

#[test]
fn mean_active_weight_tracks_weakness() {
    let mut model = SkillModel::default();
    assert_eq!(model.mean_active_weight(), 2.5); // all unseen = frontier
    let stats: Vec<PitchItemStat> = UNLOCK_ORDER[..5]
        .iter()
        .map(|&midi| PitchItemStat {
            item: SkillModel::item_name(midi),
            attempts: 10,
            errors: 0,
            ewma_error: 0.0,
            ewma_latency_ms: Some(600.0),
            last_seen_at_ms: NOW,
        })
        .collect();
    model.refresh(&stats);
    assert!((model.mean_active_weight() - 1.0).abs() < 0.01);
}

#[test]
fn descriptors_cover_both_voices_without_seam_intervals() {
    let exercise = Exercise::new(
        vec![
            ScoreNote::note(60, NoteDuration::Half),
            ScoreNote::note(62, NoteDuration::Half),
        ],
        4,
    )
    .with_bass(vec![
        ScoreNote::note(36, NoteDuration::Whole).with_staff(Staff::Bass)
    ]);
    let d = DifficultyDescriptors::compute(&exercise);
    assert_eq!(d.range_semitones, 26); // C2–D4 across both hands
    assert_eq!(d.leap_ratio, 0.0); // no phantom treble→bass interval
    assert!((d.notes_per_measure - 3.0).abs() < 1e-9);
}
