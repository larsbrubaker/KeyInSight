//! Ports the renderer probes from `Tests/KeyInSightTests/GeneratorTests.swift`
//! (`SystemLayoutProbe`, `BarlineAlignmentProbe`,
//! `RenderOptionStabilityProbe`) plus the follow-top slide at the
//! controller level. Where the Swift probes scraped the SVG for
//! `class="system"` / `class="barLine"`, these read the toolkit's per-system
//! geometry directly.

use std::cell::RefCell;
use std::rc::Rc;

use super::{NotationController, NotationRenderer, NoteState};
use crate::core::SplitMix64;
use crate::score::{Exercise, ExerciseGenerator, Hands, MusicXmlEncoder, PitchOption};

fn pitches(midis: &[u8]) -> Vec<PitchOption> {
    midis.iter().map(|&m| PitchOption::new(m)).collect()
}

/// The Swift page: scale 60 of a 1400 page → an 840px-wide score.
const PAGE_VIEW: (f64, f64) = (840.0, 600.0);

#[test]
fn each_survival_system_holds_exactly_two_measures() {
    let mut renderer = NotationRenderer::new();
    renderer.fit_view(PAGE_VIEW.0, PAGE_VIEW.1);
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 8;
    generator.config.hands = Hands::Both;
    let mut rng = SplitMix64::new(9);
    let ex = generator.generate(&pitches(&[60, 62, 64, 65, 67]), &mut rng);
    let xml = MusicXmlEncoder::encode_with_breaks(&ex, Some(2));
    renderer.render_with(&xml, true).expect("feed render");
    let layout = renderer.toolkit().current_layout().expect("layout");
    let counts: Vec<usize> = layout.systems.iter().map(|s| s.measure_count).collect();
    assert_eq!(counts, [2, 2, 2, 2], "measures per system: {counts:?}");
}

#[test]
fn feed_layout_aligns_mid_barlines() {
    let mut renderer = NotationRenderer::new();
    renderer.fit_view(PAGE_VIEW.0, PAGE_VIEW.1);
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 2;
    generator.config.hands = Hands::Both;
    let mut positions: Vec<f64> = Vec::new();
    for seed in 1..=12u64 {
        let mut rng = SplitMix64::new(seed);
        let set = pitches(&[55, 57, 59, 60, 62, 64]);
        let a = generator.generate(&set, &mut rng);
        let b = generator.generate(&set, &mut rng);
        let window = Exercise::stitched(&[a, b]);
        let xml = MusicXmlEncoder::encode_with_breaks(&window, Some(2));
        renderer.render_with(&xml, true).expect("feed render");
        let layout = renderer.toolkit().current_layout().expect("layout");
        for system in &layout.systems {
            // barline_x: the opening barline, then one per measure — the
            // first measure barline of a two-measure system = the mid barline.
            if let Some(&x) = system.barline_x.get(1) {
                positions.push(x);
            }
        }
    }
    let mean = positions.iter().sum::<f64>() / positions.len() as f64;
    let max_dev = positions
        .iter()
        .map(|p| (p - mean).abs())
        .fold(0.0, f64::max);
    assert!(positions.len() >= 20, "got {}", positions.len());
    // Time-linear spacing keeps the mid barline within a few percent of
    // the same spot on every line — the feed reads as fixed lanes.
    assert!(
        max_dev < mean * 0.05,
        "mid-barline wander {max_dev} of {mean}: {positions:?}"
    );
}

/// Per-render option setting (breaks/spacing) must not disturb the
/// fitted page: an auto render after a feed render lays out identically
/// to the one before it.
#[test]
fn feed_renders_dont_disturb_scale_or_page_size() {
    let mut renderer = NotationRenderer::new();
    renderer.fit_view(PAGE_VIEW.0, PAGE_VIEW.1);
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 2;
    let mut rng = SplitMix64::new(2);
    let ex = generator.generate(&pitches(&[60, 62, 64, 65, 67]), &mut rng);
    let page = |renderer: &NotationRenderer| {
        let layout = renderer.toolkit().current_layout().expect("layout");
        (
            layout.width,
            layout.height,
            renderer.system_width(),
            renderer.display_scale(PAGE_VIEW.0, PAGE_VIEW.1),
        )
    };
    renderer
        .render(&MusicXmlEncoder::encode(&ex))
        .expect("auto render");
    let before = page(&renderer);
    renderer
        .render_with(&MusicXmlEncoder::encode_with_breaks(&ex, Some(2)), true)
        .expect("feed render");
    renderer
        .render(&MusicXmlEncoder::encode(&ex))
        .expect("auto render");
    assert_eq!(page(&renderer), before);
}

/// A feed-laid-out 8-bar window: the ids of the first note on the first
/// and second systems, with each system's staff top.
fn two_system_score() -> (NotationController, [(String, f64); 2]) {
    let renderer = Rc::new(RefCell::new(NotationRenderer::new()));
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 8;
    let mut rng = SplitMix64::new(9);
    let ex = generator.generate(&pitches(&[60, 62, 64, 65, 67]), &mut rng);
    let xml = MusicXmlEncoder::encode_with_breaks(&ex, Some(2));
    let rendered = renderer
        .borrow_mut()
        .render_with(&xml, true)
        .expect("feed render");
    let picks = {
        let renderer = renderer.borrow();
        let layout = renderer.toolkit().current_layout().expect("layout");
        assert!(layout.systems.len() >= 2);
        let tops: Vec<f64> = layout.systems.iter().map(|s| s.staff_top).collect();
        let first_on = |system: usize| -> (String, f64) {
            let id = rendered
                .note_ids
                .iter()
                .find(|id| {
                    let &(_, y_top, _, h) = &layout.bounds_by_id[*id];
                    let cy = y_top + h / 2.0;
                    cy > tops[system] - 40.0
                        && tops.get(system + 1).is_none_or(|&next| cy < next - 40.0)
                })
                .expect("a note on the system");
            (id.clone(), tops[system])
        };
        [first_on(0), first_on(1)]
    };
    (NotationController::new(renderer), picks)
}

#[test]
fn follow_top_slides_later_systems_to_the_anchor() {
    let (mut controller, [(first, top0), (second, top1)]) = two_system_score();
    controller.set_follow_top(true);
    // A note going current queues the visibility check (next paint).
    controller.set_state(&first, Some(NoteState::Current));
    assert_eq!(controller.take_pending_visible().as_deref(), Some(&*first));
    // First system anchors the lane: no motion.
    controller.ensure_visible(&first, 1.0, 10.0);
    assert_eq!(controller.slide_offset(), 0.0);
    // Second system slides up by the whole-pixel system distance.
    controller.ensure_visible(&second, 1.0, 20.0);
    let expected = -(top1 - top0).round();
    assert!(expected < 0.0);
    assert_eq!(controller.slide_offset(), expected);
    // Ease-out over 0.4 s from the host clock.
    assert_eq!(controller.slide_offset_on_screen(20.0), 0.0);
    assert_eq!(
        controller.slide_offset_on_screen(20.2),
        (expected * 0.75).round()
    );
    assert_eq!(controller.slide_offset_on_screen(20.4), expected);
    assert_eq!(controller.slide_offset_at(20.4), expected);
    // Staying on the same system never moves; a new score resets the lane.
    controller.ensure_visible(&second, 1.0, 21.0);
    assert_eq!(controller.slide_offset(), expected);
    controller.load_score();
    assert_eq!(controller.slide_offset(), 0.0);
    assert!(controller.take_pending_visible().is_none());
}

/// A ledger note high above a later system's top line still belongs to
/// that system (its stem reaches down to the staff), not to the system
/// above whose below-ledger room it overlaps.
#[test]
fn follow_top_attributes_high_ledger_notes_to_their_own_system() {
    use crate::score::{NoteDuration, ScoreNote};
    let renderer = Rc::new(RefCell::new(NotationRenderer::new()));
    // Two measures, one per system; the second opens on B6 — five staff
    // spaces above the treble staff.
    let mut notes: Vec<ScoreNote> = [60u8, 62, 64, 65]
        .iter()
        .map(|&m| ScoreNote::note(m, NoteDuration::Quarter))
        .collect();
    notes.push(ScoreNote::note(95, NoteDuration::Quarter));
    notes.extend([60u8, 62, 64].iter().map(|&m| ScoreNote::note(m, NoteDuration::Quarter)));
    let ex = Exercise::new(notes, 4);
    let xml = MusicXmlEncoder::encode_with_breaks(&ex, Some(1));
    let rendered = renderer
        .borrow_mut()
        .render_with(&xml, true)
        .expect("feed render");
    let (first, high, top0, top1) = {
        let renderer = renderer.borrow();
        let layout = renderer.toolkit().current_layout().expect("layout");
        assert_eq!(layout.systems.len(), 2);
        let (top0, top1) = (layout.systems[0].staff_top, layout.systems[1].staff_top);
        let high = rendered.note_ids[4].clone();
        let &(_, y_top, _, h) = &layout.bounds_by_id[&high];
        // Sanity: the notehead sits more than four staff spaces above the
        // second system's top line — above any fixed ledger-room margin.
        assert!(y_top + h / 2.0 < top1 - 40.0, "B6 at {} vs top {top1}", y_top + h / 2.0);
        (rendered.note_ids[0].clone(), high, top0, top1)
    };
    let mut controller = NotationController::new(renderer);
    controller.set_follow_top(true);
    controller.ensure_visible(&first, 1.0, 0.0);
    assert_eq!(controller.slide_offset(), 0.0);
    controller.ensure_visible(&high, 1.0, 1.0);
    assert_eq!(controller.slide_offset(), -(top1 - top0).round());
}

#[test]
fn follow_top_off_keeps_the_score_still() {
    let (mut controller, [(first, _), (second, _)]) = two_system_score();
    controller.ensure_visible(&first, 1.0, 0.0);
    controller.ensure_visible(&second, 1.0, 1.0);
    assert_eq!(controller.slide_offset(), 0.0);
    // Turning follow-top on later forgets the remembered system and
    // anchors on the next one.
    controller.set_follow_top(true);
    controller.ensure_visible(&second, 1.0, 2.0);
    assert_eq!(controller.slide_offset(), 0.0);
}
