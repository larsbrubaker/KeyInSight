//! The page-scroll follow rule at the controller level — the `ensureVisible`
//! arithmetic of the Swift page script (`r.top < vh*0.05 || r.bottom >
//! vh*0.7` → `scrollTo(round(scrollY + r.top - vh*0.18))`), the manual
//! override it yields to, the `loadScore` resets, and the follow-top
//! exclusion.

use std::cell::RefCell;
use std::rc::Rc;

use super::{NotationController, NotationRenderer, NoteState};
use crate::core::SplitMix64;
use crate::score::{ExerciseGenerator, MusicXmlEncoder, PitchOption};

const VH: f64 = 400.0;

/// A page of several systems (layout px), with one note id per system
/// and the span (top staff line, bottom staff line) of each.
fn paged_score() -> (NotationController, Vec<(String, f64, f64)>) {
    let renderer = Rc::new(RefCell::new(NotationRenderer::new()));
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 16;
    let mut rng = SplitMix64::new(21);
    let pitches: Vec<PitchOption> = [60, 62, 64, 65, 67]
        .iter()
        .map(|&m| PitchOption::new(m))
        .collect();
    let ex = generator.generate(&pitches, &mut rng);
    let rendered = renderer
        .borrow_mut()
        .render(&MusicXmlEncoder::encode(&ex))
        .expect("render");
    // Narrow page at scale 1: every system is about two measures.
    renderer.borrow_mut().fit_page(420.0, 40.0);
    let systems = {
        let renderer = renderer.borrow();
        let layout = renderer.toolkit().current_layout().expect("layout");
        assert!(
            layout.systems.len() >= 4,
            "{} systems",
            layout.systems.len()
        );
        let tops: Vec<f64> = layout.systems.iter().map(|s| s.staff_top).collect();
        tops.iter()
            .enumerate()
            .map(|(i, &top)| {
                let id = rendered
                    .note_ids
                    .iter()
                    .find(|id| {
                        let &(_, y_top, _, h) = &layout.bounds_by_id[*id];
                        let cy = y_top + h / 2.0;
                        cy > top - 40.0 && tops.get(i + 1).is_none_or(|&next| cy < next - 40.0)
                    })
                    .expect("a note on the system")
                    .clone();
                // A single treble staff: four spaces tall.
                (id, top, top + 40.0)
            })
            .collect::<Vec<_>>()
    };
    (NotationController::new(renderer), systems)
}

#[test]
fn a_system_below_seventy_percent_scrolls_its_top_to_eighteen_percent() {
    let (mut c, systems) = paged_score();
    let (ref id, top, bottom) = systems[2];
    assert!(
        bottom > VH * 0.7,
        "system 2 bottom {bottom} is below the band"
    );
    let target = c.follow_scroll_target(id, 1.0, VH, 0.0);
    assert_eq!(target, Some((top - VH * 0.18).round()));
    assert_eq!(target.unwrap().fract(), 0.0);
}

#[test]
fn a_system_above_five_percent_scrolls_back_into_the_band() {
    let (mut c, systems) = paged_score();
    let (ref id, top, _) = systems[1];
    // Scrolled so the system's top sits 10 px above the viewport.
    let scroll = top + 10.0;
    assert_eq!(
        c.follow_scroll_target(id, 1.0, VH, scroll),
        Some((scroll + (top - scroll) - VH * 0.18).round())
    );
    // Just inside the 5% line: left alone.
    let scroll = top - VH * 0.06;
    assert_eq!(c.follow_scroll_target(id, 1.0, VH, scroll), None);
}

#[test]
fn a_system_inside_the_band_is_left_alone() {
    let (mut c, systems) = paged_score();
    let (ref id, top, _) = systems[2];
    let scroll = top - VH * 0.3; // top at 30%, bottom at 40%
    assert_eq!(c.follow_scroll_target(id, 1.0, VH, scroll), None);
}

#[test]
fn targets_are_whole_pixels_at_fractional_scales_and_never_negative() {
    let (mut c, systems) = paged_score();
    let (ref first, top0, _) = systems[0];
    // The first system is at the top of the page: an 18% placement would
    // be negative — clamped to 0 (and a no-op at scroll 0 since it is in
    // the band; check from further down the page).
    assert!(top0 < VH * 0.18);
    let target = c.follow_scroll_target(first, 1.0, VH, 300.0);
    assert_eq!(target, Some(0.0));
    // A fractional display scale still lands on a whole pixel.
    let (ref id, top, _) = systems[3];
    let scale = 0.8137;
    let target = c
        .follow_scroll_target(id, scale, VH, 0.0)
        .expect("system 3 is below the band");
    assert_eq!(target.fract(), 0.0);
    assert_eq!(target, (top * scale - VH * 0.18).round());
    // Never past the end of the page.
    let content_h = c
        .renderer
        .borrow()
        .toolkit()
        .current_layout()
        .unwrap()
        .height
        * scale;
    let (ref last, _, _) = systems[systems.len() - 1];
    let target = c.follow_scroll_target(last, scale, VH, 0.0).unwrap();
    assert!(target <= (content_h - VH).max(0.0).round());
}

#[test]
fn manual_scroll_yields_until_the_cursor_enters_another_system() {
    let (mut c, systems) = paged_score();
    let (ref on_two, top2, _) = systems[2];
    let (ref on_three, top3, _) = systems[3];
    // The cursor is on system 2; the user wheels away.
    c.set_state(on_two, Some(NoteState::Current));
    c.note_user_scroll();
    assert_eq!(c.user_scroll_system(), Some(2));
    // Out of the band, but the user holds system 2: no follow.
    assert_eq!(c.follow_scroll_target(on_two, 1.0, VH, 0.0), None);
    assert_eq!(c.user_scroll_system(), Some(2));
    // The cursor moves on to system 3: the override releases and the
    // follow re-engages on this very call.
    assert_eq!(
        c.follow_scroll_target(on_three, 1.0, VH, 0.0),
        Some((top3 - VH * 0.18).round())
    );
    assert_eq!(c.user_scroll_system(), None);
    let _ = top2;
    // No current note when the user scrolls: nothing is held.
    c.set_state(on_two, None);
    c.note_user_scroll();
    assert_eq!(c.user_scroll_system(), None);
    assert!(c.follow_scroll_target(on_two, 1.0, VH, 0.0).is_some());
}

#[test]
fn load_score_resets_the_override_and_scrolls_to_the_top() {
    let (mut c, systems) = paged_score();
    let (ref id, _, _) = systems[2];
    c.set_state(id, Some(NoteState::Current));
    c.note_user_scroll();
    assert_eq!(c.user_scroll_system(), Some(2));
    assert!(!c.take_scroll_reset());
    c.load_score();
    assert_eq!(c.user_scroll_system(), None);
    assert!(c.take_scroll_reset());
    assert!(!c.take_scroll_reset(), "taken once");
}

#[test]
fn follow_top_disables_the_page_follow() {
    let (mut c, systems) = paged_score();
    let (ref id, _, bottom) = systems[2];
    assert!(bottom > VH * 0.7);
    assert!(c.follow_scroll_target(id, 1.0, VH, 0.0).is_some());
    c.set_state(id, Some(NoteState::Current));
    c.note_user_scroll();
    c.set_follow_top(true);
    // The switch clears the override and zeroes the widget's scroll...
    assert_eq!(c.user_scroll_system(), None);
    assert!(c.take_scroll_reset());
    // ...and while follow-top is on, no system ever asks for a scroll.
    for (id, _, _) in &systems {
        for scroll in [0.0, 150.0, 900.0] {
            assert_eq!(c.follow_scroll_target(id, 1.0, VH, scroll), None);
        }
    }
    c.set_follow_top(false);
    assert!(c.follow_scroll_target(id, 1.0, VH, 0.0).is_some());
}
