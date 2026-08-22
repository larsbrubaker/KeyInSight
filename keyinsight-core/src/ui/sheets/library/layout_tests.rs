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

use agg_gui::geometry::{Point, Rect, Size};
use agg_gui::widget::{collect_inspector_nodes, InspectorNode};

use super::{build_library_sheet, SHEET_SIZE};
use crate::ui::app::{KeyInSightPlatform, SharedPlatform};
use crate::ui::fonts::UiFonts;
use crate::ui::side_panel::{open_cell, test_engine, SidePanelCells};

/// A window roughly the size the native shell opens at.
const WINDOW: Size = Size {
    width: 1180.0,
    height: 640.0,
};

/// The headless platform: no storage, no audio, no file picker.
struct TestPlatform;
impl KeyInSightPlatform for TestPlatform {}

/// Build the sheet, open it exactly as `KeyInSightHandles::open_library`
/// does (bump the generation, then set the visibility cell), lay it out
/// at [`WINDOW`], and return the flat screen-space widget snapshot.
fn opened_sheet_nodes() -> Vec<InspectorNode> {
    let engine = Rc::new(RefCell::new(test_engine()));
    let fonts = UiFonts::bundled();
    let cells = SidePanelCells::new();
    let platform: SharedPlatform = Rc::new(TestPlatform);
    let mut sheet = build_library_sheet(&engine, &fonts, &cells, &platform);

    cells.library_generation.set(cells.library_generation.get() + 1);
    open_cell(&cells.show_library);

    sheet.layout(WINDOW);
    sheet.set_bounds(Rect::new(0.0, 0.0, WINDOW.width, WINDOW.height));
    // A second pass mirrors the app loop, where layout runs every frame
    // against bounds the previous pass established.
    sheet.layout(WINDOW);

    let mut nodes = Vec::new();
    collect_inspector_nodes(sheet.as_ref(), 0, Point::new(0.0, 0.0), &mut nodes);
    nodes
}

/// The first node under the sheet root is the panel column; its rect is
/// the visible area everything else has to fit inside.
fn panel_rect(nodes: &[InspectorNode]) -> Rect {
    nodes
        .iter()
        .find(|node| node.depth == 1)
        .expect("the sheet panel is laid out")
        .screen_bounds
}

fn first_of_type<'a>(nodes: &'a [InspectorNode], type_name: &str) -> &'a InspectorNode {
    nodes
        .iter()
        .find(|node| node.type_name == type_name)
        .unwrap_or_else(|| panic!("the sheet contains a {type_name}"))
}

/// `inner` lies entirely inside `outer` (Y-up screen coordinates).
fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x - 0.5
        && inner.y >= outer.y - 0.5
        && inner.x + inner.width <= outer.x + outer.width + 0.5
        && inner.y + inner.height <= outer.y + outer.height + 0.5
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
