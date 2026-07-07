//! Beginner-mode keyboard strip: highlights the next key(s) to play,
//! labels them, flashes the wrongly played key, and dots middle C for
//! orientation.
//!
//! Ports the `PianoKeyboardView` SwiftUI view from
//! `UI/PianoKeyboardView.swift` as an agg-gui widget painting through
//! `DrawCtx` (the `KeyboardLayout` geometry ported earlier drives it).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::draw_ctx::DrawCtx;
use agg_gui::event::{Event, EventResult};
use agg_gui::geometry::{Rect, Size};
use agg_gui::widget::Widget;

use crate::core::PitchSpelling;
use crate::engine::SessionEngine;
use crate::ui::fonts::UiFonts;
use crate::ui::palette;

pub struct PianoStripWidget {
    engine: Rc<RefCell<SessionEngine>>,
    fonts: UiFonts,
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
}

impl PianoStripWidget {
    pub const HEIGHT: f64 = 92.0;

    pub fn new(engine: Rc<RefCell<SessionEngine>>, fonts: UiFonts) -> Self {
        Self {
            engine,
            fonts,
            bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            children: Vec::new(),
        }
    }
}

impl Widget for PianoStripWidget {
    fn type_name(&self) -> &'static str {
        "PianoStripWidget"
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
        // Zero-height when hidden (the Swift view was conditionally in the
        // tree; agg-gui keeps the widget and collapses it).
        let engine = self.engine.borrow();
        let visible = engine.show_keys() && !engine.is_free_play();
        Size::new(available.width, if visible { Self::HEIGHT } else { 0.0 })
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let engine = self.engine.borrow();
        if !engine.show_keys() || engine.is_free_play() {
            return;
        }
        let width = self.bounds.width;
        let height = self.bounds.height;
        if height <= 0.0 {
            return;
        }

        // Strip background (the Swift Color(white: 0.3)).
        ctx.set_fill_color(Color::rgb(0.3, 0.3, 0.3));
        ctx.begin_path();
        ctx.rect(0.0, 0.0, width, height);
        ctx.fill();

        let layout = engine.keyboard_layout().clone();
        let highlighted = engine.current_expected_midis().clone();
        let wrong_flash = engine.wrong_key_flash();
        // Hear It: light the keys the playback is sounding right now.
        let sounding = engine.playback_sounding_midis();
        drop(engine);

        // Key names in the bold face — the Swift
        // `.system(size: 9, weight: .semibold)`.
        ctx.set_font(Arc::clone(&self.fonts.bold));
        ctx.set_font_size(9.0);
        for key in &layout.keys {
            let is_next = highlighted.contains(&key.midi);
            let is_wrong = wrong_flash == Some(key.midi);
            let is_sounding = sounding.contains(&key.midi);
            let key_height = if key.is_black { height * 0.6 } else { height };
            let fill = if is_wrong {
                palette::RED
            } else if is_sounding {
                palette::GREEN
            } else if is_next {
                palette::KEY_BLUE
            } else if key.is_black {
                Color::black()
            } else {
                Color::white()
            };
            let x = key.x * width + 0.5;
            let w = key.width * width - 1.0;
            // Keys hang from the strip top (y-up: top = height).
            let y = height - key_height;
            ctx.set_fill_color(fill);
            ctx.begin_path();
            ctx.rounded_rect(x, y, w, key_height, 2.0);
            ctx.fill();
            ctx.set_stroke_color(Color::rgb(0.25, 0.25, 0.25));
            ctx.set_line_width(0.5);
            ctx.begin_path();
            ctx.rounded_rect(x, y, w, key_height, 2.0);
            ctx.stroke();

            if is_next || is_wrong || is_sounding {
                ctx.set_fill_color(Color::white());
                let name = PitchSpelling::name(key.midi);
                let text_w = ctx
                    .measure_text(&name)
                    .map(|m| m.width)
                    .unwrap_or(name.len() as f64 * 5.0);
                // Bottom-of-key label (the Swift `.padding(.bottom, 3)`).
                ctx.fill_text(&name, x + (w - text_w) / 2.0, y + 3.0);
            } else if key.midi == 60 {
                // Middle C orientation dot.
                ctx.set_fill_color(Color::rgba(0.5, 0.5, 0.5, 0.45));
                ctx.begin_path();
                ctx.circle(x + w / 2.0, y + 8.0, 2.5);
                ctx.fill();
            }
        }
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
