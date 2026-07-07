//! A label whose text is a closure evaluated every layout/paint — the
//! agg-gui equivalent of SwiftUI's observed `Text(...)` interpolations
//! (same helper the other agg-gui apps use).

use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::draw_ctx::DrawCtx;
use agg_gui::event::{Event, EventResult};
use agg_gui::geometry::{Rect, Size};
use agg_gui::text::Font;
use agg_gui::widget::Widget;
use agg_gui::widgets::Label;

pub struct DynamicLabel {
    label: Label,
    callback: Box<dyn Fn() -> String>,
    color_callback: Option<Box<dyn Fn() -> Option<Color>>>,
}

impl DynamicLabel {
    pub fn new(callback: impl Fn() -> String + 'static, font: Arc<Font>) -> Self {
        Self {
            label: Label::new("", font),
            callback: Box::new(callback),
            color_callback: None,
        }
    }

    pub fn with_font_size(mut self, size: f64) -> Self {
        self.label = self.label.with_font_size(size);
        self
    }

    pub fn with_dim(mut self, dim: bool) -> Self {
        self.label = self.label.with_dim(dim);
        self
    }

    /// Dynamic color (e.g. error counts turning red). `None` = theme text.
    pub fn with_color_fn(mut self, callback: impl Fn() -> Option<Color> + 'static) -> Self {
        self.color_callback = Some(Box::new(callback));
        self
    }

    fn refresh(&mut self) {
        let text = (self.callback)();
        self.label.set_text(text);
        if let Some(color_callback) = &self.color_callback {
            if let Some(color) = (color_callback)() {
                self.label.set_color(color);
            }
        }
    }
}

impl Widget for DynamicLabel {
    fn type_name(&self) -> &'static str {
        "DynamicLabel"
    }

    fn bounds(&self) -> Rect {
        self.label.bounds()
    }

    fn set_bounds(&mut self, b: Rect) {
        self.label.set_bounds(b);
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        self.label.children()
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        self.label.children_mut()
    }

    fn layout(&mut self, available: Size) -> Size {
        self.refresh();
        self.label.layout(available)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        self.refresh();
        self.label.paint(ctx);
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.label.on_event(event)
    }
}
