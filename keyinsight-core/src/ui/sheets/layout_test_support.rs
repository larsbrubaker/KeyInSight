//! Shared helpers for the sheets' layout regression tests.
//!
//! Every sheet is a fixed-size [`ModalSheet`](agg_gui::widgets::ModalSheet)
//! panel built from flex containers, and the whole family shares one
//! failure mode: a chrome row that reports "fill everything" instead of
//! its own height swallows the panel and pushes the real content off the
//! bottom (the Library search-box bug). These helpers open a sheet the
//! way its bar button does, lay it out at the native window size, and
//! hand back the flat screen-space widget snapshot to assert on.

use std::cell::RefCell;
use std::rc::Rc;

use agg_gui::geometry::{Point, Rect, Size};
use agg_gui::widget::{collect_inspector_nodes, InspectorNode, Widget};

/// A window roughly the size the native shell opens at.
pub(crate) const WINDOW: Size = Size {
    width: 1180.0,
    height: 640.0,
};

/// A clock for the sheets that animate; headless tests never advance it.
pub(crate) fn test_clock() -> super::Clock {
    Rc::new(|| 0.0)
}

pub(crate) fn shared_test_engine() -> super::Engine {
    Rc::new(RefCell::new(crate::ui::side_panel::test_engine()))
}

/// Lay `sheet` out at [`WINDOW`] and return the flat screen-space widget
/// snapshot. The caller has already flipped the visibility cell, exactly
/// as the bar button does.
pub(crate) fn laid_out_nodes(sheet: &mut Box<dyn Widget>) -> Vec<InspectorNode> {
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
pub(crate) fn panel_rect(nodes: &[InspectorNode]) -> Rect {
    nodes
        .iter()
        .find(|node| node.depth == 1)
        .expect("the sheet panel is laid out")
        .screen_bounds
}

pub(crate) fn first_of_type<'a>(nodes: &'a [InspectorNode], type_name: &str) -> &'a InspectorNode {
    nodes
        .iter()
        .find(|node| node.type_name == type_name)
        .unwrap_or_else(|| panic!("the sheet contains a {type_name}"))
}

/// Every node of `type_name`, in depth-first order.
pub(crate) fn all_of_type<'a>(
    nodes: &'a [InspectorNode],
    type_name: &str,
) -> Vec<&'a InspectorNode> {
    nodes
        .iter()
        .filter(|node| node.type_name == type_name)
        .collect()
}

/// The value of an inspector property (`"text"` on a Label, `"label"`
/// on a Button), if the node carries it.
pub(crate) fn property<'a>(node: &'a InspectorNode, name: &str) -> Option<&'a str> {
    node.properties
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
}

/// The first node whose inspector property `name` equals `value` — how
/// the tests find a specific button or label by its user-visible text.
pub(crate) fn node_with_property<'a>(
    nodes: &'a [InspectorNode],
    name: &str,
    value: &str,
) -> &'a InspectorNode {
    nodes
        .iter()
        .find(|node| property(node, name) == Some(value))
        .unwrap_or_else(|| panic!("the sheet contains a widget with {name} = {value:?}"))
}

/// The first node whose inspector property `name` starts with `prefix`.
pub(crate) fn node_with_property_prefix<'a>(
    nodes: &'a [InspectorNode],
    name: &str,
    prefix: &str,
) -> &'a InspectorNode {
    nodes
        .iter()
        .find(|node| property(node, name).is_some_and(|text| text.starts_with(prefix)))
        .unwrap_or_else(|| panic!("the sheet contains a widget with {name} starting {prefix:?}"))
}

/// `inner` lies entirely inside `outer` (Y-up screen coordinates).
pub(crate) fn contains(outer: Rect, inner: Rect) -> bool {
    contains_within(outer, inner, 0.5)
}

/// [`contains`] with an explicit slack, in logical pixels.
pub(crate) fn contains_within(outer: Rect, inner: Rect, slack: f64) -> bool {
    inner.x >= outer.x - slack
        && inner.y >= outer.y - slack
        && inner.x + inner.width <= outer.x + outer.width + slack
        && inner.y + inner.height <= outer.y + outer.height + slack
}

/// agg-gui snaps every child rect to whole pixels (`FlexColumn::layout`
/// rounds both the child's origin and its height), so a row whose
/// natural height lands on a half pixel — a 53.5-high title row — is
/// placed one pixel past its slot. That is pixel snapping, not a layout
/// error; anything beyond it is.
const PIXEL_SNAP_SLACK: f64 = 1.0;

/// Every node laid out (partly) outside the panel, ignoring a
/// `ScrollView`'s clipped content — scrolled children legitimately
/// extend past their viewport. A non-empty result is the Library
/// search-box failure: a chrome row swelled and shoved content off the
/// sheet.
pub(crate) fn nodes_outside_panel(nodes: &[InspectorNode]) -> Vec<&InspectorNode> {
    let panel = panel_rect(nodes);
    let mut offenders = Vec::new();
    let mut scroll_depth: Option<usize> = None;
    // Skip the sheet root itself: it spans the whole window (the scrim).
    for node in nodes.iter().skip(1) {
        if let Some(depth) = scroll_depth {
            if node.depth > depth {
                continue;
            }
            scroll_depth = None;
        }
        if node.type_name == "ScrollView" {
            scroll_depth = Some(node.depth);
        }
        if !contains_within(panel, node.screen_bounds, PIXEL_SNAP_SLACK) {
            offenders.push(node);
        }
    }
    offenders
}

/// `type_name [x,y wxh] "text"` for each node — the failure message that
/// makes an off-panel widget obvious.
pub(crate) fn describe(nodes: &[&InspectorNode]) -> String {
    nodes
        .iter()
        .map(|node| {
            let text = property(node, "text")
                .or_else(|| property(node, "label"))
                .unwrap_or("");
            let b = node.screen_bounds;
            format!(
                "{} [{:.1},{:.1} {:.1}x{:.1}] {text:?}",
                node.type_name, b.x, b.y, b.width, b.height
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
