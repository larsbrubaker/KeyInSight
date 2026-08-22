//! InfoRows — a stack of short, individually styled status lines with
//! optional leading icons, rebuilt from the engine every frame.
//!
//! Ports the SwiftUI status/summary `VStack`s in `UI/SidePanel.swift`:
//! each Swift `Text`/`Label` row (with its color, weight, monospaced
//! digits, and SF Symbol) becomes one [`InfoRow`], painted directly
//! through `DrawCtx`. A dedicated row kind paints the tempo-mode beat
//! dots (the Swift `Circle` HStack).

use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::draw_ctx::DrawCtx;
use agg_gui::event::{Event, EventResult};
use agg_gui::geometry::{Rect, Size};
use agg_gui::widget::Widget;

use crate::ui::fonts::UiFonts;

/// Row spacing — the Swift `VStack(spacing: 5)`.
const ROW_GAP: f64 = 5.0;
/// Icon column: glyph size relative to the row's font size.
const ICON_SCALE: f64 = 0.85;
const ICON_GAP: f64 = 5.0;
/// Gap between the glyphs of a [`InfoRow::glyph_run`] row (the Swift
/// `HStack(spacing: 3)`).
const GLYPH_GAP: f64 = 3.0;

/// Text treatment for a row (SwiftUI font modifiers).
///
/// SwiftUI distinguishes two things the port must not merge:
/// `.monospaced()` swaps in a code face, while `.monospacedDigit()` keeps
/// the body face and only pins the digits to one advance so counters do
/// not jitter. [`RowStyle::Mono`] is the first; [`RowStyle::TabularDigits`]
/// and [`RowStyle::BoldTabularDigits`] are the second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowStyle {
    /// `.body` / `.callout` regular text.
    Regular,
    /// `.headline` / `.bold()`.
    Bold,
    /// `.monospaced()` — the code face (Cascadia Code).
    Mono,
    /// `.monospacedDigit()` — the body face with tabular figures.
    TabularDigits,
    /// `.font(.headline).monospacedDigit()` — headline weight, tabular
    /// figures (the tempo count-in row).
    BoldTabularDigits,
}

/// One status line: optional icon, text, color, style, font size.
pub struct InfoRow {
    pub icon: Option<char>,
    pub text: String,
    /// `None` follows the theme text color; `Some` overrides (Swift
    /// `.foregroundStyle`). Icons always take the row color.
    pub color: Option<Color>,
    /// Theme secondary text (Swift `.foregroundStyle(.secondary)`);
    /// ignored when `color` is set.
    pub dim: bool,
    pub style: RowStyle,
    pub size: f64,
    /// Paint the tempo beat dots instead of text: `Some((beat, count))`
    /// fills dot `beat` with the accent blue, the rest gray.
    pub dots: Option<(usize, usize)>,
    /// A run of icon glyphs, each in its own color (`None` = theme
    /// secondary), instead of text (the survival hearts `HStack(spacing: 3)`).
    pub glyphs: Option<Vec<(char, Option<Color>)>>,
}

impl InfoRow {
    pub fn text(text: impl Into<String>, size: f64) -> Self {
        Self {
            icon: None,
            text: text.into(),
            color: None,
            dim: false,
            style: RowStyle::Regular,
            size,
            dots: None,
            glyphs: None,
        }
    }

    pub fn with_icon(mut self, icon: char) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub fn with_style(mut self, style: RowStyle) -> Self {
        self.style = style;
        self
    }

    /// The tempo beat-dot row: `beat` highlighted of `count`, followed by
    /// the row text (the BPM readout sits on the same line in Swift).
    pub fn beat_dots(beat: usize, count: usize, text: impl Into<String>, size: f64) -> Self {
        Self {
            icon: None,
            text: text.into(),
            color: None,
            dim: true,
            style: RowStyle::Mono,
            size,
            dots: Some((beat, count)),
            glyphs: None,
        }
    }

    /// A row of repeated icon glyphs, each painted in its own color —
    /// the survival lives row (`heart.fill` red / `heart` secondary).
    pub fn glyph_run(glyphs: Vec<(char, Option<Color>)>, size: f64) -> Self {
        Self {
            icon: None,
            text: String::new(),
            color: None,
            dim: false,
            style: RowStyle::Regular,
            size,
            dots: None,
            glyphs: Some(glyphs),
        }
    }

    fn line_height(&self) -> f64 {
        self.size * 1.5
    }
}

/// The widget: a closure produces the rows each frame (the agg-gui
/// equivalent of SwiftUI re-evaluating the `@ViewBuilder` on observed
/// changes).
pub struct InfoRows {
    rows_fn: Box<dyn Fn() -> Vec<InfoRow>>,
    fonts: UiFonts,
    rows: Vec<InfoRow>,
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    /// Center each row horizontally (the CalibrationSheet's
    /// `multilineTextAlignment(.center)` block).
    centered: bool,
}

impl InfoRows {
    pub fn new(fonts: &UiFonts, rows_fn: impl Fn() -> Vec<InfoRow> + 'static) -> Self {
        Self {
            rows_fn: Box::new(rows_fn),
            fonts: fonts.clone(),
            rows: Vec::new(),
            bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            children: Vec::new(),
            centered: false,
        }
    }

    pub fn with_centered(mut self, centered: bool) -> Self {
        self.centered = centered;
        self
    }

    fn total_height(&self) -> f64 {
        if self.rows.is_empty() {
            return 0.0;
        }
        let lines: f64 = self.rows.iter().map(InfoRow::line_height).sum();
        lines + ROW_GAP * (self.rows.len() as f64 - 1.0)
    }
}

impl Widget for InfoRows {
    fn type_name(&self) -> &'static str {
        "InfoRows"
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
        self.rows = (self.rows_fn)();
        Size::new(available.width, self.total_height())
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let visuals = ctx.visuals();
        // Y-up: first row at the top.
        let mut top = self.bounds.height;
        for row in &self.rows {
            let line_h = row.line_height();
            let baseline_center = top - line_h / 2.0;
            let color = row.color.unwrap_or(if row.dim {
                visuals.text_dim
            } else {
                visuals.text_color
            });

            let font = match row.style {
                RowStyle::Regular => &self.fonts.regular,
                RowStyle::Bold => &self.fonts.bold,
                RowStyle::Mono => &self.fonts.mono,
                RowStyle::TabularDigits => &self.fonts.tabular,
                RowStyle::BoldTabularDigits => &self.fonts.bold_tabular,
            };
            ctx.set_font(Arc::clone(font));
            ctx.set_font_size(row.size);
            let text_metrics = ctx.measure_text(&row.text);
            let text_w = text_metrics.as_ref().map(|m| m.width).unwrap_or(0.0);

            // Leading decoration width (beat dots or icon).
            let dot_r = 4.5;
            let lead = if let Some((_, count)) = row.dots {
                count as f64 * (dot_r * 2.0 + 5.0) + 3.0
            } else if let Some(glyphs) = &row.glyphs {
                glyphs
                    .iter()
                    .map(|(glyph, _)| icon_width(ctx, &self.fonts, *glyph, row.size) + GLYPH_GAP)
                    .sum()
            } else if let Some(icon) = row.icon {
                icon_width(ctx, &self.fonts, icon, row.size) + ICON_GAP
            } else {
                0.0
            };
            let x0 = if self.centered {
                ((self.bounds.width - lead - text_w) / 2.0).max(0.0)
            } else {
                0.0
            };
            let mut x = x0;

            if let Some((beat, count)) = row.dots {
                // The Swift 9pt beat circles with 5pt gaps.
                for i in 0..count {
                    let fill = if i == beat {
                        crate::ui::palette::BLUE
                    } else {
                        Color::rgba(0.5, 0.5, 0.5, 0.3)
                    };
                    ctx.set_fill_color(fill);
                    ctx.begin_path();
                    ctx.circle(x + dot_r, baseline_center, dot_r);
                    ctx.fill();
                    x += dot_r * 2.0 + 5.0;
                }
                x += 3.0;
            } else if let Some(glyphs) = &row.glyphs {
                // The Swift `HStack(spacing: 3)` of heart symbols.
                for (glyph, glyph_color) in glyphs {
                    let width = icon_width(ctx, &self.fonts, *glyph, row.size);
                    ctx.set_font(Arc::clone(&self.fonts.icons));
                    ctx.set_font_size(row.size * ICON_SCALE);
                    ctx.set_fill_color(glyph_color.unwrap_or(visuals.text_dim));
                    let ty = match self
                        .fonts
                        .icons
                        .glyph_visual_bounds(*glyph, row.size * ICON_SCALE)
                    {
                        Some((y_min, y_max)) => baseline_center - (y_min + y_max) / 2.0,
                        None => baseline_center,
                    };
                    ctx.fill_text(&glyph.to_string(), x, ty);
                    x += width + GLYPH_GAP;
                }
            } else if let Some(icon) = row.icon {
                ctx.set_font(Arc::clone(&self.fonts.icons));
                ctx.set_font_size(row.size * ICON_SCALE);
                ctx.set_fill_color(color);
                // Visually center the glyph on the row (Font Awesome
                // glyphs sit in a sub-rect of the em square).
                let ty = match self
                    .fonts
                    .icons
                    .glyph_visual_bounds(icon, row.size * ICON_SCALE)
                {
                    Some((y_min, y_max)) => baseline_center - (y_min + y_max) / 2.0,
                    None => baseline_center,
                };
                ctx.fill_text(&icon.to_string(), x, ty);
                x = x0 + lead;
            }

            ctx.set_font(Arc::clone(font));
            ctx.set_font_size(row.size);
            ctx.set_fill_color(color);
            if let Some(m) = text_metrics {
                let ty = (baseline_center - line_h / 2.0) + m.centered_baseline_y(line_h);
                ctx.fill_text(&row.text, x, ty);
            }
            top -= line_h + ROW_GAP;
        }
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

/// Measured advance of an icon glyph at the row's icon size.
fn icon_width(ctx: &mut dyn DrawCtx, fonts: &UiFonts, icon: char, row_size: f64) -> f64 {
    ctx.set_font(Arc::clone(&fonts.icons));
    ctx.set_font_size(row_size * ICON_SCALE);
    ctx.measure_text(&icon.to_string())
        .map(|m| m.width)
        .unwrap_or(row_size)
}
