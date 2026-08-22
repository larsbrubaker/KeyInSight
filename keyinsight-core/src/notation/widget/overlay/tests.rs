//! The overlay geometry, in the screen pixels the Swift page's CSS used:
//! the ghost oval and its ledger lines (`showGhost` / `addLedger`) and the
//! timing tick's anchor (`addTick`).

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::core::SplitMix64;
use crate::notation::NotationRenderer;
use crate::score::{ExerciseGenerator, MusicXmlEncoder, PitchOption};

/// A notehead box as Verovio engraves one at the reading scale: wider
/// than it is tall, sitting on the staff.
fn head() -> Rect {
    Rect::new(100.0, 200.0, 15.0, 12.5)
}

/// A staff space on screen, and the staff `head()` sits on: the notehead
/// centre is on the bottom line, the five lines four spaces above it.
const SPACE: f64 = 10.0;
const STAFF_BOTTOM: f64 = 206.25;
const STAFF_TOP: f64 = STAFF_BOTTOM + 4.0 * SPACE;

#[test]
fn the_ghost_is_the_notehead_box_moved_half_a_space_per_step() {
    // No offset: exactly the expected note's box.
    assert_eq!(ghost_oval(head(), 0, SPACE), head());
    // Positive steps play higher — up the screen, half a space each.
    let up = ghost_oval(head(), 3, SPACE);
    assert_eq!(up.center().y, head().center().y + 15.0);
    assert_eq!((up.x, up.width, up.height), (100.0, 15.0, 12.5));
    let down = ghost_oval(head(), -2, SPACE);
    assert_eq!(down.center().y, head().center().y - 10.0);
    // The oval is the notehead box; `box-sizing: border-box` puts the
    // 2.5 px border inside it, so the stroked path is a stroke narrower.
    assert_eq!(GHOST_STROKE, 2.5);
    assert_eq!(up.width - GHOST_STROKE, 12.5);
}

#[test]
fn ghost_ledgers_step_out_one_per_space_beyond_the_staff() {
    // Inside the staff: no ledgers.
    let inside = ghost_oval(head(), 4, SPACE);
    assert!(ghost_ledgers(inside, STAFF_TOP, STAFF_BOTTOM, SPACE).is_empty());
    // Three spaces below the bottom line: one line per space crossed,
    // the last one under the ghost itself.
    let below = ghost_oval(head(), -6, SPACE);
    let ledgers = ghost_ledgers(below, STAFF_TOP, STAFF_BOTTOM, SPACE);
    let ys: Vec<f64> = ledgers.iter().map(|l| l.center().y).collect();
    assert_eq!(ys, vec![196.25, 186.25, 176.25]);
    assert_eq!(below.center().y, 176.25);
    // Each is 2 px of #9a9a9a, headWidth × 1.8 wide, centred on the head.
    for ledger in &ledgers {
        assert_eq!(ledger.height, 2.0);
        assert_eq!(ledger.width, 15.0 * 1.8);
        assert_eq!(ledger.center().x, head().center().x);
    }
    // Above the staff, the same count going the other way.
    let above = ghost_oval(head(), 14, SPACE);
    let ys: Vec<f64> = ghost_ledgers(above, STAFF_TOP, STAFF_BOTTOM, SPACE)
        .iter()
        .map(|l| l.center().y)
        .collect();
    assert_eq!(ys, vec![256.25, 266.25, 276.25]);
    assert_eq!(above.center().y, 276.25);
    // A ghost that stops in the space between two ledger positions only
    // gets the lines it has actually passed.
    let half = ghost_oval(head(), -5, SPACE);
    assert_eq!(ghost_ledgers(half, STAFF_TOP, STAFF_BOTTOM, SPACE).len(), 2);
}

#[test]
fn the_tick_sits_twenty_four_above_and_six_left_of_the_notehead() {
    let ink = tick_ink(head());
    // `left: cx - 6`, `top: r.top - 24` — plus where SF Pro put the ink
    // inside that 15 px text box.
    assert_eq!(ink.x, head().center().x - TICK_LEFT + TICK_INK_DX);
    assert_eq!(ink.top(), head().top() + TICK_ABOVE - TICK_INK_DY);
    assert_eq!((ink.x, ink.top()), (103.25, 228.0));
    assert_eq!((ink.width, ink.height), (4.5, 4.5));
    // The box is clear of the notehead it belongs to.
    assert!(ink.bottom() > head().top());
}

#[test]
fn a_ghost_reads_the_staff_lines_of_its_own_note() {
    let renderer = Rc::new(RefCell::new(NotationRenderer::new()));
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 2;
    let mut rng = SplitMix64::new(7);
    let pitches: Vec<PitchOption> = [60, 62, 64, 65, 67]
        .iter()
        .map(|&m| PitchOption::new(m))
        .collect();
    let exercise = generator.generate(&pitches, &mut rng);
    let rendered = renderer
        .borrow_mut()
        .render(&MusicXmlEncoder::encode(&exercise))
        .expect("render");
    let renderer = renderer.borrow();
    let space = renderer.staff_space();
    let layout = renderer.toolkit().current_layout().expect("layout");
    for id in &rendered.note_ids {
        let bounds = layout.bounds_by_id[id];
        let (top, bottom) = staff_lines_at(layout, bounds, space).expect("the note's staff");
        assert_eq!(bottom - top, 4.0 * space, "five lines, four spaces");
        // Middle C hangs a ledger line below the treble staff, so the
        // head is only ever within a space of the staff itself.
        let cy = bounds.1 + bounds.3 / 2.0;
        assert!(
            cy > top - space && cy < bottom + space,
            "note {id} at {cy} against staff {top}..{bottom}"
        );
    }
}
