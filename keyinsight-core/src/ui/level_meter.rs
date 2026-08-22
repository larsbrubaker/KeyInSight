//! LevelMeter — the mic input capsule from `UI/SidePanel.swift`: a gray
//! track with a green fill that turns red near clipping (level > 0.85).

use agg_gui::color::Color;
use agg_gui::draw_ctx::DrawCtx;
use agg_gui::event::{Event, EventResult};
use agg_gui::geometry::{Rect, Size};
use agg_gui::widget::Widget;

/// The Swift `.frame(height: 7)` capsule.
const HEIGHT: f64 = 7.0;

pub struct LevelMeter {
    level_fn: Box<dyn Fn() -> f64>,
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
}

impl LevelMeter {
    pub fn new(level_fn: impl Fn() -> f64 + 'static) -> Self {
        Self {
            level_fn: Box::new(level_fn),
            bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            children: Vec::new(),
        }
    }
}

impl Widget for LevelMeter {
    fn type_name(&self) -> &'static str {
        "LevelMeter"
    }

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
        Size::new(available.width, HEIGHT)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let width = self.bounds.width;
        let level = (self.level_fn)().clamp(0.0, 1.0);
        let radius = HEIGHT / 2.0;

        // Track (Capsule, gray 25%).
        ctx.set_fill_color(Color::rgba(0.5, 0.5, 0.5, 0.25));
        ctx.begin_path();
        ctx.rounded_rect(0.0, 0.0, width, HEIGHT, radius);
        ctx.fill();

        // Fill: `Color.green`, `Color.red` past the clip threshold; never
        // thinner than the Swift `max(3, …)`.
        let fill_w = (width * level).max(3.0);
        let fill = if level > 0.85 {
            crate::ui::palette::RED
        } else {
            crate::ui::palette::GREEN
        };
        ctx.set_fill_color(fill);
        ctx.begin_path();
        ctx.rounded_rect(0.0, 0.0, fill_w, HEIGHT, radius);
        ctx.fill();
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
