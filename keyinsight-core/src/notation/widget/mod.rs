//! The agg-gui widget that paints the engraved score with its feedback
//! overlays. Replaces the Swift `NotationView`/WKWebView: the score paints
//! on a light page (music is always light — docs/platform-substitutions.md),
//! per-note colors come from the controller's states, and the ghost /
//! ticks / follow cursor are ordinary painting. Long scores lay out as a
//! page the widget scrolls itself (`scroll.rs`), keeping the current
//! system in the upper third like the Swift page's `ensureVisible`.

mod scroll;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::draw_ctx::DrawCtx;
use agg_gui::event::{Event, EventResult, MouseButton};
use agg_gui::geometry::{Point, Rect, Size};
use agg_gui::text::Font;
use agg_gui::widget::Widget;

use crate::notation::renderer::READING_STAFF_PX;
use crate::notation::{NotationController, NoteState};
use scroll::{PageScroll, ScrollOutcome};

/// Whole device pixels for every vertical content offset. The Swift page
/// learned this the hard way: settling on a fractional offset knocks staff
/// lines off the pixel grid — a 1px black line painted as 2px of gray. The
/// rounding rule applies to the paint origin, the page scroll, and the
/// follow-top slide alike.
pub fn whole_px(offset: f64) -> f64 {
    offset.round()
}

/// How the widget fits the engraving to its viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotationFit {
    /// Fit the whole score to the view (training); when the fitted score
    /// would read smaller than the reading scale, it pages instead.
    #[default]
    Fit,
    /// Always a page at the reading scale, scrolled by the widget (the
    /// Progress heat staves — `NotationController(scrollable: true)`).
    Page,
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
    fit: NotationFit,
    scroll: PageScroll,
}

impl NotationWidget {
    pub fn new(controller: Rc<RefCell<NotationController>>, now: Rc<dyn Fn() -> f64>) -> Self {
        Self {
            controller,
            now,
            music_font: verovio_rust::leipzig_font(),
            bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            children: Vec::new(),
            fit: NotationFit::default(),
            scroll: PageScroll::new(),
        }
    }

    pub fn with_fit(mut self, fit: NotationFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn fit(&self) -> NotationFit {
        self.fit
    }

    /// The page scroll offset on screen (content px from the top, whole
    /// px); 0 in a fitted view.
    pub fn scroll_offset(&self) -> f64 {
        self.scroll.offset()
    }

    /// Some(display scale) while the score is a scrolling page. Follow-top
    /// (survival) slides by transform and never pages — structurally: the
    /// layout drops to the fitted view and this reads `None`.
    fn page_scale(&self) -> Option<f64> {
        let controller = self.controller.borrow();
        if controller.follow_top() {
            return None;
        }
        let renderer = controller.renderer.borrow();
        renderer.page_scale()
    }

    /// Fit the engraving to the widget, centered horizontally, capped so
    /// small exercises don't balloon — or, on a page, at the reading scale
    /// — then shifted by `shift` widget px (positive = content moves up:
    /// the page scroll, or the negated follow-top slide).
    fn placement(&self, shift: f64) -> Option<Placement> {
        let page_scale = self.page_scale();
        let controller = self.controller.borrow();
        let renderer = controller.renderer.borrow();
        let layout = renderer.toolkit().current_layout()?;
        let (width, height) = (self.bounds.width, self.bounds.height);
        let scale = match page_scale {
            Some(scale) => scale,
            None => renderer.display_scale(width, height)?,
        };
        let score_h = layout.height;
        Some(Placement {
            scale,
            offset_x: (width - layout.width * scale) / 2.0,
            origin_y: whole_px(height - score_h * scale) + whole_px(shift),
            score_h,
        })
    }

    /// The vertical shift the score is painted with right now.
    fn shift_on_screen(&self, now: f64) -> f64 {
        if self.page_scale().is_some() {
            self.scroll.offset()
        } else {
            -self.controller.borrow().slide_offset_on_screen(now)
        }
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
        // Reflow long scores into systems fitted to this viewport — or,
        // on a page, wrapped at its width.
        if available.width > 0.0 && available.height > 0.0 {
            if let Ok(controller) = self.controller.try_borrow() {
                if let Ok(mut renderer) = controller.renderer.try_borrow_mut() {
                    let (w, h) = (available.width, available.height);
                    if controller.follow_top() {
                        renderer.fit_view(w, h);
                    } else {
                        match self.fit {
                            NotationFit::Fit => renderer.fit_auto(w, h, READING_STAFF_PX),
                            NotationFit::Page => {
                                renderer.fit_page(w, READING_STAFF_PX);
                            }
                        }
                    }
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
        let (follow_ids, shift) = self.prepare_frame(now);
        let page_scale = self.page_scale();
        let Some(Placement {
            scale,
            offset_x,
            origin_y,
            score_h,
        }) = self.placement(shift)
        else {
            return;
        };

        ctx.save();
        // A slid or scrolled score leaves the widget through its edges.
        ctx.clip_rect(0.0, 0.0, width, height);
        ctx.translate(offset_x, origin_y);
        ctx.scale(scale, scale);

        {
            let controller = self.controller.borrow();
            let renderer = controller.renderer.borrow();
            let toolkit = renderer.toolkit();
            if let Some(layout) = toolkit.current_layout() {
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
                paint_overlays(ctx, &controller, toolkit, &self.music_font, score_h);
            }
        }

        ctx.restore();

        // The scrollbar rides over the page, like every ScrollView's.
        if page_scale.is_some() && self.scroll.paint(ctx, self.bounds) {
            agg_gui::animation::request_draw();
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if self.page_scale().is_some() {
            match self.scroll.on_event(event, self.bounds) {
                ScrollOutcome::Consumed { by_user } => {
                    if by_user {
                        // Manual scroll hands control to the user for the
                        // rest of the current system.
                        self.controller.borrow_mut().note_user_scroll();
                    }
                    return EventResult::Consumed;
                }
                ScrollOutcome::Ignored => {}
            }
        }
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

/// Ghost note and timing ticks, painted in score space (y-up, the score
/// box's top edge at `score_h`).
fn paint_overlays(
    ctx: &mut dyn DrawCtx,
    controller: &NotationController,
    toolkit: &verovio_rust::Toolkit,
    music_font: &Arc<Font>,
    score_h: f64,
) {
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
            ctx.set_font(Arc::clone(music_font));
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
}

impl NotationWidget {
    /// Everything a painted frame settles before the score is drawn: the
    /// page scroll (reset on a new score, content height, the glide in
    /// flight), the note that just went current — on a page it may glide
    /// its system into the upper third, in the follow-top lane it may
    /// slide its system to the top — and the playback-follow cursor.
    /// Returns the follow ids to paint as current and the vertical shift
    /// (widget px, positive = content moves up) the score paints with.
    fn prepare_frame(&mut self, now: f64) -> (Option<Vec<String>>, f64) {
        let height = self.bounds.height;
        let page_scale = self.page_scale();
        if self.controller.borrow_mut().take_scroll_reset() {
            self.scroll.reset();
        }
        if let Some(scale) = page_scale {
            let content_h = {
                let controller = self.controller.borrow();
                let renderer = controller.renderer.borrow();
                renderer
                    .toolkit()
                    .current_layout()
                    .map_or(0.0, |layout| layout.height * scale)
            };
            self.scroll.set_content(content_h, height);
        } else {
            self.scroll.reset(); // fitted view / follow-top: no page scroll
        }
        if self.scroll.advance(now, height) {
            agg_gui::animation::request_draw();
        }

        // The follow cursor first: reaching a new group queues its first
        // id for the same ensureVisible path a `Current` note takes, so
        // the Hear It playback glides the page along.
        let follow_ids = self.controller.borrow_mut().follow_ids_at(now);
        let pending = self.controller.borrow_mut().take_pending_visible();
        if let Some(id) = pending {
            match page_scale {
                Some(scale) => {
                    let target = self.controller.borrow_mut().follow_scroll_target(
                        &id,
                        scale,
                        height,
                        self.scroll.offset(),
                    );
                    if let Some(target) = target {
                        if self.scroll.glide_to(target, height, now) {
                            agg_gui::animation::request_draw();
                        }
                    }
                }
                None => {
                    if let Some(placement) = self.placement(0.0) {
                        self.controller
                            .borrow_mut()
                            .ensure_visible(&id, placement.scale, now);
                    }
                }
            }
        }
        let slide_offset = self.controller.borrow_mut().slide_offset_at(now);
        if self.controller.borrow().is_following() {
            agg_gui::animation::request_draw(); // keep the cursor moving
        }
        let shift = if page_scale.is_some() {
            self.scroll.offset()
        } else {
            -slide_offset
        };
        (follow_ids, shift)
    }

    /// Padded notehead hit boxes, nearest center wins (ports the Swift
    /// page's `noteHitAt`). Hit geometry follows the slide transform and
    /// the page scroll, like the page rebuilding its rects at
    /// `transitionend` and on `scroll`.
    fn note_hit_at(&self, pos: Point) -> Option<String> {
        if self.page_scale().is_some() && self.scroll.hovering_bar(pos, self.bounds) {
            return None;
        }
        let placement = self.placement(self.shift_on_screen((self.now)()))?;
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
mod tests;
