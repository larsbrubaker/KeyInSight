//! Layout regression tests for the Library sheet.
//!
//! The sheet's chrome is built out of flex containers whose natural
//! heights decide how much room is left for the song list. When one of
//! the fixed rows reports "fill everything" instead of its own height,
//! the list and the footer are pushed off the bottom of the 700×560
//! panel and the sheet renders as a header plus a full-height search
//! box — the `--library` screenshot bug. These tests lay the real sheet
//! out and assert on the resulting screen rectangles.

use std::cell::RefCell;
use std::rc::Rc;

use agg_gui::geometry::Rect;
use agg_gui::widget::InspectorNode;

use super::{build_library_sheet, SHEET_SIZE};
use crate::ui::app::{KeyInSightPlatform, SharedPlatform};
use crate::ui::fonts::UiFonts;
use crate::ui::sheets::layout_test_support::{
    contains, describe, first_of_type, laid_out_nodes, nodes_outside_panel, panel_rect, WINDOW,
};
use crate::ui::side_panel::{open_cell, test_engine, SidePanelCells};

/// The headless platform: no storage, no audio, no file picker.
struct TestPlatform;
impl KeyInSightPlatform for TestPlatform {}

/// Build the sheet, open it exactly as `KeyInSightHandles::open_library`
/// does (bump the generation, then set the visibility cell), and lay it
/// out at the shared [`WINDOW`] size.
fn opened_sheet_nodes() -> Vec<InspectorNode> {
    let engine = Rc::new(RefCell::new(test_engine()));
    let fonts = UiFonts::bundled();
    let cells = SidePanelCells::new();
    let platform: SharedPlatform = Rc::new(TestPlatform);
    let mut sheet = build_library_sheet(&engine, &fonts, &cells, &platform);

    cells.library_generation.set(cells.library_generation.get() + 1);
    open_cell(&cells.show_library);
    laid_out_nodes(&mut sheet)
}

/// The panel is the Swift `idealWidth: 700, idealHeight: 560` frame,
/// centered in the window.
#[test]
fn panel_is_the_swift_ideal_frame() {
    let nodes = opened_sheet_nodes();
    let panel = panel_rect(&nodes);
    assert_eq!(panel.width, SHEET_SIZE.width);
    assert_eq!(panel.height, SHEET_SIZE.height);
    assert!(
        contains(Rect::new(0.0, 0.0, WINDOW.width, WINDOW.height), panel),
        "panel {panel:?} must sit inside the window"
    );
}

/// The hands filter is a real control on the toolbar row, not a
/// zero-sized or off-panel widget.
#[test]
fn hands_filter_lays_out_inside_the_panel() {
    let nodes = opened_sheet_nodes();
    let panel = panel_rect(&nodes);
    let segmented = first_of_type(&nodes, "SegmentedControl").screen_bounds;
    assert!(
        segmented.width > 0.0 && segmented.height > 0.0,
        "the All/One hand/Two hands filter must have a non-zero size, got {segmented:?}"
    );
    assert!(
        contains(panel, segmented),
        "the hands filter {segmented:?} must sit inside the panel {panel:?}"
    );
}

/// The toolbar row keeps its natural height so the list below it gets
/// the rest of the panel — the regression that made the search box fill
/// the sheet.
#[test]
fn song_list_gets_the_space_below_the_toolbar() {
    let nodes = opened_sheet_nodes();
    let panel = panel_rect(&nodes);
    let scroll = first_of_type(&nodes, "ScrollView").screen_bounds;
    assert!(
        scroll.height > SHEET_SIZE.height / 2.0,
        "the song list must own most of the panel height, got {scroll:?}"
    );
    assert!(
        contains(panel, scroll),
        "the song list {scroll:?} must sit inside the panel {panel:?}"
    );
}

/// At least one bundled song row is laid out where the user can see it.
#[test]
fn at_least_one_song_row_is_visible_in_the_panel() {
    let nodes = opened_sheet_nodes();
    let panel = panel_rect(&nodes);
    let scroll_index = nodes
        .iter()
        .position(|node| node.type_name == "ScrollView")
        .expect("the sheet contains a ScrollView");
    let scroll_depth = nodes[scroll_index].depth;
    // Depth-first order: the ScrollView's descendants follow it until the
    // depth drops back to its own.
    let rows = nodes[scroll_index + 1..]
        .iter()
        .take_while(|node| node.depth > scroll_depth)
        .filter(|node| {
            node.depth == scroll_depth + 2
                && node.screen_bounds.width > 0.0
                && node.screen_bounds.height > 0.0
                && contains(panel, node.screen_bounds)
        })
        .count();
    assert!(
        rows > 0,
        "at least one song row must be laid out inside the panel {panel:?}"
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
