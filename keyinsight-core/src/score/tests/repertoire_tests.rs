//! Ports `Tests/KeyInSightTests/RepertoireTests.swift`
//! (MusicXMLImporterTests, PiecePlayPersistenceTests,
//! TwinkleCompletenessTests). RepertoireLibraryTests lives in
//! `bundled_library_tests.rs`.

use crate::persistence::AppDatabase;
use crate::score::{
    Exercise, ImportError, ImportedPiece, MusicXmlEncoder, MusicXmlImporter, NoteDuration,
    RepertoireLibrary, ScoreNote, Staff,
};

fn parse(xml: &str) -> Result<ImportedPiece, ImportError> {
    MusicXmlImporter::parse(xml.as_bytes(), "fallback")
}

fn wrap(measures: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<score-partwise version="4.0">
  <work><work-title>Test Piece</work-title></work>
  <part-list><score-part id="P1"><part-name>P</part-name></score-part></part-list>
  <part id="P1">{measures}</part>
</score-partwise>"#
    )
}

const ATTRS: &str = r#"<attributes><divisions>2</divisions><key><fifths>0</fifths></key>
<time><beats>4</beats><beat-type>4</beat-type></time>
<clef><sign>G</sign><line>2</line></clef></attributes>"#;

// --- MusicXMLImporterTests ---

#[test]
fn parses_simple_score() {
    let piece = parse(&wrap(&format!(
        r#"<measure number="1">{ATTRS}
  <note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>
  <note><rest/><duration>2</duration><type>quarter</type></note>
  <note><pitch><step>F</step><alter>1</alter><octave>4</octave></pitch><duration>4</duration><type>half</type></note>
</measure>"#
    )))
    .unwrap();
    assert_eq!(piece.title, "Test Piece");
    assert_eq!(
        piece.exercise.notes,
        vec![
            ScoreNote::note(60, NoteDuration::Quarter),
            ScoreNote::rest(NoteDuration::Quarter),
            ScoreNote::note(66, NoteDuration::Half),
        ]
    );
    assert_eq!(piece.exercise.beats_per_measure, 4);
    assert_eq!(piece.exercise.fifths, 0);
}

#[test]
fn scales_foreign_divisions() {
    // divisions=8: a quarter is 8, an eighth is 4.
    let piece = parse(&wrap(
        r#"<measure number="1">
  <attributes><divisions>8</divisions>
  <time><beats>2</beats><beat-type>4</beat-type></time>
  <clef><sign>G</sign><line>2</line></clef></attributes>
  <note><pitch><step>G</step><octave>4</octave></pitch><duration>8</duration><type>quarter</type></note>
  <note><pitch><step>A</step><octave>4</octave></pitch><duration>4</duration><type>eighth</type></note>
  <note><pitch><step>B</step><octave>4</octave></pitch><duration>4</duration><type>eighth</type></note>
</measure>"#,
    ))
    .unwrap();
    assert_eq!(
        piece.exercise.notes.iter().map(|n| n.duration).collect::<Vec<_>>(),
        [NoteDuration::Quarter, NoteDuration::Eighth, NoteDuration::Eighth]
    );
    assert_eq!(piece.exercise.beats_per_measure, 2);
}

#[test]
fn rejects_unsupported_content() {
    // Chords import since grand-staff support.
    let chord = wrap(&format!(
        r#"<measure number="1">{ATTRS}
  <note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration></note>
  <note><chord/><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration></note>
</measure>"#
    ));
    let chord_piece = parse(&chord).unwrap();
    let expected: Vec<std::collections::HashSet<u8>> = vec![[60, 64].into_iter().collect()];
    assert_eq!(chord_piece.exercise.expected_sets(), expected);

    // Flats import; double accidentals still reject.
    let flat = wrap(&format!(
        r#"<measure number="1">{ATTRS}
  <note><pitch><step>B</step><alter>-1</alter><octave>4</octave></pitch><duration>8</duration></note>
</measure>"#
    ));
    assert_eq!(
        parse(&flat)
            .unwrap()
            .exercise
            .sounded_notes()
            .first()
            .and_then(|n| n.midi),
        Some(70)
    );
    let double_sharp = wrap(&format!(
        r#"<measure number="1">{ATTRS}
  <note><pitch><step>F</step><alter>2</alter><octave>4</octave></pitch><duration>8</duration></note>
</measure>"#
    ));
    assert_eq!(
        parse(&double_sharp),
        Err(ImportError::Unsupported("double accidentals".into()))
    );

    let pickup = wrap(&format!(
        r#"<measure number="1">{ATTRS}
  <note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration></note>
</measure>
<measure number="2">
  <note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration></note>
</measure>"#
    ));
    assert!(parse(&pickup).is_err());

    let bass_clef = wrap(
        r#"<measure number="1">
  <attributes><divisions>2</divisions><clef><sign>F</sign><line>4</line></clef></attributes>
  <note><pitch><step>C</step><octave>3</octave></pitch><duration>8</duration></note>
</measure>"#,
    );
    assert_eq!(
        parse(&bass_clef),
        Err(ImportError::Unsupported(
            "clefs other than treble/bass".into()
        ))
    );
}

#[test]
fn round_trips_our_own_encoder() {
    let original = Exercise::new(
        vec![
            ScoreNote::note(67, NoteDuration::Quarter),
            ScoreNote::note(66, NoteDuration::Eighth),
            ScoreNote::note(67, NoteDuration::Eighth),
            ScoreNote::rest(NoteDuration::Quarter),
            ScoreNote::note(71, NoteDuration::Quarter),
            ScoreNote::note(74, NoteDuration::Whole),
        ],
        4,
    )
    .with_fifths(1);
    let xml = MusicXmlEncoder::encode(&original);
    let piece = MusicXmlImporter::parse(xml.as_bytes(), "rt").unwrap();
    assert_eq!(piece.exercise, original);
}

// --- PiecePlayPersistenceTests ---

#[test]
fn record_and_aggregate() {
    const NOW: i64 = 1_700_000_000_000;
    let mut db = AppDatabase::in_memory(NOW);
    assert_eq!(db.piece_stats("x"), None);
    db.record_piece_play("x", "X", "Self-paced", 10, 2, 0.8, "[0,2,0]", NOW);
    db.record_piece_play("x", "X", "Tempo", 10, 0, 1.0, "[0,0,0]", NOW);
    let stats = db.piece_stats("x");
    assert_eq!(stats, Some((2, 1.0)));
}

// --- TwinkleCompletenessTests ---

#[test]
fn twinkle_is_the_complete_twelve_measure_song() {
    let piece = RepertoireLibrary::bundled()
        .into_iter()
        .find(|p| p.slug == "twinkle-twinkle")
        .unwrap();
    assert_eq!(piece.exercise.measures().len(), 12);
    assert_eq!(piece.exercise.sounded_notes().len(), 42);
    // Ends on the tonic with a half note.
    let binding = piece.exercise.sounded_notes();
    let last = binding.last().unwrap();
    assert_eq!(last.midi, Some(60));
    assert_eq!(last.duration, NoteDuration::Half);
}

#[test]
fn ode_to_joy_two_hands_has_independent_voices() {
    let pieces = RepertoireLibrary::bundled();
    let piece = pieces.iter().find(|p| p.slug == "ode-to-joy-two-hands").unwrap();
    let exercise = &piece.exercise;
    assert!(exercise.is_two_voice());
    assert_eq!(exercise.measure_count(), 8);
    // LH: I/V long tones, V-I cadence (two halves) in the final bar.
    assert_eq!(exercise.bass_notes.len(), 9);
    assert!(exercise.bass_notes.iter().all(|n| n.staff == Staff::Bass));
    assert_eq!(
        exercise.bass_notes[..2]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [48, 43]
    );
    assert_eq!(
        exercise.bass_notes[exercise.bass_notes.len() - 2..]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [43, 48]
    );
    assert_eq!(
        exercise.bass_notes.last().map(|n| n.duration),
        Some(NoteDuration::Half)
    );
    // The RH melody is note-identical to the single-staff edition.
    let single = pieces.iter().find(|p| p.slug == "ode-to-joy").unwrap();
    assert_eq!(exercise.notes, single.exercise.notes);
    // Both hands are required together on the downbeats.
    assert_eq!(exercise.match_events()[0].pitches, [64, 48]);
}

#[test]
fn long_pieces_load_with_source_verified_shapes() {
    let pieces = RepertoireLibrary::bundled();

    // Gymnopédie No. 1: full Part I (31 bars + 8-bar second ending), 3/4,
    // two sharps; famous LH texture = bass quarter + held chord.
    let gym = pieces.iter().find(|p| p.slug == "gymnopedie-1").unwrap();
    assert!(gym.exercise.is_two_voice());
    assert_eq!(gym.exercise.measure_count(), 39);
    assert_eq!(gym.exercise.beats_per_measure, 3);
    assert_eq!(gym.exercise.fifths, 2);
    // Bar 1 LH: G2 bass then B3/D4/F#4 chord.
    assert_eq!(
        gym.exercise.bass_notes[..4]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [43, 66, 62, 59]
    );
    // The melody enters in bar 5 on F#5.
    assert_eq!(
        gym.exercise.sounded_notes().first().and_then(|n| n.midi),
        Some(78)
    );

    // Moonlight opening: 5 source bars re-metered as 10 bars of 3/4,
    // straight-eighth arpeggios over held bass octaves, B minor.
    let moon = pieces.iter().find(|p| p.slug == "moonlight-opening").unwrap();
    assert!(moon.exercise.is_two_voice());
    assert_eq!(moon.exercise.measure_count(), 10);
    assert_eq!(moon.exercise.fifths, 2);
    assert!(moon
        .exercise
        .sounded_notes()
        .iter()
        .all(|n| n.duration == NoteDuration::Eighth));
    assert_eq!(
        moon.exercise.sounded_notes()[..3]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [54, 59, 62]
    );
    // Bass: B octave (B1+B2) as a chord, tied across the re-metered bar
    // pair (real held notes since ties landed).
    assert_eq!(
        moon.exercise.bass_notes[..2]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [35, 47]
    );
    assert!(moon.exercise.bass_notes[1].chord_with_previous);
    assert!(moon.exercise.bass_notes.iter().any(|n| n.tied_from_previous));
    // Tied bass adds no extra matchable onsets: 7 bass onsets total.
    let bass_onsets: std::collections::HashSet<i32> =
        Exercise::voice_note_spans(&moon.exercise.bass_notes)
            .iter()
            .map(|s| s.start_units)
            .collect();
    assert_eq!(bass_onsets.len(), 7);

    // Sheep May Safely Graze: 13-bar aria strophe, faithful B-flat major.
    let sheep = pieces
        .iter()
        .find(|p| p.slug == "sheep-may-safely-graze")
        .unwrap();
    assert!(sheep.exercise.is_two_voice());
    assert_eq!(sheep.exercise.measure_count(), 13);
    assert_eq!(sheep.exercise.fifths, -2);
    // "Schafe können sicher weiden": Bb4 D5 C5 C5. D5
    assert_eq!(
        sheep.exercise.sounded_notes()[..5]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [70, 74, 72, 72, 74]
    );

    // Gymnopédie's held melody notes are real ties now, not rests.
    assert!(gym.exercise.notes.iter().any(|n| n.tied_from_previous));
    // Tie chains don't add matchable onsets: the long F#4 is ONE event.
    assert!(gym.exercise.match_events().len() < gym.exercise.all_sounded_notes().len());
}

#[test]
fn new_arrangements_load() {
    let pieces = RepertoireLibrary::bundled();

    // Jingle Bells (Two Hands): same chorus melody as the single-staff
    // edition, over root-fifth halves.
    let jb2 = pieces
        .iter()
        .find(|p| p.slug == "jingle-bells-two-hands")
        .unwrap();
    let jb1 = pieces.iter().find(|p| p.slug == "jingle-bells").unwrap();
    assert!(jb2.exercise.is_two_voice());
    assert_eq!(jb2.exercise.measure_count(), 16);
    assert_eq!(
        jb2.exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        jb1.exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>()
    );
    assert!(jb2.exercise.bass_notes.iter().all(|n| n.staff == Staff::Bass));

    // Happy Birthday: 9 bars of 3/4 with the pickup as leading rests;
    // melody G G A G C B…, ends on a dotted-half C5.
    let hb = pieces.iter().find(|p| p.slug == "happy-birthday").unwrap();
    assert!(!hb.exercise.is_two_voice());
    assert_eq!(hb.exercise.measure_count(), 9);
    assert_eq!(hb.exercise.beats_per_measure, 3);
    assert_eq!(
        hb.exercise.sounded_notes()[..6]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [67, 67, 69, 67, 72, 71]
    );
    assert_eq!(
        hb.exercise.sounded_notes().last().map(|n| n.duration),
        Some(NoteDuration::DottedHalf)
    );

    let hb2 = pieces
        .iter()
        .find(|p| p.slug == "happy-birthday-two-hands")
        .unwrap();
    assert!(hb2.exercise.is_two_voice());
    assert_eq!(
        hb2.exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        hb.exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        hb2.exercise
            .bass_notes
            .iter()
            .filter(|n| !n.is_rest())
            .count(),
        8
    );
}

#[test]
fn friska_loads_in_both_editions() {
    let pieces = RepertoireLibrary::bundled();
    let single = pieces.iter().find(|p| p.slug == "friska").unwrap();
    assert!(!single.exercise.is_two_voice());
    assert_eq!(single.exercise.measure_count(), 8);
    // The chromatic-neighbor figure in C: F#5 G5 F5 G5 (q q dotted-q 8th).
    assert_eq!(
        single.exercise.sounded_notes()[..4]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [78, 79, 77, 79]
    );
    assert_eq!(
        single.exercise.sounded_notes()[2].duration,
        NoteDuration::DottedQuarter
    );
    // Ends on the high tonic.
    assert_eq!(
        single.exercise.sounded_notes().last().and_then(|n| n.midi),
        Some(84)
    );

    let two = pieces.iter().find(|p| p.slug == "friska-two-hands").unwrap();
    assert!(two.exercise.is_two_voice());
    assert_eq!(
        two.exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        single
            .exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>()
    );
    // Oom-pah bass: alternating low root / mid pah, all bass staff.
    assert!(two.exercise.bass_notes.iter().all(|n| n.staff == Staff::Bass));
    assert_eq!(
        two.exercise.bass_notes[..4]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [38, 50, 43, 50]
    );
}

#[test]
fn camptown_races_loads_in_both_editions() {
    let pieces = RepertoireLibrary::bundled();
    let single = pieces.iter().find(|p| p.slug == "camptown-races").unwrap();
    assert!(!single.exercise.is_two_voice());
    assert_eq!(single.exercise.measure_count(), 16);
    assert_eq!(single.exercise.beats_per_measure, 2);
    // "Camp-town la-dies sing this song": G G E G A G E
    assert_eq!(
        single.exercise.sounded_notes()[..7]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [67, 67, 64, 67, 69, 67, 64]
    );
    // Chorus rises to the held high C ("...run all night").
    assert!(single.exercise.sounded_notes().iter().any(|n| n.midi == Some(72)));
    assert_eq!(
        single.exercise.sounded_notes().last().and_then(|n| n.midi),
        Some(60)
    );

    let two = pieces
        .iter()
        .find(|p| p.slug == "camptown-races-two-hands")
        .unwrap();
    assert!(two.exercise.is_two_voice());
    assert_eq!(
        two.exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        single
            .exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>()
    );
    assert!(two.exercise.bass_notes.iter().all(|n| n.staff == Staff::Bass));
}

#[test]
fn ode_to_joy_full_is_the_sixteen_bar_theme_with_busier_left_hand() {
    let pieces = RepertoireLibrary::bundled();
    let full = pieces.iter().find(|p| p.slug == "ode-to-joy-full").unwrap();
    let exercise = &full.exercise;
    assert!(exercise.is_two_voice());
    assert_eq!(exercise.measure_count(), 16);
    assert_eq!(exercise.sounded_notes().len(), 62);
    assert_eq!(exercise.bass_notes.len(), 33);
    // The bridge dips to G3 in the melody and uses eighth pairs.
    assert!(exercise.sounded_notes().iter().any(|n| n.midi == Some(55)));
    assert!(exercise
        .sounded_notes()
        .iter()
        .any(|n| n.duration == NoteDuration::Eighth));
    // LH moves in halves (root/fifth) with a quarter walkdown, all bass staff.
    assert!(exercise.bass_notes.iter().all(|n| n.staff == Staff::Bass));
    assert!(exercise
        .bass_notes
        .iter()
        .any(|n| n.duration == NoteDuration::Quarter));
    assert!(exercise
        .bass_notes
        .iter()
        .any(|n| n.duration == NoteDuration::Half));
    // Harder than the 8-bar two-hand edition.
    let simple = pieces
        .iter()
        .find(|p| p.slug == "ode-to-joy-two-hands")
        .unwrap();
    assert!(full.difficulty_index() > simple.difficulty_index());
    // Ends V-I: last two bass notes G2 -> C3.
    assert_eq!(
        exercise.bass_notes[exercise.bass_notes.len() - 2..]
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        [43, 48]
    );
}

#[test]
fn jingle_bells_is_the_full_sixteen_bar_chorus() {
    let piece = RepertoireLibrary::bundled()
        .into_iter()
        .find(|p| p.slug == "jingle-bells")
        .unwrap();
    assert_eq!(piece.exercise.fifths, 0);
    assert_eq!(piece.exercise.measures().len(), 16);
    let sounded = piece.exercise.sounded_notes();
    // "Jingle bells, jingle bells" — E E E, E E E.
    assert_eq!(
        sounded[..6].iter().map(|n| n.midi.unwrap()).collect::<Vec<_>>(),
        [64, 64, 64, 64, 64, 64]
    );
    // Ends on a whole-note tonic.
    assert_eq!(sounded.last().and_then(|n| n.midi), Some(60));
    assert_eq!(sounded.last().map(|n| n.duration), Some(NoteDuration::Whole));
}

/// Solace's A strain, machine-extracted from the CCARH kern edition.
#[test]
fn solace_a_theme_loads_from_the_kern_derived_score() {
    let piece = RepertoireLibrary::bundled()
        .into_iter()
        .find(|p| p.slug == "solace")
        .unwrap();
    assert_eq!(piece.exercise.fifths, 0);
    assert_eq!(piece.exercise.measures().len(), 16);
    // Opening of the habanera line: B4 Bb4 A4 G4…
    let sounded = piece.exercise.sounded_notes();
    assert_eq!(
        sounded[..4].iter().map(|n| n.midi.unwrap()).collect::<Vec<_>>(),
        [71, 70, 69, 67]
    );
    // Ends on C5.
    assert_eq!(sounded.last().and_then(|n| n.midi), Some(72));
    // Cross-measure held notes are real ties now.
    assert!(piece.exercise.notes.iter().any(|n| n.tied_from_previous));
}

/// The G-major version is the same song transposed up a fifth.
#[test]
fn twinkle_in_g_is_an_exact_fifth_transposition() {
    let pieces = RepertoireLibrary::bundled();
    let c = pieces.iter().find(|p| p.slug == "twinkle-twinkle").unwrap();
    let g = pieces.iter().find(|p| p.slug == "twinkle-twinkle-g").unwrap();
    assert_eq!(g.title, "Twinkle, Twinkle, Little Star (in G)");
    assert_eq!(g.exercise.fifths, 1);
    assert_eq!(g.exercise.measures().len(), 12);
    assert_eq!(
        g.exercise.sounded_notes().len(),
        c.exercise.sounded_notes().len()
    );
    assert_eq!(
        g.exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.unwrap())
            .collect::<Vec<_>>(),
        c.exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.unwrap() + 7)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        g.exercise
            .sounded_notes()
            .iter()
            .map(|n| n.duration)
            .collect::<Vec<_>>(),
        c.exercise
            .sounded_notes()
            .iter()
            .map(|n| n.duration)
            .collect::<Vec<_>>()
    );
}
