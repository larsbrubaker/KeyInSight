//! Ports `Tests/KeyInSightTests/OctaveAnchorTests.swift`.

use crate::engine::OctaveAnchor;

#[test]
fn anchors_an_octave_down_and_follows_for_the_rest() {
    let mut anchor = OctaveAnchor::default();
    // Expected C4 (60), played C3 (48): matched, exercise follows.
    assert_eq!(anchor.process_note_on(48, Some(60)), 60);
    assert_eq!(anchor.user_octaves(), -1);
    // Subsequent notes shift by the same amount (D3 → D4).
    assert_eq!(anchor.process_note_on(50, Some(62)), 62);
    // Note-offs too.
    assert_eq!(anchor.apply(50), 62);
}

#[test]
fn anchors_up_to_two_octaves_up() {
    let mut anchor = OctaveAnchor::default();
    assert_eq!(anchor.process_note_on(84, Some(60)), 60);
    assert_eq!(anchor.user_octaves(), 2);
}

#[test]
fn exact_first_note_locks_zero_shift_so_later_octave_slips_are_errors() {
    let mut anchor = OctaveAnchor::default();
    assert_eq!(anchor.process_note_on(60, Some(60)), 60);
    assert_eq!(anchor.user_octaves(), 0);
    // An octave slip after anchoring is NOT absorbed.
    assert_eq!(anchor.process_note_on(74, Some(62)), 74);
}

#[test]
fn wrong_pitch_class_never_anchors_but_a_later_octave_match_does() {
    let mut anchor = OctaveAnchor::default();
    // E3 against expected C4: wrong note, passes through unshifted.
    assert_eq!(anchor.process_note_on(52, Some(60)), 52);
    assert_eq!(anchor.shift(), None);
    // The user finds the right key in their octave: anchors now.
    assert_eq!(anchor.process_note_on(48, Some(60)), 60);
    assert_eq!(anchor.user_octaves(), -1);
}

#[test]
fn beyond_two_octaves_does_not_anchor() {
    let mut anchor = OctaveAnchor::default();
    // C7 against C4 (+36): almost certainly not "the user's octave".
    assert_eq!(anchor.process_note_on(96, Some(60)), 96);
    assert_eq!(anchor.shift(), None);
}

#[test]
fn no_expectation_passes_through() {
    let mut anchor = OctaveAnchor::default();
    assert_eq!(anchor.process_note_on(48, None), 48);
    assert_eq!(anchor.shift(), None);
}
