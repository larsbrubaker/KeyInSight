//! The feedback overlays that ride over the engraving: the wrong-answer
//! ghost note with its ledger lines, and the tempo-mode timing ticks.
//!
//! Ports the HTML overlay half of `Notation/NotationController.swift` —
//! the `.ghost-note` / `.ghost-ledger` / `.tick` CSS rules and the
//! `showGhost` / `addTick` positioning script. Those are absolutely
//! positioned CSS boxes over the SVG, i.e. *screen* pixels that do not
//! scale with the engraving, so everything here works in widget space
//! (y-up screen px) and keeps each CSS length verbatim.

use std::f64::consts::PI;

use agg_gui::color::Color;
use agg_gui::draw_ctx::DrawCtx;
use agg_gui::geometry::Rect;
use verovio_rust::{ElementKind, Layout, Primitive};

use super::Placement;
use crate::notation::NotationController;

/// `.ghost-note { border: 2.5px solid #8a8a8a; box-sizing: border-box }`:
/// the border's outer edge is the notehead box, so the stroke's centre
/// line runs half a stroke inside it.
const GHOST_STROKE: f64 = 2.5;
/// `border: … solid #8a8a8a` and `background: rgba(138,138,138,0.25)`.
const GHOST_INK: Color = Color::from_rgb8(0x8A, 0x8A, 0x8A);
const GHOST_FILL: Color = Color::from_rgba8(0x8A, 0x8A, 0x8A, 64); // 0.25 × 255
/// `transform: rotate(-20deg)` turns the oval counter-clockwise on
/// screen; widget space is y-up, so that is a positive rotation here.
const GHOST_ROTATION: f64 = 20.0 * PI / 180.0;
/// `.ghost-ledger { height: 2px; background: #9a9a9a }`, laid out
/// `headWidth * 1.8` wide and centred on the notehead (`addLedger`).
const LEDGER_THICKNESS: f64 = 2.0;
const LEDGER_WIDTH_FACTOR: f64 = 1.8;
const LEDGER_INK: Color = Color::from_rgb8(0x9A, 0x9A, 0x9A);
/// `.tick { font: bold 15px …; color: #b8860b }`, placed by `addTick` at
/// `left = notehead centre − 6`, `top = notehead top − 24`.
const TICK_INK: Color = Color::from_rgb8(0xB8, 0x86, 0x0B);
const TICK_FONT_PX: f64 = 15.0;
const TICK_ABOVE: f64 = 24.0;
const TICK_LEFT: f64 = 6.0;
/// macOS drew the ticks as SF Pro's `◂` / `▸` (U+25C2 / U+25B8). No
/// bundled face carries them — Inter stops at the full-size U+25C0 /
/// U+25B6 — so they are painted as paths matching the ink SF Pro put on
/// screen (`reference/swift/notation/tempo-complete.png`, a 2× capture:
/// a 4.5 px triangle, 1.75 px in from the text box's left edge and 8.5 px
/// below its top, at `font-size: 15px`).
const TICK_INK_SIZE: f64 = 0.30 * TICK_FONT_PX;
const TICK_INK_DX: f64 = 1.75;
const TICK_INK_DY: f64 = 8.5;

/// Paint the ghost and the ticks over an already-engraved score.
/// `staff_space` is the engraving's staff space in layout px.
pub(super) fn paint(
    ctx: &mut dyn DrawCtx,
    controller: &NotationController,
    layout: &Layout,
    placement: Placement,
    staff_space: f64,
) {
    let space = staff_space * placement.scale;
    if let Some(ghost) = controller.ghost() {
        if let Some(&bounds) = layout.bounds_by_id.get(&ghost.expected_id) {
            let head = placement.widget_rect(bounds);
            let oval = ghost_oval(head, ghost.offset_steps, space);
            // The Swift page measured the five lines of the document's first
            // `g.staff`; here the ghost reads the staff its own note sits
            // on, so a grand staff or a wrapped page ledgers correctly.
            if let Some((top, bottom)) = staff_lines_at(layout, bounds, staff_space) {
                for ledger in ghost_ledgers(
                    oval,
                    placement.widget_y(top),
                    placement.widget_y(bottom),
                    space,
                ) {
                    ctx.set_fill_color(LEDGER_INK);
                    ctx.begin_path();
                    ctx.rect(ledger.x, ledger.y, ledger.width, ledger.height);
                    ctx.fill();
                }
            }
            paint_oval(ctx, oval);
        }
    }
    for tick in controller.ticks() {
        if let Some(&bounds) = layout.bounds_by_id.get(&tick.id) {
            paint_tick(ctx, tick_ink(placement.widget_rect(bounds)), tick.early);
        }
    }
}

/// `showGhost`: the played note's head — the expected note's box moved
/// `offset_steps` diatonic steps, half a staff space each (positive =
/// played higher, i.e. up the screen).
pub(super) fn ghost_oval(head: Rect, offset_steps: i32, space: f64) -> Rect {
    let cy = head.center().y + offset_steps as f64 * space / 2.0;
    Rect::new(head.x, cy - head.height / 2.0, head.width, head.height)
}

/// One ledger line per staff space between the staff and the ghost, from
/// the first space beyond the staff out to the ghost itself (`showGhost`'s
/// two `addLedger` loops). `staff_top` / `staff_bottom` are the outer
/// staff lines in widget space (y-up, so `staff_top` is the larger).
pub(super) fn ghost_ledgers(oval: Rect, staff_top: f64, staff_bottom: f64, space: f64) -> Vec<Rect> {
    let mut ledgers = Vec::new();
    if space <= 0.0 {
        return ledgers;
    }
    let gy = oval.center().y;
    let width = oval.width * LEDGER_WIDTH_FACTOR;
    let left = oval.center().x - oval.width * (LEDGER_WIDTH_FACTOR / 2.0);
    // The page's ±1 px slack keeps a ledger that lands exactly on the
    // ghost from being lost to rounding.
    let mut y = staff_bottom - space;
    while y >= gy - 1.0 {
        ledgers.push(Rect::new(
            left,
            y - LEDGER_THICKNESS / 2.0,
            width,
            LEDGER_THICKNESS,
        ));
        y -= space;
    }
    let mut y = staff_top + space;
    while y <= gy + 1.0 {
        ledgers.push(Rect::new(
            left,
            y - LEDGER_THICKNESS / 2.0,
            width,
            LEDGER_THICKNESS,
        ));
        y += space;
    }
    ledgers
}

/// `addTick`: the ink box of the `◂` / `▸` glyph over a notehead.
pub(super) fn tick_ink(head: Rect) -> Rect {
    let x = head.center().x - TICK_LEFT + TICK_INK_DX;
    let top = head.top() + TICK_ABOVE - TICK_INK_DY;
    Rect::new(x, top - TICK_INK_SIZE, TICK_INK_SIZE, TICK_INK_SIZE)
}

/// The five staff lines of the staff an element sits on (layout px,
/// y-down): the outermost pair, i.e. the Swift page's `lineYs[0]` and
/// `lineYs[4]`. Staff lines are engraved one per line per staff per
/// system, spanning the system, so the ones over the element's x that run
/// contiguously at the staff spacing are its staff.
pub(super) fn staff_lines_at(
    layout: &Layout,
    bounds: (f64, f64, f64, f64),
    staff_space: f64,
) -> Option<(f64, f64)> {
    let (x, y_top, _, h) = bounds;
    let cy = y_top + h / 2.0;
    let mut ys: Vec<f64> = layout
        .elements
        .iter()
        .filter(|element| element.kind == ElementKind::StaffLine)
        .filter_map(|element| match element.primitive {
            Primitive::Line { x1, y1, x2, .. } if x1 <= x && x <= x2 => Some(y1),
            _ => None,
        })
        .collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    let nearest = ys
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - cy).abs().total_cmp(&(b.1 - cy).abs()))?
        .0;
    // Walk out while the lines keep coming one staff space apart; the gap
    // to the next staff (or system) is always wider.
    let reach = staff_space * 1.5;
    let mut first = nearest;
    while first > 0 && ys[first] - ys[first - 1] <= reach {
        first -= 1;
    }
    let mut last = nearest;
    while last + 1 < ys.len() && ys[last + 1] - ys[last] <= reach {
        last += 1;
    }
    Some((ys[first], ys[last]))
}

fn paint_oval(ctx: &mut dyn DrawCtx, oval: Rect) {
    // `box-sizing: border-box`: the box is the border's outer edge, so the
    // stroked path runs half a stroke inside it.
    let rx = (oval.width / 2.0 - GHOST_STROKE / 2.0).max(0.1);
    let ry = (oval.height / 2.0 - GHOST_STROKE / 2.0).max(0.1);
    let center = oval.center();
    ctx.save();
    ctx.translate(center.x, center.y);
    ctx.rotate(GHOST_ROTATION);
    ctx.set_fill_color(GHOST_FILL);
    ctx.set_stroke_color(GHOST_INK);
    ctx.set_line_width(GHOST_STROKE);
    ellipse_path(ctx, rx, ry);
    ctx.fill_and_stroke();
    ctx.restore();
}

/// An ellipse centred on the origin: four cubics, the usual `kappa`
/// circle approximation stretched to the two radii (agg-gui's `DrawCtx`
/// offers circles and rects, not ellipses).
fn ellipse_path(ctx: &mut dyn DrawCtx, rx: f64, ry: f64) {
    const K: f64 = 0.552_284_749_830_793_4;
    ctx.begin_path();
    ctx.move_to(rx, 0.0);
    ctx.cubic_to(rx, ry * K, rx * K, ry, 0.0, ry);
    ctx.cubic_to(-rx * K, ry, -rx, ry * K, -rx, 0.0);
    ctx.cubic_to(-rx, -ry * K, -rx * K, -ry, 0.0, -ry);
    ctx.cubic_to(rx * K, -ry, rx, -ry * K, rx, 0.0);
    ctx.close_path();
}

/// The `◂` / `▸` ink: a triangle filling the glyph's box, apex toward the
/// side it points at.
fn paint_tick(ctx: &mut dyn DrawCtx, ink: Rect, early: bool) {
    ctx.set_fill_color(TICK_INK);
    ctx.begin_path();
    if early {
        ctx.move_to(ink.x, ink.center().y);
        ctx.line_to(ink.right(), ink.top());
        ctx.line_to(ink.right(), ink.bottom());
    } else {
        ctx.move_to(ink.right(), ink.center().y);
        ctx.line_to(ink.x, ink.top());
        ctx.line_to(ink.x, ink.bottom());
    }
    ctx.close_path();
    ctx.fill();
}

#[cfg(test)]
mod tests;
