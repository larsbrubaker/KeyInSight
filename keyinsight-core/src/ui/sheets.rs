//! The Library and Progress overlays.
//!
//! Ports `UI/LibrarySheet.swift` and `UI/ProgressPanel.swift` as
//! `Conditional`-gated overlay panels (SwiftUI `.sheet` maps to an overlay
//! column above the training view). The CalibrationSheet flow arrives with
//! the tempo-latency work in Phase 2.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::text::Font;
use agg_gui::widget::Widget;
use agg_gui::widgets::{Button, FlexColumn, FlexRow, Label, ScrollView};

use crate::engine::SessionEngine;
use crate::score::RepertoireLibrary;
use crate::ui::DynamicLabel;

type Engine = Rc<RefCell<SessionEngine>>;

/// Bundled repertoire, sorted by the difficulty index (the LibrarySheet's
/// easiest-first ordering).
pub fn build_library_sheet(
    engine: &Engine,
    font: &Arc<Font>,
    visible: Rc<Cell<bool>>,
) -> Box<dyn Widget> {
    let mut column = FlexColumn::new().with_gap(6.0).with_padding(14.0);
    column = column.add(Box::new(
        Label::new("Library", Arc::clone(font)).with_font_size(18.0),
    ));
    column = column.add(Box::new(
        Label::new(
            "Public-domain pieces, easiest first",
            Arc::clone(font),
        )
        .with_font_size(12.0)
        .with_dim(true),
    ));

    let mut pieces = RepertoireLibrary::bundled();
    pieces.sort_by(|a, b| {
        a.difficulty_index()
            .partial_cmp(&b.difficulty_index())
            .expect("difficulty indices are finite")
    });
    let mut list = FlexColumn::new().with_gap(4.0);
    for piece in pieces {
        let click = Rc::clone(engine);
        let close = Rc::clone(&visible);
        let title = piece.title.clone();
        list = list.add(Box::new(Button::new(title, Arc::clone(font)).on_click(
            move || {
                click.borrow_mut().start_piece(piece.clone());
                close.set(false);
                agg_gui::animation::request_draw();
            },
        )));
    }
    column = column.add_flex(Box::new(ScrollView::new(Box::new(list))), 1.0);

    let close = Rc::clone(&visible);
    column = column.add(Box::new(Button::new("Close", Arc::clone(font)).on_click(
        move || {
            close.set(false);
            agg_gui::animation::request_draw();
        },
    )));

    Box::new(agg_gui::widgets::Conditional::new(visible, Box::new(column)))
}

/// Per-item progress: the unlock ladder with attempts / error / latency,
/// plus interval shapes (the ProgressPanel's tables as dynamic text; the
/// notation heat staff renders through `render_progress_staff` when the
/// dedicated staff view lands).
pub fn build_progress_sheet(
    engine: &Engine,
    font: &Arc<Font>,
    visible: Rc<Cell<bool>>,
) -> Box<dyn Widget> {
    let mut column = FlexColumn::new().with_gap(6.0).with_padding(14.0);
    column = column.add(Box::new(
        Label::new("Progress", Arc::clone(font)).with_font_size(18.0),
    ));

    {
        let engine = Rc::clone(engine);
        column = column.add_flex(
            Box::new(ScrollView::new(Box::new(
                DynamicLabel::new(
                    move || progress_text(&mut engine.borrow_mut()),
                    Arc::clone(font),
                )
                .with_font_size(12.0),
            ))),
            1.0,
        );
    }

    let close = Rc::clone(&visible);
    column = column.add(Box::new(Button::new("Close", Arc::clone(font)).on_click(
        move || {
            close.set(false);
            agg_gui::animation::request_draw();
        },
    )));

    Box::new(agg_gui::widgets::Conditional::new(visible, Box::new(column)))
}

fn progress_text(engine: &mut SessionEngine) -> String {
    let mut lines = Vec::new();
    lines.push("Notes (unlock ladder)".to_string());
    for entry in engine.progress_entries() {
        let state = if !entry.unlocked {
            "locked"
        } else if entry.mastered {
            "mastered"
        } else {
            "learning"
        };
        let stats = if entry.attempts > 0 {
            format!(
                " · {} tries · {}% err{}",
                entry.attempts,
                entry.error_percent.unwrap_or(0),
                entry
                    .latency_ms
                    .map(|l| format!(" · {:.1}s", l / 1000.0))
                    .unwrap_or_default()
            )
        } else {
            String::new()
        };
        lines.push(format!("{:5}  {state}{stats}", entry.name));
    }
    lines.push(String::new());
    lines.push("Interval shapes".to_string());
    for entry in engine.interval_entries() {
        let stats = if entry.attempts > 0 {
            format!(
                "{} tries · {}% err",
                entry.attempts,
                entry.error_percent.unwrap_or(0)
            )
        } else {
            "—".to_string()
        };
        lines.push(format!("{:10} {stats}", entry.label));
    }
    lines.join("\n")
}

/// Overlay container: dims the training view behind whichever sheet is
/// open (both sheets share one full-window overlay slot).
pub fn build_sheet_overlay(
    engine: &Engine,
    font: &Arc<Font>,
    cells: &crate::ui::side_panel::SidePanelCells,
) -> Box<dyn Widget> {
    let library = build_library_sheet(engine, font, Rc::clone(&cells.show_library));
    let progress = build_progress_sheet(engine, font, Rc::clone(&cells.show_progress));
    Box::new(
        FlexRow::new()
            .with_gap(0.0)
            .add_flex(library, 1.0)
            .add_flex(progress, 1.0),
    )
}
