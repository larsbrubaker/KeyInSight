//! The Progress sheet — `UI/ProgressPanel.swift` as an 860×720 modal:
//! the skill-item heat map drawn ON the staff, recent exercises with
//! re-practice, per-note and per-interval stats, and the unlock footer.
//!
//! The content rebuilds on every open (the Swift `onAppear` reload):
//! the Progress button bumps `progress_generation`, the [`Rebuilder`]
//! sees the new version, and the builder re-queries the engine and
//! re-renders the heat staff.

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::geometry::Size;
use agg_gui::widget::Widget;
use agg_gui::widgets::{
    Button, FlexColumn, FlexRow, Label, LabelAlign, ModalSheet, Rebuilder, ScrollView, Separator,
    SizedBox,
};

use crate::engine::{IntervalEntry, ProgressEntry};
use crate::notation::{NotationController, NotationRenderer, NoteState, NotationWidget};
use crate::persistence::ExerciseRecord;
use crate::ui::fonts::{icon, size, UiFonts};
use crate::ui::palette;
use crate::ui::side_panel::SidePanelCells;

use super::{Clock, Engine};

/// The Swift ideal frame (`idealWidth: 860, idealHeight: 720`).
const SHEET_SIZE: Size = Size {
    width: 860.0,
    height: 720.0,
};
/// The heat staff strip height.
const STAFF_HEIGHT: f64 = 220.0;

pub fn build_progress_sheet(
    engine: &Engine,
    fonts: &UiFonts,
    clock: &Clock,
    cells: &SidePanelCells,
) -> Box<dyn Widget> {
    let visible = Rc::clone(&cells.show_progress);
    let generation = Rc::clone(&cells.progress_generation);

    let build_engine = Rc::clone(engine);
    let build_fonts = fonts.clone();
    let build_clock = Rc::clone(clock);
    let build_visible = Rc::clone(&visible);
    let content = Rebuilder::new(
        move || generation.get(),
        move || {
            build_content(
                &build_engine,
                &build_fonts,
                &build_clock,
                &build_visible,
            )
        },
    );

    Box::new(ModalSheet::new(visible, Box::new(content)).with_panel_size(SHEET_SIZE))
}

fn build_content(
    engine: &Engine,
    fonts: &UiFonts,
    clock: &Clock,
    visible: &Rc<std::cell::Cell<bool>>,
) -> Box<dyn Widget> {
    // The Swift `onAppear` data load + heat staff render.
    let (entries, intervals, history, staff_controller) = {
        let mut engine = engine.borrow_mut();
        let entries = engine.progress_entries(crate::score::Staff::Treble);
        let intervals = engine.interval_entries();
        let history = engine.recent_exercises(20);
        let renderer = Rc::new(std::cell::RefCell::new(NotationRenderer::new()));
        let controller = Rc::new(std::cell::RefCell::new(NotationController::new(renderer)));
        engine.render_progress_staff(&mut controller.borrow_mut(), crate::score::Staff::Treble);
        (entries, intervals, history, controller)
    };

    let mut column = FlexColumn::new().with_gap(0.0);

    // Header: title + legend + Done.
    {
        let mut header = FlexRow::new().with_gap(10.0).with_padding(14.0);
        header = header.add(Box::new(
            Label::new("Progress", Arc::clone(&fonts.bold)).with_font_size(size::TITLE2),
        ));
        header = header.add_flex(Box::new(crate::ui::hspacer()), 1.0);
        for (color, label) in [
            (palette::GREEN, "mastered"),
            (palette::ORANGE, "learning"),
            (palette::RED, "weak"),
            (palette::GRAY_LOCKED, "locked"),
        ] {
            header = header.add(Box::new(legend_dot(fonts, color, label)));
        }
        let close = Rc::clone(visible);
        header = header.add(Box::new(
            Button::new("Done", Arc::clone(&fonts.regular)).with_subtle().with_active_fn(|| false).on_click(move || {
                close.set(false);
                agg_gui::animation::request_draw();
            }),
        ));
        column = column.add(Box::new(header));
    }

    // The heat-map staff (white page, fixed height).
    column = column.add(Box::new(
        SizedBox::new()
            .with_height(STAFF_HEIGHT)
            .with_child(Box::new(NotationWidget::new(
                staff_controller,
                Rc::clone(clock),
            ))),
    ));
    column = column.add(Box::new(Separator::horizontal().with_line_inset(0.0)));

    // The three list sections.
    let mut list = FlexColumn::new().with_gap(4.0).with_padding(14.0);
    list = list.add(Box::new(section_header("Recent exercises", fonts)));
    if history.is_empty() {
        list = list.add(Box::new(
            Label::new(
                "Complete an exercise and it will appear here.",
                Arc::clone(&fonts.regular),
            )
            .with_font_size(size::CALLOUT)
            .with_dim(true),
        ));
    }
    for record in &history {
        list = list.add(history_row(engine, fonts, visible, record));
    }

    list = list.add(Box::new(section_header("Notes", fonts)));
    for entry in entries.iter().filter(|e| e.unlocked) {
        list = list.add(note_row(fonts, entry));
    }

    list = list.add(Box::new(section_header("Intervals", fonts)));
    for entry in intervals.iter().filter(|e| e.attempts > 0) {
        list = list.add(interval_row(fonts, entry));
    }
    column = column.add_flex(Box::new(ScrollView::new(Box::new(list))), 1.0);

    // Footer: mastery tally + next unlock.
    column = column.add(Box::new(Separator::horizontal().with_line_inset(0.0)));
    {
        let unlocked: Vec<&ProgressEntry> = entries.iter().filter(|e| e.unlocked).collect();
        let mastered = unlocked.iter().filter(|e| e.mastered).count();
        let tally = format!("{mastered} of {} active items mastered", unlocked.len());
        let next = match entries.iter().find(|e| !e.unlocked) {
            Some(next) => format!("Next unlock: {} — master all active items", next.name),
            None => "All items unlocked".to_string(),
        };
        column = column.add(Box::new(
            FlexRow::new()
                .with_gap(10.0)
                .with_padding(14.0)
                .add(Box::new(
                    Label::new(tally, Arc::clone(&fonts.regular))
                        .with_font_size(size::CALLOUT)
                        .with_dim(true),
                ))
                .add_flex(Box::new(crate::ui::hspacer()), 1.0)
                .add(Box::new(
                    Label::new(next, Arc::clone(&fonts.regular))
                        .with_font_size(size::CALLOUT)
                        .with_dim(true),
                )),
        ));
    }

    Box::new(column)
}

/// `● label` — one legend entry.
fn legend_dot(fonts: &UiFonts, color: Color, label: &str) -> FlexRow {
    FlexRow::new()
        .with_fit_width(true)
        .with_gap(3.0)
        .add(Box::new(
            Label::new("\u{f111}".to_string(), Arc::clone(&fonts.icons))
                .with_font_size(8.0)
                .with_color(color),
        ))
        .add(Box::new(
            Label::new(label, Arc::clone(&fonts.regular))
                .with_font_size(size::CAPTION)
                .with_dim(true),
        ))
}

fn section_header(title: &str, fonts: &UiFonts) -> Label {
    Label::new(title, Arc::clone(&fonts.bold)).with_font_size(size::BODY)
}

/// `date | n notes | clean/wrong | … | Practice`.
fn history_row(
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
            .with_subtle().with_active_fn(|| false)
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
fn note_row(fonts: &UiFonts, entry: &ProgressEntry) -> Box<dyn Widget> {
    let mut row = FlexRow::new().with_gap(10.0);
    row = row.add(Box::new(
        Label::new("\u{f111}".to_string(), Arc::clone(&fonts.icons))
            .with_font_size(9.0)
            .with_color(heat_color(entry.heat)),
    ));
    row = row.add(Box::new(
        Label::new(entry.name.clone(), Arc::clone(&fonts.mono))
            .with_font_size(size::BODY)
            .with_min_size(Size::new(44.0, 0.0))
            .with_max_size(Size::new(44.0, f64::INFINITY)),
    ));
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

/// `label | plays | err%` — one Intervals row.
fn interval_row(fonts: &UiFonts, entry: &IntervalEntry) -> Box<dyn Widget> {
    let mut row = FlexRow::new().with_gap(10.0);
    row = row.add(Box::new(
        Label::new(entry.label.clone(), Arc::clone(&fonts.mono))
            .with_font_size(size::BODY)
            .with_min_size(Size::new(80.0, 0.0))
            .with_max_size(Size::new(80.0, f64::INFINITY)),
    ));
    row = stat_columns(row, fonts, entry.attempts, entry.error_percent, entry.latency_ms);
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
        error_percent
            .map(|e| format!("{e}% err"))
            .unwrap_or_else(|| "—".to_string()),
        fonts,
        70.0,
        LabelAlign::Right,
    )))
    .add(Box::new(fixed_label(
        latency_ms
            .map(|l| format!("{:.1} s", l / 1000.0))
            .unwrap_or_else(|| "—".to_string()),
        fonts,
        60.0,
        LabelAlign::Right,
    )))
}

fn fixed_label(text: String, fonts: &UiFonts, width: f64, align: LabelAlign) -> Label {
    Label::new(text, Arc::clone(&fonts.regular))
        .with_font_size(size::CALLOUT)
        .with_dim(true)
        .with_align(align)
        .with_min_size(Size::new(width, 0.0))
        .with_max_size(Size::new(width, f64::INFINITY))
}

fn heat_color(heat: NoteState) -> Color {
    match heat {
        NoteState::Mastered => palette::GREEN,
        NoteState::Weak => palette::RED,
        NoteState::Locked => palette::GRAY_LOCKED,
        _ => palette::ORANGE,
    }
}

/// `record.startedAt.formatted(date: .abbreviated, time: .shortened)` —
/// "Jul 7, 16:32" from epoch milliseconds (UTC; the engine clock has no
/// timezone database).
fn format_timestamp(epoch_ms: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = epoch_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    // civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{} {}, {}, {:02}:{:02}",
        MONTHS[(month - 1) as usize],
        day,
        year,
        tod / 3600,
        (tod % 3600) / 60
    )
}
