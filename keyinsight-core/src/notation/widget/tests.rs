//! Widget-level coverage: hit routing, and the scrolling page — the fit
//! decision, the keep-in-upper-third glide, manual override, whole-pixel
//! offsets, and the follow-top exclusion.

use super::*;
use crate::core::SplitMix64;
use crate::notation::NotationRenderer;
use crate::score::{ExerciseGenerator, MusicXmlEncoder, PitchOption};
use agg_gui::event::Modifiers;

fn short_exercise() -> (Rc<RefCell<NotationRenderer>>, Vec<String>) {
    let renderer = Rc::new(RefCell::new(NotationRenderer::new()));
    let mut generator = ExerciseGenerator::default();
    generator.config.measures = 2;
    let mut rng = SplitMix64::new(4);
    let pitches: Vec<PitchOption> = [60, 62, 64, 65, 67]
        .iter()
        .map(|&m| PitchOption::new(m))
        .collect();
    let ex = generator.generate(&pitches, &mut rng);
    let rendered = renderer
        .borrow_mut()
        .render(&MusicXmlEncoder::encode(&ex))
        .expect("render");
    (renderer, rendered.note_ids)
}

/// A long piece: many systems at any sane width.
fn long_piece() -> (Rc<RefCell<NotationRenderer>>, Vec<String>) {
    let xml = String::from_utf8_lossy(include_bytes!(
        "../../../assets/pieces/gymnopedie-1.musicxml"
    ))
    .into_owned();
    let renderer = Rc::new(RefCell::new(NotationRenderer::new()));
    let rendered = renderer
        .borrow_mut()
        .render(&xml)
        .expect("gymnopedie engraves");
    (renderer, rendered.note_ids)
}

fn widget_for(
    renderer: &Rc<RefCell<NotationRenderer>>,
    fit: NotationFit,
    size: (f64, f64),
    clock: Rc<std::cell::Cell<f64>>,
) -> (NotationWidget, Rc<RefCell<NotationController>>) {
    let controller = Rc::new(RefCell::new(NotationController::new(Rc::clone(renderer))));
    let now: Rc<dyn Fn() -> f64> = Rc::new(move || clock.get());
    let mut widget = NotationWidget::new(Rc::clone(&controller), now).with_fit(fit);
    widget.set_bounds(Rect::new(0.0, 0.0, size.0, size.1));
    widget.layout(Size::new(size.0, size.1));
    (widget, controller)
}

/// Paint one frame into an offscreen framebuffer (the real paint path).
fn paint(widget: &mut NotationWidget) {
    let (w, h) = (
        widget.bounds.width.ceil() as u32,
        widget.bounds.height.ceil() as u32,
    );
    let mut framebuffer = agg_gui::framebuffer::Framebuffer::new(w, h);
    let mut ctx = agg_gui::gfx_ctx::GfxCtx::new(&mut framebuffer);
    widget.paint(&mut ctx);
}

/// Layout y-down of a note's center, and the index of its system.
fn note_system(renderer: &Rc<RefCell<NotationRenderer>>, id: &str) -> (f64, usize) {
    let renderer = renderer.borrow();
    let layout = renderer.toolkit().current_layout().expect("layout");
    let &(_, y_top, _, h) = &layout.bounds_by_id[id];
    let cy = y_top + h / 2.0;
    let system = layout
        .systems
        .iter()
        .rev()
        .find(|s| s.staff_top - 40.0 <= cy)
        .map_or(0, |s| s.index);
    (cy, system)
}

/// The index of the first system whose staff top lies past `min_top`
/// layout px.
fn system_past(renderer: &Rc<RefCell<NotationRenderer>>, min_top: f64) -> usize {
    let renderer = renderer.borrow();
    let layout = renderer.toolkit().current_layout().expect("layout");
    layout
        .systems
        .iter()
        .find(|s| s.staff_top > min_top)
        .expect("a system past min_top")
        .index
}

/// A note on system `index`.
fn note_on_system(
    renderer: &Rc<RefCell<NotationRenderer>>,
    ids: &[String],
    index: usize,
) -> String {
    ids.iter()
        .find(|id| note_system(renderer, id).1 == index)
        .cloned()
        .expect("a note on that system")
}

#[test]
fn route_click_reports_the_padded_note_under_the_pointer() {
    let (renderer, note_ids) = short_exercise();
    let target = note_ids[1].clone();
    let controller = Rc::new(RefCell::new(NotationController::new(Rc::clone(&renderer))));
    let clicked: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&clicked);
    controller.borrow_mut().on_note_click =
        Some(Box::new(move |id| sink.borrow_mut().push(id.to_string())));

    let mut widget = NotationWidget::new(controller, Rc::new(|| 0.0));
    widget.set_bounds(Rect::new(0.0, 0.0, 700.0, 300.0));
    let placement = widget.placement(0.0).expect("placement");
    let (x, y_top, w, h) = renderer.borrow().toolkit().element_bounds(&target).unwrap();
    // Layout y-down → widget y-up, just inside the padded box.
    let sx = placement.offset_x + (x + w / 2.0) * placement.scale + 4.0;
    let sy = placement.origin_y + (placement.score_h - (y_top + h / 2.0)) * placement.scale;
    let click = |widget: &mut NotationWidget, pos: Point| {
        widget.on_event(&Event::MouseDown {
            pos,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        })
    };
    assert!(matches!(
        click(&mut widget, Point::new(sx, sy)),
        EventResult::Consumed
    ));
    assert_eq!(*clicked.borrow(), vec![target.clone()]);
    // Empty page: nothing reported, event left to bubble.
    assert!(matches!(
        click(&mut widget, Point::new(2.0, 2.0)),
        EventResult::Ignored
    ));
    assert_eq!(clicked.borrow().len(), 1);
    // Hover uses the same boxes.
    widget.route_hover(Point::new(sx, sy));
}

#[test]
fn short_exercises_fit_and_long_pieces_page_in_training() {
    let clock = Rc::new(std::cell::Cell::new(0.0));
    let (renderer, _) = short_exercise();
    let (widget, _) = widget_for(
        &renderer,
        NotationFit::Fit,
        (700.0, 300.0),
        Rc::clone(&clock),
    );
    assert_eq!(widget.page_scale(), None, "a two-bar exercise fits");
    assert_eq!(widget.scroll_offset(), 0.0);

    let (renderer, _) = long_piece();
    let (mut widget, _) = widget_for(
        &renderer,
        NotationFit::Fit,
        (700.0, 300.0),
        Rc::clone(&clock),
    );
    let scale = widget.page_scale().expect("gymnopedie pages");
    assert_eq!(scale, renderer.borrow().reading_scale(READING_STAFF_PX));
    // Page: wrapped inside the widget width, taller than the viewport.
    let layout_size = {
        let renderer = renderer.borrow();
        let layout = renderer.toolkit().current_layout().unwrap();
        (layout.width * scale, layout.height * scale)
    };
    assert!(layout_size.0 <= 700.0, "width {}", layout_size.0);
    assert!(layout_size.1 > 300.0, "height {}", layout_size.1);
    paint(&mut widget);
    assert_eq!(
        widget.scroll_offset(),
        0.0,
        "a fresh page starts at the top"
    );
    assert!(widget.scroll.can_scroll(300.0));

    // The Progress staves always page, even when they would fit.
    let (renderer, _) = short_exercise();
    let (widget, _) = widget_for(&renderer, NotationFit::Page, (700.0, 300.0), clock);
    assert!(widget.page_scale().is_some());
}

#[test]
fn current_note_below_the_band_glides_its_system_to_eighteen_percent() {
    let clock = Rc::new(std::cell::Cell::new(10.0));
    let (renderer, ids) = long_piece();
    let (mut widget, controller) = widget_for(
        &renderer,
        NotationFit::Fit,
        (700.0, 300.0),
        Rc::clone(&clock),
    );
    let scale = widget.page_scale().unwrap();
    paint(&mut widget);
    // A note well below the viewport goes current.
    let target = note_on_system(&renderer, &ids, system_past(&renderer, 400.0 / scale));
    let (_, system) = note_system(&renderer, &target);
    let system_top = renderer
        .borrow()
        .toolkit()
        .current_layout()
        .unwrap()
        .systems[system]
        .staff_top;
    controller
        .borrow_mut()
        .set_state(&target, Some(NoteState::Current));
    paint(&mut widget);
    // The glide has begun but the first frame is still at 0...
    assert!(widget.scroll.is_gliding());
    assert_eq!(widget.scroll_offset(), 0.0);
    let expected = (system_top * scale - 300.0 * 0.18).round();
    assert_eq!(widget.scroll.target(), expected);
    // ...eases out over 0.4 s in whole pixels...
    clock.set(10.2);
    paint(&mut widget);
    assert_eq!(widget.scroll_offset(), (expected * 0.75).round());
    assert_eq!(widget.scroll_offset().fract(), 0.0);
    clock.set(10.5);
    paint(&mut widget);
    assert_eq!(widget.scroll_offset(), expected);
    assert!(!widget.scroll.is_gliding());
    // ...and hit geometry follows the scroll: the target note is now on
    // screen, 18% down from the top, and clicking there reports it.
    let clicked: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&clicked);
    controller.borrow_mut().on_note_click =
        Some(Box::new(move |id| sink.borrow_mut().push(id.to_string())));
    let placement = widget.placement(widget.scroll_offset()).unwrap();
    let (x, y_top, w, h) = renderer.borrow().toolkit().element_bounds(&target).unwrap();
    let sx = placement.offset_x + (x + w / 2.0) * placement.scale;
    let sy = placement.origin_y + (placement.score_h - (y_top + h / 2.0)) * placement.scale;
    assert!(sy > 0.0 && sy < 300.0, "note on screen at y {sy}");
    widget.on_event(&Event::MouseDown {
        pos: Point::new(sx, sy),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
    });
    assert_eq!(*clicked.borrow(), vec![target.clone()]);
    // Another note on the same system, now inside the band: no motion.
    let neighbour = ids
        .iter()
        .find(|id| *id != &target && note_system(&renderer, id).1 == system)
        .expect("a second note on the system")
        .clone();
    controller
        .borrow_mut()
        .set_state(&neighbour, Some(NoteState::Current));
    paint(&mut widget);
    assert!(!widget.scroll.is_gliding());
    assert_eq!(widget.scroll_offset(), expected);
}

#[test]
fn wheel_hands_control_to_the_user_until_the_cursor_moves_on() {
    let clock = Rc::new(std::cell::Cell::new(0.0));
    let (renderer, ids) = long_piece();
    let (mut widget, controller) = widget_for(
        &renderer,
        NotationFit::Fit,
        (700.0, 300.0),
        Rc::clone(&clock),
    );
    let scale = widget.page_scale().unwrap();
    paint(&mut widget);
    let first = ids[0].clone();
    controller
        .borrow_mut()
        .set_state(&first, Some(NoteState::Current));
    paint(&mut widget);
    assert_eq!(widget.scroll_offset(), 0.0, "first system already in view");
    // Wheel back: see content below — jumps a notch, grabs control.
    let wheel = Event::MouseWheel {
        pos: Point::new(50.0, 50.0),
        delta_y: -2.0,
        delta_x: 0.0,
        modifiers: Modifiers::default(),
    };
    assert!(matches!(widget.on_event(&wheel), EventResult::Consumed));
    assert_eq!(widget.scroll_offset(), 80.0);
    assert_eq!(controller.borrow().user_scroll_system(), Some(0));
    // Scrolled well away; the next note on the SAME system does not pull
    // the page back.
    for _ in 0..10 {
        widget.on_event(&wheel);
    }
    let held = widget.scroll_offset();
    assert!(held > 300.0 * 0.7);
    let same_system = ids
        .iter()
        .find(|id| *id != &first && note_system(&renderer, id).1 == 0)
        .unwrap()
        .clone();
    controller
        .borrow_mut()
        .set_state(&same_system, Some(NoteState::Current));
    paint(&mut widget);
    assert!(!widget.scroll.is_gliding());
    assert_eq!(widget.scroll_offset(), held);
    // A note on a different system re-engages the follow.
    let next = note_on_system(&renderer, &ids, 1);
    controller
        .borrow_mut()
        .set_state(&next, Some(NoteState::Current));
    paint(&mut widget);
    assert_eq!(controller.borrow().user_scroll_system(), None);
    assert!(widget.scroll.is_gliding());
    let (_, system) = note_system(&renderer, &next);
    let top = renderer
        .borrow()
        .toolkit()
        .current_layout()
        .unwrap()
        .systems[system]
        .staff_top;
    assert_eq!(
        widget.scroll.target(),
        (top * scale - 300.0 * 0.18).round().max(0.0)
    );
    // A new score: back to the top, override gone.
    controller.borrow_mut().load_score();
    paint(&mut widget);
    assert_eq!(widget.scroll_offset(), 0.0);
    assert!(!widget.scroll.is_gliding());
    assert_eq!(controller.borrow().user_scroll_system(), None);
}

#[test]
fn follow_top_forces_the_fitted_view_and_freezes_the_scroll() {
    let clock = Rc::new(std::cell::Cell::new(0.0));
    let (renderer, ids) = long_piece();
    let (mut widget, controller) = widget_for(
        &renderer,
        NotationFit::Page,
        (700.0, 300.0),
        Rc::clone(&clock),
    );
    assert!(widget.page_scale().is_some());
    paint(&mut widget);
    widget.on_event(&Event::MouseWheel {
        pos: Point::new(50.0, 50.0),
        delta_y: -3.0,
        delta_x: 0.0,
        modifiers: Modifiers::default(),
    });
    assert_eq!(widget.scroll_offset(), 120.0);
    // Survival: the feed slides by transform, never by page scroll.
    controller.borrow_mut().set_follow_top(true);
    widget.layout(Size::new(700.0, 300.0));
    assert_eq!(widget.page_scale(), None);
    assert_eq!(
        renderer.borrow().page_scale(),
        None,
        "the layout dropped to the fitted view"
    );
    paint(&mut widget);
    assert_eq!(widget.scroll_offset(), 0.0);
    assert_eq!(controller.borrow().user_scroll_system(), None);
    // No page to scroll: the wheel passes through, the offset stays 0.
    let result = widget.on_event(&Event::MouseWheel {
        pos: Point::new(50.0, 50.0),
        delta_y: -3.0,
        delta_x: 0.0,
        modifiers: Modifiers::default(),
    });
    assert!(matches!(result, EventResult::Ignored));
    assert_eq!(widget.scroll_offset(), 0.0);
    let far = note_on_system(&renderer, &ids, 1);
    controller
        .borrow_mut()
        .set_state(&far, Some(NoteState::Current));
    paint(&mut widget);
    assert_eq!(widget.scroll_offset(), 0.0);
    // Off again: the page comes back at the top.
    controller.borrow_mut().set_follow_top(false);
    widget.layout(Size::new(700.0, 300.0));
    assert!(widget.page_scale().is_some());
    paint(&mut widget);
    assert_eq!(widget.scroll_offset(), 0.0);
}

#[test]
fn playback_follow_glides_the_page_along_with_the_cursor() {
    // Hear It on a paged piece: the cursor is painted by override, never
    // by `Current` states, so the follow itself must drive ensureVisible.
    let clock = Rc::new(std::cell::Cell::new(10.0));
    let (renderer, ids) = long_piece();
    let (mut widget, controller) = widget_for(
        &renderer,
        NotationFit::Fit,
        (700.0, 300.0),
        Rc::clone(&clock),
    );
    let scale = widget.page_scale().unwrap();
    paint(&mut widget);
    let first = ids[0].clone();
    let far = note_on_system(&renderer, &ids, system_past(&renderer, 400.0 / scale));
    let (_, far_system) = note_system(&renderer, &far);
    let far_top = renderer
        .borrow()
        .toolkit()
        .current_layout()
        .unwrap()
        .systems[far_system]
        .staff_top;
    controller.borrow_mut().follow_schedule(
        vec![vec![first.clone()], vec![far.clone()]],
        vec![0.0, 1.0],
        10.0,
    );
    // Group 0: the first system is already in view — no motion.
    paint(&mut widget);
    assert_eq!(controller.borrow().follow_log(), [0]);
    assert!(!widget.scroll.is_gliding());
    assert_eq!(widget.scroll_offset(), 0.0);
    // Group 1 lands below the band: the page glides its system to 18%.
    clock.set(11.0);
    paint(&mut widget);
    assert_eq!(controller.borrow().follow_log(), [0, 1]);
    assert!(widget.scroll.is_gliding());
    assert_eq!(
        widget.scroll.target(),
        (far_top * scale - 300.0 * 0.18).round()
    );
    clock.set(11.5);
    paint(&mut widget);
    assert!(!widget.scroll.is_gliding());
    assert_eq!(widget.scroll_offset(), (far_top * scale - 300.0 * 0.18).round());
    // A wheel during playback hands control to the user for the rest of
    // the cursor's system, exactly like a `Current` note would.
    widget.on_event(&Event::MouseWheel {
        pos: Point::new(50.0, 50.0),
        delta_y: 2.0,
        delta_x: 0.0,
        modifiers: Modifiers::default(),
    });
    assert_eq!(controller.borrow().user_scroll_system(), Some(far_system));
}
