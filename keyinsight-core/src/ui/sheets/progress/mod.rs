//! The Progress sheet — `UI/ProgressPanel.swift` as an 860×720 modal:
//! the skill-item heat map drawn ON the staff (one staff per hand),
//! recent exercises with re-practice, per-note / trouble-transition /
//! interval / chord stats, and the unlock footer.
//!
//! The content rebuilds on every open (the Swift `onAppear` reload):
//! the Progress button bumps `progress_generation`, the [`Rebuilder`]
//! sees the new version, and the builder re-queries the engine and
//! re-renders both heat staves. Rows live in [`rows`], the pure
//! text/color helpers in [`format`].

mod format;
mod rows;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::geometry::Size;
use agg_gui::widget::Widget;
use agg_gui::widgets::{
    Button, FlexColumn, FlexRow, Label, ModalSheet, Rebuilder, ScrollView, Separator, SizedBox,
};

use crate::notation::{NotationController, NotationFit, NotationRenderer, NotationWidget};
use crate::score::Staff;
use crate::ui::fonts::{size, UiFonts};
use crate::ui::palette;
use crate::ui::side_panel::SidePanelCells;

use super::{Clock, Engine};

/// The Swift ideal frame (`idealWidth: 860, idealHeight: 720`).
const SHEET_SIZE: Size = Size {
    width: 860.0,
    height: 720.0,
};
/// Each heat staff strip's height (`.frame(height: 150)`, one per hand).
const STAFF_HEIGHT: f64 = 150.0;

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

/// A fresh scrollable heat-map controller with `staff` rendered into it.
fn heat_staff(
    engine: &mut crate::engine::SessionEngine,
    staff: Staff,
) -> Rc<RefCell<NotationController>> {
    let renderer = Rc::new(RefCell::new(NotationRenderer::new()));
    let controller = Rc::new(RefCell::new(NotationController::new(renderer)));
    engine.render_progress_staff(&mut controller.borrow_mut(), staff);
    controller
}

fn build_content(
    engine: &Engine,
    fonts: &UiFonts,
    clock: &Clock,
    visible: &Rc<std::cell::Cell<bool>>,
) -> Box<dyn Widget> {
    // The Swift `onAppear` data load + both heat staff renders.
    let (
        entries,
        bass_entries,
        intervals,
        transitions,
        chords,
        history,
        treble_controller,
        bass_controller,
        next_unlock,
    ) = {
        let mut engine = engine.borrow_mut();
        let entries = engine.progress_entries(Staff::Treble);
        let bass_entries = engine.progress_entries(Staff::Bass);
        let intervals = engine.interval_entries();
        let transitions = engine.trouble_transitions(8);
        let chords = engine.chord_shape_entries();
        let history = engine.recent_exercises(20);
        let treble = heat_staff(&mut engine, Staff::Treble);
        let bass = heat_staff(&mut engine, Staff::Bass);
        let next_unlock = format::next_unlock_text(
            engine.skill.next_locked_midi(),
            engine.bass_skill.next_locked_midi(),
        );
        (
            entries,
            bass_entries,
            intervals,
            transitions,
            chords,
            history,
            treble,
            bass,
            next_unlock,
        )
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
            header = header.add(Box::new(rows::legend_dot(fonts, color, label)));
        }
        let close = Rc::clone(visible);
        header = header.add(Box::new(
            // `.keyboardShortcut(.cancelAction)`: Esc closes.
            Button::new("Done", Arc::clone(&fonts.regular))
                .with_subtle()
                .with_active_fn(|| false)
                .with_cancel_action()
                .on_click(move || {
                    close.set(false);
                    agg_gui::animation::request_draw();
                }),
        ));
        column = column.add(Box::new(header));
    }

    // The heat-map staves (white page, fixed height), treble over bass —
    // the one place the Swift controller was `scrollable: true`: the
    // multi-system staff scrolls inside its fixed height.
    for controller in [treble_controller, bass_controller] {
        column = column.add(Box::new(
            SizedBox::new().with_height(STAFF_HEIGHT).with_child(Box::new(
                NotationWidget::new(controller, Rc::clone(clock)).with_fit(NotationFit::Page),
            )),
        ));
    }
    column = column.add(Box::new(Separator::horizontal().with_line_inset(0.0)));

    // The list sections.
    let mut list = FlexColumn::new().with_gap(4.0).with_padding(14.0);
    list = list.add(Box::new(rows::section_header("Recent exercises", fonts)));
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
        list = list.add(rows::history_row(engine, fonts, visible, record));
    }

    for (title, staff_entries) in [
        ("Notes · right hand", &entries),
        ("Notes · left hand", &bass_entries),
    ] {
        list = list.add(Box::new(rows::section_header(title, fonts)));
        for entry in staff_entries.iter().filter(|e| e.unlocked) {
            list = list.add(rows::note_row(fonts, entry));
        }
    }

    if !transitions.is_empty() {
        list = list.add(Box::new(rows::section_header("Trouble transitions", fonts)));
        for entry in &transitions {
            list = list.add(rows::transition_row(fonts, entry));
        }
    }

    list = list.add(Box::new(rows::section_header("Intervals", fonts)));
    for entry in intervals.iter().filter(|e| e.attempts > 0) {
        list = list.add(rows::interval_row(fonts, entry));
    }

    list = list.add(Box::new(rows::section_header("Chords", fonts)));
    for entry in &chords {
        list = list.add(rows::chord_row(fonts, entry));
    }
    column = column.add_flex(Box::new(ScrollView::new(Box::new(list))), 1.0);

    // Footer: mastery tally over both staves + next unlock per ladder.
    column = column.add(Box::new(Separator::horizontal().with_line_inset(0.0)));
    column = column.add(Box::new(
        FlexRow::new()
            .with_gap(10.0)
            .with_padding(14.0)
            .add(Box::new(
                Label::new(
                    format::mastery_tally(&entries, &bass_entries),
                    Arc::clone(&fonts.regular),
                )
                .with_font_size(size::CALLOUT)
                .with_dim(true),
            ))
            .add_flex(Box::new(crate::ui::hspacer()), 1.0)
            .add(Box::new(
                Label::new(next_unlock, Arc::clone(&fonts.regular))
                    .with_font_size(size::CALLOUT)
                    .with_dim(true),
            )),
    ));

    Box::new(column)
}

/// Layout regression tests: the sheet's chrome (header, the two fixed
/// heat staves, the footer) must keep its own height so the scrolling
/// list gets the rest of the panel — the Library search-box failure
/// class, where one row reports "fill everything" and shoves the
/// content off the sheet.
#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::ui::sheets::layout_test_support::{
        contains, describe, first_of_type, laid_out_nodes, node_with_property,
        node_with_property_prefix, nodes_outside_panel, panel_rect, shared_test_engine, test_clock,
        WINDOW,
    };
    use crate::ui::side_panel::{open_cell, SidePanelCells};
    use agg_gui::geometry::Rect;
    use agg_gui::widget::InspectorNode;

    /// Build the sheet and open it exactly as the bar's Progress button
    /// does: bump the generation, then set the visibility cell.
    fn opened_sheet_nodes() -> Vec<InspectorNode> {
        let engine = shared_test_engine();
        let fonts = UiFonts::bundled();
        let cells = SidePanelCells::new();
        let clock = test_clock();
        let mut sheet = build_progress_sheet(&engine, &fonts, &clock, &cells);

        cells.progress_generation.set(cells.progress_generation.get() + 1);
        open_cell(&cells.show_progress);
        laid_out_nodes(&mut sheet)
    }

    /// The Swift `idealWidth: 860, idealHeight: 720` frame, clamped to
    /// the window (the sheet is taller than a 640-high window).
    #[test]
    fn panel_is_the_swift_ideal_frame_clamped_to_the_window() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);
        assert_eq!(panel.width, SHEET_SIZE.width);
        assert_eq!(panel.height, SHEET_SIZE.height.min(WINDOW.height - 48.0));
        assert!(
            contains(Rect::new(0.0, 0.0, WINDOW.width, WINDOW.height), panel),
            "panel {panel:?} must sit inside the window"
        );
    }

    /// The header row is title-high — it must not grow into the staves
    /// and the list below it.
    #[test]
    fn header_keeps_its_own_height_with_done_on_the_panel() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);
        let header = first_of_type(&nodes, "FlexRow").screen_bounds;
        // The Swift header is a title (26pt) inside a 14pt-padded row:
        // 54pt. Anything materially taller means the row stretched into
        // the content below it.
        assert!(
            header.height <= 60.0,
            "the header must keep its own height, got {header:?} of the panel {panel:?}"
        );
        let done = node_with_property(&nodes, "label", "Done").screen_bounds;
        assert!(
            done.width > 0.0 && done.height > 0.0 && contains(panel, done),
            "Done {done:?} must sit inside the panel {panel:?}"
        );
    }

    /// One `.frame(height: 150)` heat staff per hand, both on the panel.
    #[test]
    fn both_heat_staves_keep_their_fixed_height() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);
        let staves: Vec<Rect> = nodes
            .iter()
            .filter(|node| node.type_name == "NotationWidget")
            .map(|node| node.screen_bounds)
            .collect();
        assert_eq!(staves.len(), 2, "one heat staff per hand");
        for staff in staves {
            assert_eq!(staff.height, STAFF_HEIGHT);
            assert!(
                contains(panel, staff),
                "heat staff {staff:?} must sit inside the panel {panel:?}"
            );
        }
    }

    /// The list scrolls in what's left between the staves and the
    /// footer, with real rows visible and the footer below it.
    #[test]
    fn list_rows_and_footer_share_the_panel_below_the_staves() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);

        let scroll = first_of_type(&nodes, "ScrollView").screen_bounds;
        assert!(
            scroll.height > 100.0,
            "the list needs the room left below the staves, got {scroll:?}"
        );
        assert!(
            contains(panel, scroll),
            "the list {scroll:?} must sit inside the panel {panel:?}"
        );

        // The first section header and the first note row are visible in
        // the list's viewport, not scrolled off or zero-sized.
        for text in ["Recent exercises", "Notes · right hand"] {
            let row = node_with_property_prefix(&nodes, "text", text).screen_bounds;
            assert!(
                row.width > 0.0 && row.height > 0.0 && contains(scroll, row),
                "{text:?} at {row:?} must be visible in the list viewport {scroll:?}"
            );
        }

        // The unlock footer sits on the panel, below the list.
        let footer = node_with_property_prefix(&nodes, "text", "Next unlock").screen_bounds;
        assert!(
            footer.width > 0.0 && footer.height > 0.0,
            "the unlock footer must have a size, got {footer:?}"
        );
        assert!(
            contains(panel, footer),
            "the unlock footer {footer:?} must sit inside the panel {panel:?}"
        );
        assert!(
            footer.y + footer.height <= scroll.y + 0.5,
            "the footer {footer:?} belongs below the list {scroll:?}"
        );
    }

    /// Nothing outside the list's clipped content may leave the panel.
    #[test]
    fn nothing_is_laid_out_off_the_panel() {
        let nodes = opened_sheet_nodes();
        let outside = nodes_outside_panel(&nodes);
        assert!(
            outside.is_empty(),
            "widgets laid out off the panel:\n{}",
            describe(&outside)
        );
    }
}
