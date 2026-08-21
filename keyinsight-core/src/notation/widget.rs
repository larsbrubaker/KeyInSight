//! The agg-gui widget that paints the engraved score with its feedback
//! overlays. Replaces the Swift `NotationView`/WKWebView: the score paints
//! on a light page (music is always light — docs/platform-substitutions.md),
//! per-note colors come from the controller's states, and the ghost /
//! ticks / follow cursor are ordinary painting.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::draw_ctx::DrawCtx;
use agg_gui::event::{Event, EventResult, MouseButton};
use agg_gui::geometry::{Point, Rect, Size};
use agg_gui::text::Font;
use agg_gui::widget::Widget;

use crate::notation::{NotationController, NoteState};

/// Whole device pixels for every vertical content offset. The Swift page
/// learned this the hard way: settling on a fractional offset knocks staff
/// lines off the pixel grid — a 1px black line painted as 2px of gray. Its
/// keep-in-upper-third scroll (`r.top < vh*0.05 || r.bottom > vh*0.7` →
/// put the system's top at `vh*0.18`, rounded) has no work to do here —
/// the widget fits the whole score to its viewport — but the rounding rule
/// applies to the paint origin and the follow-top slide alike.
pub fn whole_px(offset: f64) -> f64 {
    offset.round()
}

/// Where the engraving sits in the widget: display scale, and the content
/// origin (widget px, y-up) of the score box's bottom-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Placement {
    scale: f64,
    offset_x: f64,
    origin_y: f64,
    score_h: f64,
}

pub struct NotationWidget {
    controller: Rc<RefCell<NotationController>>,
    /// Host clock for the follow schedule (injected so native and WASM
    /// share code; seconds, monotonic).
    now: Rc<dyn Fn() -> f64>,
    music_font: Arc<Font>,
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
}

impl NotationWidget {
    pub fn new(controller: Rc<RefCell<NotationController>>, now: Rc<dyn Fn() -> f64>) -> Self {
        Self {
            controller,
            now,
            music_font: verovio_rust::leipzig_font(),
            bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            children: Vec::new(),
        }
    }

    /// Fit the engraving to the widget, centered horizontally, capped so
    /// small exercises don't balloon; the follow-top slide (whole px, CSS
    /// sign: negative = up) moves the whole score box.
    fn placement(&self, slide_offset: f64) -> Option<Placement> {
        let controller = self.controller.borrow();
        let renderer = controller.renderer.borrow();
        let layout = renderer.toolkit().current_layout()?;
        let (width, height) = (self.bounds.width, self.bounds.height);
        let scale = renderer.display_scale(width, height)?;
        let score_h = layout.height;
        Some(Placement {
            scale,
            offset_x: (width - layout.width * scale) / 2.0,
            origin_y: whole_px(height - score_h * scale) - slide_offset,
            score_h,
        })
    }
}

impl Widget for NotationWidget {
    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: Size) -> Size {
        // Reflow long scores into systems fitted to this viewport.
        if available.width > 0.0 && available.height > 0.0 {
            if let Ok(controller) = self.controller.try_borrow() {
                if let Ok(mut renderer) = controller.renderer.try_borrow_mut() {
                    renderer.fit_view(available.width, available.height);
                }
            }
        }
        available
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let width = self.bounds.width;
        let height = self.bounds.height;

        // The page: music always renders light regardless of app theme.
        ctx.set_fill_color(Color::white());
        ctx.begin_path();
        ctx.rect(0.0, 0.0, width, height);
        ctx.fill();

        let now = (self.now)();
        // Follow overlay: while playback-following, the scheduled group
        // paints as current on top of the stored states. Then the
        // follow-top lane: a note that just went current may slide its
        // system to the top.
        let pending = self.controller.borrow_mut().take_pending_visible();
        if let (Some(id), Some(placement)) = (pending, self.placement(0.0)) {
            self.controller
                .borrow_mut()
                .ensure_visible(&id, placement.scale, now);
        }
        let (follow_ids, slide_offset) = {
            let mut controller = self.controller.borrow_mut();
            (
                controller.follow_ids_at(now),
                controller.slide_offset_at(now),
            )
        };
        if self.controller.borrow().is_following() {
            agg_gui::animation::request_draw(); // keep the cursor moving
        }
        let Some(Placement {
            scale,
            offset_x,
            origin_y,
            score_h,
        }) = self.placement(slide_offset)
        else {
            return;
        };

        ctx.save();
        // A slid score leaves the widget through its top edge.
        ctx.clip_rect(0.0, 0.0, width, height);
        ctx.translate(offset_x, origin_y);
        ctx.scale(scale, scale);

        let controller = self.controller.borrow();
        let renderer = controller.renderer.borrow();
        let toolkit = renderer.toolkit();
        let Some(layout) = toolkit.current_layout() else {
            ctx.restore();
            return;
        };

        let mut options = verovio_rust::RenderOptions::default();
        for element in &layout.elements {
            let Some(id) = &element.id else { continue };
            if let Some(state) = controller.state_of(id) {
                options.overrides.insert(id.clone(), state.color());
            }
        }
        if let Some(ids) = &follow_ids {
            for id in ids {
                options
                    .overrides
                    .insert(id.clone(), NoteState::Current.color());
            }
        }

        // The toolkit draws y-up given the top edge of the score box.
        toolkit.render(ctx, &self.music_font, 0.0, score_h, &options);

        // Ghost note: gray notehead at the played staff position, aligned
        // with the expected note (ports the HTML overlay math — half a
        // staff space per diatonic step).
        if let Some(ghost) = controller.ghost() {
            if let Some((x, y_top, w, h)) = toolkit.element_bounds(&ghost.expected_id) {
                let staff_space = 10.0; // LayoutOptions::default().staff_space
                let cx = x + w / 2.0;
                let cy_down = y_top + h / 2.0 - ghost.offset_steps as f64 * staff_space / 2.0;
                let cy = score_h - cy_down;
                let gray = Color::from_rgba8(0x8A, 0x8A, 0x8A, 64);
                ctx.set_fill_color(gray);
                ctx.begin_path();
                ctx.circle(cx, cy, w * 0.5);
                ctx.fill();
                ctx.set_stroke_color(Color::from_rgb8(0x8A, 0x8A, 0x8A));
                ctx.set_line_width(2.0);
                ctx.begin_path();
                ctx.circle(cx, cy, w * 0.5);
                ctx.stroke();
            }
        }

        // Timing ticks: ◂ early / ▸ late above the note.
        for tick in controller.ticks() {
            if let Some((x, y_top, w, _h)) = toolkit.element_bounds(&tick.id) {
                let color = Color::from_rgb8(0xB8, 0x86, 0x0B);
                ctx.set_fill_color(color);
                ctx.set_font(Arc::clone(&self.music_font));
                let cx = x + w / 2.0;
                let cy = score_h - (y_top - 14.0);
                // Simple triangle glyphs drawn as paths (the UI font isn't
                // loaded here; a filled triangle reads identically).
                let s = 5.0;
                ctx.begin_path();
                if tick.early {
                    ctx.move_to(cx + s, cy + s);
                    ctx.line_to(cx - s, cy);
                    ctx.line_to(cx + s, cy - s);
                } else {
                    ctx.move_to(cx - s, cy + s);
                    ctx.line_to(cx + s, cy);
                    ctx.line_to(cx - s, cy - s);
                }
                ctx.close_path();
                ctx.fill();
            }
        }

        ctx.restore();
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseMove { pos } => self.route_hover(*pos),
            Event::MouseDown {
                pos,
                button: MouseButton::Left,
                ..
            } => {
                if self.route_click(*pos) {
                    return EventResult::Consumed;
                }
            }
            _ => {}
        }
        EventResult::Ignored
    }

    fn type_name(&self) -> &'static str {
        "NotationWidget"
    }
}

impl NotationWidget {
    /// Padded notehead hit boxes, nearest center wins (ports the Swift
    /// page's `noteHitAt`). Hit geometry follows the slide transform, like
    /// the page rebuilding its rects at `transitionend`.
    fn note_hit_at(&self, pos: Point) -> Option<String> {
        let slide_offset = self
            .controller
            .borrow()
            .slide_offset_on_screen((self.now)());
        let placement = self.placement(slide_offset)?;
        let controller = self.controller.borrow();
        let renderer = controller.renderer.borrow();
        let layout = renderer.toolkit().current_layout()?;
        // Widget y-up → layout y-down.
        let lx = (pos.x - placement.offset_x) / placement.scale;
        let ly = placement.score_h - (pos.y - placement.origin_y) / placement.scale;
        const HIT_PAD: f64 = 10.0;
        let mut best: Option<(String, f64)> = None;
        for (id, &(x, y_top, w, h)) in &layout.bounds_by_id {
            if lx < x - HIT_PAD
                || lx > x + w + HIT_PAD
                || ly < y_top - HIT_PAD
                || ly > y_top + h + HIT_PAD
            {
                continue;
            }
            let cx = x + w / 2.0;
            let cy = y_top + h / 2.0;
            let d = (lx - cx).powi(2) + (ly - cy).powi(2);
            if best.as_ref().map(|(_, bd)| d < *bd).unwrap_or(true) {
                best = Some((id.clone(), d));
            }
        }
        best.map(|(id, _)| id)
    }

    /// Hover-to-name (the precise per-kind fallback arrives with non-note
    /// element hovers).
    fn route_hover(&self, pos: Point) {
        let hit = self.note_hit_at(pos);
        let controller = self.controller.borrow();
        match hit {
            Some(id) => {
                let kind = if id.starts_with("rest-") {
                    "rest"
                } else {
                    "note"
                };
                controller.send_hover(kind, &id)
            }
            None => controller.send_hover("", ""),
        }
    }

    /// Clicking a note reports it (repertoire: practice-from-here) — the
    /// same padded hit boxes as hover. True when a note was hit.
    fn route_click(&self, pos: Point) -> bool {
        let Some(id) = self.note_hit_at(pos) else {
            return false;
        };
        self.controller.borrow().send_click(&id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::SplitMix64;
    use crate::score::{ExerciseGenerator, MusicXmlEncoder, PitchOption};
    use agg_gui::event::Modifiers;

    #[test]
    fn route_click_reports_the_padded_note_under_the_pointer() {
        let renderer = Rc::new(RefCell::new(crate::notation::NotationRenderer::new()));
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
        let target = rendered.note_ids[1].clone();

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
}
