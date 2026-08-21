//! The notation widget's own page scroll: a whole-pixel offset moved by
//! the wheel, the scrollbar thumb, and the controller's keep-in-upper-
//! third follow; painted with agg-gui's shared scrollbar so it looks like
//! every `ScrollView` in the app.
//!
//! Ports the `window.scrollTo({ behavior: 'smooth' })` half of the Swift
//! page script: the follow glides over the same 0.4 s ease-out the slide
//! lane uses, while manual input jumps at once and cancels any glide.

use agg_gui::draw_ctx::DrawCtx;
use agg_gui::event::{Event, MouseButton};
use agg_gui::geometry::{Point, Rect};
use agg_gui::widgets::scrollbar::{
    paint_prepared_scrollbar, ScrollbarAxis, ScrollbarGeometry, ScrollbarOrientation,
    DEFAULT_GRAB_MARGIN,
};
use agg_gui::{current_scroll_style, current_scroll_visibility, ScrollBarStyle};

use crate::notation::slide::{ease_out, SLIDE_DURATION};
use crate::notation::widget::whole_px;

/// Content px per wheel notch — the `ScrollView` convention.
const WHEEL_STEP: f64 = 40.0;
/// Pixels at the right edge left for a parent window's resize grip (the
/// `ScrollView` guard, so the bar lands where every other bar does).
const RIGHT_EDGE_GUARD: f64 = 4.0;

/// A smooth scroll in flight: `from` → `to` over [`SLIDE_DURATION`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct Glide {
    from: f64,
    to: f64,
    started_at: f64,
}

/// What the scroll layer did with an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScrollOutcome {
    /// Not a scroll event (or nothing to scroll): the widget carries on.
    Ignored,
    /// Taken by the scroll layer; `by_user` when the offset moved under
    /// manual control (wheel, thumb drag, track page) — the follow yields.
    Consumed { by_user: bool },
}

pub(super) struct PageScroll {
    axis: ScrollbarAxis,
    glide: Option<Glide>,
}

impl PageScroll {
    pub fn new() -> Self {
        Self {
            axis: ScrollbarAxis {
                enabled: true,
                ..Default::default()
            },
            glide: None,
        }
    }

    /// The offset on screen (content px from the top, whole px).
    pub fn offset(&self) -> f64 {
        self.axis.offset
    }

    /// The settled target: where a glide is heading, else the offset.
    #[cfg(test)]
    pub fn target(&self) -> f64 {
        self.glide.map_or(self.axis.offset, |g| g.to)
    }

    #[cfg(test)]
    pub fn is_gliding(&self) -> bool {
        self.glide.is_some()
    }

    /// The content height (widget px) the page scrolls through; clamps
    /// the offset when the content shrinks.
    pub fn set_content(&mut self, content_h: f64, viewport_h: f64) {
        self.axis.content = content_h.max(0.0);
        self.axis.clamp_offset(viewport_h);
    }

    pub fn max_scroll(&self, viewport_h: f64) -> f64 {
        self.axis.max_scroll(viewport_h)
    }

    pub fn can_scroll(&self, viewport_h: f64) -> bool {
        self.axis.can_scroll(viewport_h)
    }

    /// `window.scrollTo(0, 0)`: back to the top, no motion.
    pub fn reset(&mut self) {
        self.glide = None;
        self.axis.offset = 0.0;
        self.axis.dragging = false;
    }

    /// `scrollTo({ top, behavior: 'smooth' })`: glide to `target` from the
    /// offset on screen right now (a glide restarting mid-flight continues
    /// from where the page is). Returns true when motion started.
    pub fn glide_to(&mut self, target: f64, viewport_h: f64, now: f64) -> bool {
        let target = whole_px(target).clamp(0.0, self.max_scroll(viewport_h));
        if target == self.axis.offset {
            self.glide = None;
            return false;
        }
        self.glide = Some(Glide {
            from: self.axis.offset,
            to: target,
            started_at: now,
        });
        true
    }

    /// Move the on-screen offset along the glide at `now`; true while
    /// frames are still needed.
    pub fn advance(&mut self, now: f64, viewport_h: f64) -> bool {
        let Some(glide) = self.glide else {
            return false;
        };
        let t = (now - glide.started_at) / SLIDE_DURATION;
        if t >= 1.0 {
            self.axis.offset = glide.to;
            self.glide = None;
        } else {
            self.axis.offset = whole_px(glide.from + (glide.to - glide.from) * ease_out(t));
        }
        self.axis.clamp_offset(viewport_h);
        self.glide.is_some()
    }

    /// The vertical bar against the widget's right edge, the `ScrollView`
    /// way (track inset by the style's inner margin, widget-local y-up).
    fn geometry(&self, bounds: Rect, style: ScrollBarStyle) -> ScrollbarGeometry {
        let lo = style.inner_margin;
        let hi = (bounds.height - style.inner_margin).max(lo);
        ScrollbarGeometry {
            orientation: ScrollbarOrientation::Vertical,
            track_start: lo,
            track_end: hi,
            cross_end: bounds.width - RIGHT_EDGE_GUARD - style.outer_margin,
            hit_margin: DEFAULT_GRAB_MARGIN,
        }
    }

    /// The pointer is over the bar's hover zone (no note hover there).
    pub fn hovering_bar(&self, pos: Point, bounds: Rect) -> bool {
        let style = current_scroll_style();
        self.can_scroll(bounds.height)
            && self
                .axis
                .pos_in_hover(pos, style, self.geometry(bounds, style))
    }

    /// Paint the bar over the page; true while its hover/visibility
    /// animation still wants frames.
    pub fn paint(&mut self, ctx: &mut dyn DrawCtx, bounds: Rect) -> bool {
        let style = current_scroll_style();
        let geom = self.geometry(bounds, style);
        if let Some(bar) =
            self.axis
                .prepare_paint(bounds.height, style, current_scroll_visibility(), geom)
        {
            paint_prepared_scrollbar(ctx, bar);
        }
        self.axis.animation_active()
    }

    /// Wheel, thumb drag, and track paging — manual input jumps at once
    /// and cancels any glide in flight.
    pub fn on_event(&mut self, event: &Event, bounds: Rect) -> ScrollOutcome {
        let viewport_h = bounds.height;
        let style = current_scroll_style();
        let geom = self.geometry(bounds, style);
        match event {
            Event::MouseWheel { delta_y, .. } => {
                if !self.can_scroll(viewport_h) {
                    return ScrollOutcome::Ignored;
                }
                // Positive delta_y = the user wants to see content above
                // = a smaller offset.
                self.glide = None;
                self.axis.scroll_by(-delta_y * WHEEL_STEP, viewport_h);
                ScrollOutcome::Consumed { by_user: true }
            }
            Event::MouseMove { pos } => {
                if self.axis.dragging {
                    self.glide = None;
                    let moved = self.axis.drag_to(*pos, viewport_h, style, geom);
                    return ScrollOutcome::Consumed { by_user: moved };
                }
                if self.axis.update_hover(*pos, viewport_h, style, geom) {
                    agg_gui::animation::request_draw();
                }
                ScrollOutcome::Ignored
            }
            Event::MouseDown {
                pos,
                button: MouseButton::Left,
                ..
            } => {
                if !self.can_scroll(viewport_h) || !self.axis.pos_in_hover(*pos, style, geom) {
                    return ScrollOutcome::Ignored;
                }
                if self.axis.begin_drag(*pos, viewport_h, style, geom) {
                    return ScrollOutcome::Consumed { by_user: false };
                }
                self.glide = None;
                let moved = self.axis.page_at(*pos, viewport_h, style, geom);
                ScrollOutcome::Consumed { by_user: moved }
            }
            Event::MouseUp { .. } => {
                let was = self.axis.dragging;
                self.axis.dragging = false;
                if was {
                    ScrollOutcome::Consumed { by_user: false }
                } else {
                    ScrollOutcome::Ignored
                }
            }
            _ => ScrollOutcome::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agg_gui::event::Modifiers;

    fn bounds() -> Rect {
        Rect::new(0.0, 0.0, 600.0, 400.0)
    }

    #[test]
    fn glide_eases_out_in_whole_pixels_and_settles() {
        let mut scroll = PageScroll::new();
        scroll.set_content(2000.0, 400.0);
        assert!(scroll.glide_to(100.0, 400.0, 10.0));
        assert_eq!(scroll.offset(), 0.0);
        assert_eq!(scroll.target(), 100.0);
        assert!(scroll.advance(10.2, 400.0));
        // ease-out: past halfway at half time (1 - 0.25 = 0.75).
        assert_eq!(scroll.offset(), 75.0);
        assert!(!scroll.advance(10.4, 400.0));
        assert_eq!(scroll.offset(), 100.0);
        assert!(!scroll.is_gliding());
        // Fractional targets land on whole pixels; a restart continues
        // from the offset on screen.
        assert!(scroll.glide_to(250.4, 400.0, 20.0));
        scroll.advance(20.2, 400.0);
        assert!(
            (scroll.offset() - 212.5).abs() <= 0.5,
            "{}",
            scroll.offset()
        );
        assert_eq!(scroll.offset().fract(), 0.0);
        scroll.glide_to(0.0, 400.0, 20.2);
        scroll.advance(20.6, 400.0);
        assert_eq!(scroll.offset(), 0.0);
    }

    #[test]
    fn glide_clamps_to_the_page_and_reset_returns_to_top() {
        let mut scroll = PageScroll::new();
        scroll.set_content(1000.0, 400.0);
        assert_eq!(scroll.max_scroll(400.0), 600.0);
        scroll.glide_to(5000.0, 400.0, 0.0);
        scroll.advance(1.0, 400.0);
        assert_eq!(scroll.offset(), 600.0);
        scroll.reset();
        assert_eq!(scroll.offset(), 0.0);
        assert!(!scroll.is_gliding());
        // Nothing to scroll: no glide, no offset.
        scroll.set_content(300.0, 400.0);
        assert!(!scroll.glide_to(50.0, 400.0, 2.0));
        assert_eq!(scroll.offset(), 0.0);
    }

    #[test]
    fn wheel_jumps_cancels_the_glide_and_is_user_input() {
        let mut scroll = PageScroll::new();
        scroll.set_content(2000.0, 400.0);
        scroll.glide_to(500.0, 400.0, 0.0);
        let wheel = Event::MouseWheel {
            pos: Point::new(10.0, 10.0),
            delta_y: -1.0, // wheel back: see content below
            delta_x: 0.0,
            modifiers: Modifiers::default(),
        };
        assert_eq!(
            scroll.on_event(&wheel, bounds()),
            ScrollOutcome::Consumed { by_user: true }
        );
        assert!(!scroll.is_gliding());
        assert_eq!(scroll.offset(), WHEEL_STEP);
        // Nothing to scroll: the wheel passes through to the parent.
        scroll.set_content(300.0, 400.0);
        assert_eq!(scroll.on_event(&wheel, bounds()), ScrollOutcome::Ignored);
    }
}
