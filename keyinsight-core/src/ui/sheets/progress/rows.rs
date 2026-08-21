//! The Progress sheet's list rows — history, per-note, trouble
//! transitions, intervals, chords — plus the shared `statColumns`,
//! fixed-width labels, legend dots, and section headers.

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::geometry::Size;
use agg_gui::widget::Widget;
use agg_gui::widgets::{Button, FlexRow, Label, LabelAlign};

use crate::engine::{ChordEntry, IntervalEntry, ProgressEntry, TransitionEntry};
use crate::persistence::ExerciseRecord;
use crate::ui::fonts::{icon, size, UiFonts};
use crate::ui::palette;

use super::format::{
    chord_status_color, error_text, format_timestamp, heat_color, latency_text,
    transition_color,
};
use super::Engine;

/// Font Awesome `circle` — the legend/heat dot.
const DOT: char = '\u{f111}';

/// `● label` — one legend entry.
pub(super) fn legend_dot(fonts: &UiFonts, color: Color, label: &str) -> FlexRow {
    FlexRow::new()
        .with_fit_width(true)
        .with_gap(3.0)
        .add(Box::new(
            Label::new(DOT.to_string(), Arc::clone(&fonts.icons))
                .with_font_size(8.0)
                .with_color(color),
        ))
        .add(Box::new(
            Label::new(label, Arc::clone(&fonts.regular))
                .with_font_size(size::CAPTION)
                .with_dim(true),
        ))
}

pub(super) fn section_header(title: &str, fonts: &UiFonts) -> Label {
    Label::new(title, Arc::clone(&fonts.bold)).with_font_size(size::BODY)
}

/// `date | n notes | clean/wrong | … | Practice`.
pub(super) fn history_row(
    engine: &Engine,
    fonts: &UiFonts,
    visible: &Rc<std::cell::Cell<bool>>,
    record: &ExerciseRecord,
) -> Box<dyn Widget> {
    let outcome = if record.error_count == 0 {
        ("clean".to_string(), palette::GREEN)
    } else {
        (format!("{} wrong", record.error_count), palette::RED)
    };
    let practice = {
        let engine = Rc::clone(engine);
        let close = Rc::clone(visible);
        let spec = record.spec_json.clone();
        Button::new("Practice", Arc::clone(&fonts.regular))
            .with_subtle()
            .with_active_fn(|| false)
            .with_compact()
            .on_click(move || {
                engine.borrow_mut().practice_exercise(&spec);
                close.set(false);
                agg_gui::animation::request_draw();
            })
    };
    Box::new(
        FlexRow::new()
            .with_gap(10.0)
            .add(Box::new(fixed_label(
                format_timestamp(record.started_at_ms),
                fonts,
                150.0,
                LabelAlign::Left,
            )))
            .add(Box::new(fixed_label(
                format!("{} notes", record.note_count),
                fonts,
                70.0,
                LabelAlign::Right,
            )))
            .add(Box::new(
                Label::new(outcome.0, Arc::clone(&fonts.regular))
                    .with_font_size(size::CALLOUT)
                    .with_color(outcome.1)
                    .with_align(LabelAlign::Right)
                    .with_min_size(Size::new(80.0, 0.0))
                    .with_max_size(Size::new(80.0, f64::INFINITY)),
            ))
            .add_flex(Box::new(crate::ui::hspacer()), 1.0)
            .add(Box::new(practice)),
    )
}

/// `● name | plays | err% | latency | ✓` — one Notes row.
pub(super) fn note_row(fonts: &UiFonts, entry: &ProgressEntry) -> Box<dyn Widget> {
    let mut row = FlexRow::new().with_gap(10.0);
    row = row.add(Box::new(
        Label::new(DOT.to_string(), Arc::clone(&fonts.icons))
            .with_font_size(9.0)
            .with_color(heat_color(entry.heat)),
    ));
    row = row.add(Box::new(mono_label(&entry.name, fonts, 44.0)));
    row = stat_columns(row, fonts, entry.attempts, entry.error_percent, entry.latency_ms);
    row = row.add_flex(Box::new(crate::ui::hspacer()), 1.0);
    if entry.mastered {
        row = row.add(Box::new(
            Label::new(icon::CHECK_SEAL.to_string(), Arc::clone(&fonts.icons))
                .with_font_size(12.0)
                .with_color(palette::GREEN),
        ));
    }
    Box::new(row)
}

/// `label | plays | err%` — one Trouble transitions row (err% red at
/// 35% and above, orange below).
pub(super) fn transition_row(fonts: &UiFonts, entry: &TransitionEntry) -> Box<dyn Widget> {
    Box::new(
        FlexRow::new()
            .with_gap(10.0)
            .add(Box::new(mono_label(&entry.label, fonts, 110.0)))
            .add(Box::new(fixed_label(
                format!("{} plays", entry.attempts),
                fonts,
                80.0,
                LabelAlign::Right,
            )))
            .add(Box::new(
                Label::new(format!("{}% err", entry.error_percent), Arc::clone(&fonts.regular))
                    .with_font_size(size::CALLOUT)
                    .with_color(transition_color(entry.error_percent))
                    .with_align(LabelAlign::Right)
                    .with_min_size(Size::new(70.0, 0.0))
                    .with_max_size(Size::new(70.0, f64::INFINITY)),
            ))
            .add_flex(Box::new(crate::ui::hspacer()), 1.0),
    )
}

/// `label | plays | err%` — one Intervals row.
pub(super) fn interval_row(fonts: &UiFonts, entry: &IntervalEntry) -> Box<dyn Widget> {
    let mut row = FlexRow::new().with_gap(10.0);
    row = row.add(Box::new(mono_label(&entry.label, fonts, 80.0)));
    row = stat_columns(row, fonts, entry.attempts, entry.error_percent, entry.latency_ms);
    row = row.add_flex(Box::new(crate::ui::hspacer()), 1.0);
    Box::new(row)
}

/// `label | status | [plays | err%]` — one Chords row; the stat columns
/// only once the shape has been played.
pub(super) fn chord_row(fonts: &UiFonts, entry: &ChordEntry) -> Box<dyn Widget> {
    let mut row = FlexRow::new().with_gap(10.0);
    row = row.add(Box::new(mono_label(&entry.label, fonts, 120.0)));
    let mut status = Label::new(entry.status.clone(), Arc::clone(&fonts.regular))
        .with_font_size(size::CALLOUT)
        .with_align(LabelAlign::Left)
        .with_min_size(Size::new(70.0, 0.0))
        .with_max_size(Size::new(70.0, f64::INFINITY));
    status = match chord_status_color(&entry.status) {
        Some(color) => status.with_color(color),
        None => status.with_dim(true),
    };
    row = row.add(Box::new(status));
    if entry.attempts > 0 {
        row = row
            .add(Box::new(fixed_label(
                format!("{} plays", entry.attempts),
                fonts,
                80.0,
                LabelAlign::Right,
            )))
            .add(Box::new(fixed_label(
                error_text(entry.error_percent),
                fonts,
                70.0,
                LabelAlign::Right,
            )));
    }
    row = row.add_flex(Box::new(crate::ui::hspacer()), 1.0);
    Box::new(row)
}

/// The Swift `statColumns`: plays / err% / latency, right-aligned fixed
/// widths.
fn stat_columns(
    row: FlexRow,
    fonts: &UiFonts,
    attempts: i64,
    error_percent: Option<i64>,
    latency_ms: Option<f64>,
) -> FlexRow {
    row.add(Box::new(fixed_label(
        format!("{attempts} plays"),
        fonts,
        80.0,
        LabelAlign::Right,
    )))
    .add(Box::new(fixed_label(
        error_text(error_percent),
        fonts,
        70.0,
        LabelAlign::Right,
    )))
    .add(Box::new(fixed_label(
        latency_text(latency_ms),
        fonts,
        60.0,
        LabelAlign::Right,
    )))
}

/// `.font(.body.monospaced()).frame(width:, alignment: .leading)`.
fn mono_label(text: &str, fonts: &UiFonts, width: f64) -> Label {
    Label::new(text, Arc::clone(&fonts.mono))
        .with_font_size(size::BODY)
        .with_min_size(Size::new(width, 0.0))
        .with_max_size(Size::new(width, f64::INFINITY))
}

fn fixed_label(text: String, fonts: &UiFonts, width: f64, align: LabelAlign) -> Label {
    Label::new(text, Arc::clone(&fonts.regular))
        .with_font_size(size::CALLOUT)
        .with_dim(true)
        .with_align(align)
        .with_min_size(Size::new(width, 0.0))
        .with_max_size(Size::new(width, f64::INFINITY))
}
