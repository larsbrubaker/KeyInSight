//! Ports the score/skill half of `Tests/KeyInSightTests/LeftHandTests.swift`
//! (`LeftHandTrainingTests`): bass-clef generation and encoding, drill
//! cards on the grand staff, bass skill items, and the per-staff key
//! availability. (`SynchronousChordTests`, `FreePlayRecordingTests`, and
//! `PartialReplayTests` from the same file are engine/matcher tests.)

use std::collections::HashSet;

use crate::core::SplitMix64;
use crate::notation::NotationRenderer;
use crate::persistence::PitchItemStat;
use crate::score::{
    DifficultyDescriptors, Exercise, ExerciseGenerator, Hands, MusicXmlEncoder, NoteDuration,
    PitchOption, ScoreNote, Staff,
};
use crate::skill::{SkillModel, BASS_UNLOCK_ORDER, UNLOCK_ORDER};

const NOW: i64 = 1_700_000_000_000;
const BASS_SEED: [u8; 5] = [48, 50, 52, 53, 55];

fn bass_seed_options() -> Vec<PitchOption> {
    BASS_SEED.iter().map(|&m| PitchOption::new(m)).collect()
}

fn generate_left(seed: u64, measures: i32) -> Exercise {
    let mut rng = SplitMix64::new(seed);
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = measures;
    generator.config.hands = Hands::Left;
    generator.generate(&bass_seed_options(), &mut rng)
}

#[test]
fn left_hand_walk_lives_in_the_bass_voice() {
    let allowed: HashSet<u8> = BASS_SEED.iter().copied().collect();
    for seed in 1..=30u64 {
        let ex = generate_left(seed, 3);
        assert!(ex.notes.is_empty());
        assert!(ex.is_bass_only());
        assert!(ex.bass_notes.iter().all(|n| n.staff == Staff::Bass));
        for note in ex.bass_notes.iter().filter(|n| !n.is_rest()) {
            assert!(allowed.contains(&note.midi.unwrap()), "seed {seed}");
        }
        assert_eq!(ex.bass_measures().len(), 3, "seed {seed}");
        for measure in ex.bass_measures() {
            assert_eq!(
                measure.iter().map(|n| n.duration.units()).sum::<i32>(),
                ex.units_per_measure(),
                "seed {seed}"
            );
        }
    }
}

#[test]
fn bass_only_encodes_as_single_bass_clef_staff() {
    let xml = MusicXmlEncoder::encode(&generate_left(3, 2));
    assert!(xml.contains("<clef><sign>F</sign><line>4</line></clef>"));
    assert!(!xml.contains("<staves>"));
    assert!(!xml.contains("<backup>"));
    assert!(!xml.contains("<staff>"));
}

#[test]
fn bass_only_match_events_are_all_bass_staff() {
    let ex = generate_left(4, 2);
    let events = ex.match_events();
    assert!(!events.is_empty());
    assert!(events
        .iter()
        .all(|e| e.staves.iter().all(|&s| s == Staff::Bass)));
}

/// A single bass-clef staff engraves (the sibling `verovio-rust` importer
/// accepts an F clef on a one-staff part) and its ids read in order.
#[test]
fn bass_only_renders_and_pitches_agree_per_id() {
    let mut renderer = NotationRenderer::new();
    let ex = generate_left(5, 2);
    let rendered = renderer
        .render(&MusicXmlEncoder::encode(&ex))
        .expect("bass-only exercise engraves");
    let events = ex.match_events();
    assert_eq!(rendered.note_ids.len(), events.len()); // monophonic line
    for (id, event) in rendered.note_ids.iter().zip(&events) {
        assert_eq!(renderer.midi_pitch(id), Some(event.pitches[0]));
    }
}

#[test]
fn drill_cards_render_on_the_grand_staff() {
    // Which-clef is part of the read: the other staff holds a rest.
    let mut rng = SplitMix64::new(1);
    let bass_card =
        ExerciseGenerator::drill_note(&bass_seed_options(), Staff::Bass, None, &mut rng);
    assert!(bass_card.is_two_voice() && !bass_card.is_bass_only());
    assert!(bass_card.bass_notes.len() == 1 && !bass_card.bass_notes[0].is_rest());
    assert!(bass_card.notes.len() == 1 && bass_card.notes[0].is_rest());
    assert!(MusicXmlEncoder::encode(&bass_card).contains("<staves>2</staves>"));
    let treble_card =
        ExerciseGenerator::drill_note(&[PitchOption::new(60)], Staff::Treble, None, &mut rng);
    assert!(treble_card.is_two_voice());
    assert!(treble_card.bass_notes.len() == 1 && treble_card.bass_notes[0].is_rest());
    assert_eq!(treble_card.match_events().len(), 1);
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
