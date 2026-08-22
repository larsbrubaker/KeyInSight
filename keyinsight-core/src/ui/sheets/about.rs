//! The About/Method sheet — `UI/AboutSheet.swift` (OQ-25): what this
//! trainer believes and the vocabulary it uses — the two beginner reading
//! skills, how progression works, and what each mode trains.
//!
//! The Swift `term` bodies are inline-Markdown (`*italics*`); agg-gui
//! labels are plain text, so the emphasis asterisks are stripped and the
//! words stand unmarked (see `docs/platform-substitutions.md`).

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::geometry::Size;
use agg_gui::widget::Widget;
use agg_gui::widgets::{Button, FlexColumn, FlexRow, Label, ModalSheet, ScrollView, Separator};

use crate::ui::fonts::{size, UiFonts};
use crate::ui::side_panel::SidePanelCells;

/// The Swift ideal frame (`idealWidth: 620, idealHeight: 640`).
const SHEET_SIZE: Size = Size {
    width: 620.0,
    height: 640.0,
};

/// The sheet's copy, section by section: `(title, [Term(name, body) |
/// Paragraph(text)])`. Swift's `\`-continued string literals are joined
/// into single paragraphs here.
pub(crate) enum Block {
    /// `term(name, body)` — bold name over a secondary body.
    Term(&'static str, &'static str),
    /// A bare `Text` in the section styled `.callout` + `.secondary`.
    Paragraph(&'static str),
    /// A bare, unstyled `Text` — body font, primary color.
    Body(&'static str),
}

pub(crate) const SECTIONS: &[(&str, &[Block])] = &[
    (
        "The model",
        &[Block::Body(
            "KeyInSight works like a typing tutor for the staff: your skills are tracked as small items, exercises are generated at your current frontier with your weak items resurfaced, and new material unlocks itself when what's active is mastered — accuracy and speed, measured continuously. You never pick a level.",
        )],
    ),
    (
        "Two reading skills",
        &[
            Block::Term(
                "Note-naming fluency",
                "Seeing a notehead and knowing the key, cold. Built by retrieval practice until it's automatic; the Drill's shrinking timer is rate building — in learning science, \"fluency\" means accuracy plus speed.",
            ),
            Block::Term(
                "Intervallic reading",
                "Reading the next note as a move — up a step, down a 3rd — and finding it by feel (keyboard geography). Fluent sight-readers read this way, sustained by eye–hand span (how far the eyes run ahead of the hands) and chunking. Longer, continuous material trains this; Survival mode exists for exactly that.",
            ),
            Block::Paragraph(
                "A 12-note phrase is one absolute read plus eleven relative moves; twelve drill cards are twelve absolute reads. Exercise length is the dial between the two skills.",
            ),
        ],
    ),
    (
        "How progression works",
        &[
            Block::Term(
                "Items",
                "Pitches per staff (\"treble G4\"), interval shapes (\"down a 3rd\"), specific transitions (\"F#4→B4\"), chord shapes (\"harmonic 5th\"), rhythm values, and tempo — each tracked with an error rate and response time that favor recent plays.",
            ),
            Block::Term(
                "Mastery & unlocks",
                "When every active item on a ladder is accurate and fast, the next item joins. Range grows outward from C4–G4 (and C3–G3 for the left hand), leaps grow from 4ths toward octaves, chords from open 5ths toward triads.",
            ),
            Block::Term(
                "Readiness probes",
                "Before something new formally unlocks, exercises occasionally slip one instance in — a single wide leap, one dyad. Handle the probes well and the unlock follows. (Precision teaching calls these \"probes\": testing untaught material to detect readiness.)",
            ),
            Block::Term(
                "Scaffold fading",
                "Helpers like the keyboard strip and octave-following are training wheels — they're designed to disappear as mastery arrives, not to stay on forever.",
            ),
        ],
    ),
    (
        "What each mode trains",
        &[
            Block::Term(
                "Training",
                "The adaptive loop: both skills, weak items drilled hardest.",
            ),
            Block::Term(
                "Drill",
                "Note-naming fluency under time pressure (rate building).",
            ),
            Block::Term(
                "Survival",
                "Fluency assessment: endless reading at your level with neutral bias, three lives, and a score — notes × notes per minute × difficulty. It measures; Training improves.",
            ),
            Block::Term(
                "Tempo pacing",
                "Timing and rhythm against the metronome; rhythm vocabulary unlocks here fastest.",
            ),
            Block::Term(
                "Free Play",
                "A notation mirror of whatever you play, recorded and replayable.",
            ),
            Block::Term(
                "Library",
                "Real pieces — apply the skills; click any note to practice from that spot.",
            ),
        ],
    ),
];

pub fn build_about_sheet(fonts: &UiFonts, cells: &SidePanelCells) -> Box<dyn Widget> {
    let visible = Rc::clone(&cells.show_about);
    let mut column = FlexColumn::new().with_gap(0.0);

    // Header: title + Done (padding 14).
    {
        let close = Rc::clone(&visible);
        let header = FlexRow::new()
            .with_gap(8.0)
            .with_padding(14.0)
            .add(Box::new(
                Label::new("How This Trainer Works", Arc::clone(&fonts.bold))
                    .with_font_size(size::TITLE2),
            ))
            .add_flex(Box::new(crate::ui::hspacer()), 1.0)
            .add(Box::new(
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
    column = column.add(Box::new(Separator::horizontal().with_line_inset(0.0)));

    // ScrollView of VStack(alignment: .leading, spacing: 18).
    let mut body = FlexColumn::new().with_gap(18.0).with_padding(14.0);
    for (title, blocks) in SECTIONS {
        body = body.add(Box::new(section(fonts, title, blocks)));
    }
    column = column.add_flex(Box::new(ScrollView::new(Box::new(body))), 1.0);

    Box::new(ModalSheet::new(visible, Box::new(column)).with_panel_size(SHEET_SIZE))
}

/// `section(title) { content }` — headline title over its blocks, spacing 8.
fn section(fonts: &UiFonts, title: &str, blocks: &[Block]) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(8.0).add(Box::new(
        Label::new(title, Arc::clone(&fonts.bold)).with_font_size(size::BODY),
    ));
    for block in blocks {
        match block {
            Block::Term(name, body) => column = column.add(Box::new(term(fonts, name, body))),
            Block::Paragraph(text) => column = column.add(Box::new(paragraph(fonts, text))),
            Block::Body(text) => {
                column = column.add(Box::new(
                    Label::new(*text, Arc::clone(&fonts.regular))
                        .with_font_size(size::BODY)
                        .with_wrap(true),
                ))
            }
        }
    }
    column
}

/// `term(name, body)` — callout bold name over callout secondary body,
/// spacing 2.
fn term(fonts: &UiFonts, name: &str, body: &str) -> FlexColumn {
    FlexColumn::new()
        .with_gap(2.0)
        .add(Box::new(
            Label::new(name, Arc::clone(&fonts.bold)).with_font_size(size::CALLOUT),
        ))
        .add(Box::new(paragraph(fonts, body)))
}

/// A wrapped callout-secondary paragraph.
fn paragraph(fonts: &UiFonts, text: &str) -> Label {
    Label::new(text, Arc::clone(&fonts.regular))
        .with_font_size(size::CALLOUT)
        .with_dim(true)
        .with_wrap(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Swift copy is ported verbatim minus the inline-Markdown
    /// emphasis; no asterisks may survive and the section order is fixed.
    #[test]
    fn about_copy_is_plain_text_in_swift_order() {
        let titles: Vec<&str> = SECTIONS.iter().map(|(title, _)| *title).collect();
        assert_eq!(
            titles,
            [
                "The model",
                "Two reading skills",
                "How progression works",
                "What each mode trains"
            ]
        );
        for (_, blocks) in SECTIONS {
            for block in *blocks {
                let (name, body) = match block {
                    Block::Term(name, body) => (*name, *body),
                    Block::Paragraph(text) | Block::Body(text) => ("", *text),
                };
                assert!(!name.contains('*') && !body.contains('*'), "markdown left in {name:?}");
                assert!(!body.contains("  "), "line-continuation double space in {name:?}");
            }
        }
        let modes: Vec<&str> = SECTIONS[3]
            .1
            .iter()
            .filter_map(|b| match b {
                Block::Term(name, _) => Some(*name),
                Block::Paragraph(_) | Block::Body(_) => None,
            })
            .collect();
        assert_eq!(
            modes,
            ["Training", "Drill", "Survival", "Tempo pacing", "Free Play", "Library"]
        );
        // "The model" is plain body text; the skills coda is the callout.
        assert!(matches!(SECTIONS[0].1, [Block::Body(_)]));
        assert!(matches!(SECTIONS[1].1.last(), Some(Block::Paragraph(_))));
    }
}

/// Layout regression tests: the header keeps its own height so the body
/// scrolls in the rest of the panel (the Library search-box failure
/// class, where a chrome row swells and shoves content off the sheet).
#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::ui::sheets::layout_test_support::{
        contains, describe, first_of_type, laid_out_nodes, node_with_property,
        node_with_property_prefix, nodes_outside_panel, panel_rect, WINDOW,
    };
    use crate::ui::side_panel::{open_cell, SidePanelCells};
    use agg_gui::geometry::Rect;
    use agg_gui::widget::InspectorNode;

    /// Build the sheet and open it exactly as the bar's About button does.
    fn opened_sheet_nodes() -> Vec<InspectorNode> {
        let fonts = UiFonts::bundled();
        let cells = SidePanelCells::new();
        let mut sheet = build_about_sheet(&fonts, &cells);
        open_cell(&cells.show_about);
        laid_out_nodes(&mut sheet)
    }

    /// The Swift `idealWidth: 620, idealHeight: 640` frame, clamped to
    /// the window.
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

    /// The header row is title-high, with Done reachable on the panel.
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

    /// The scrolling body owns the panel below the header, with the first
    /// section title and its copy visible in the viewport.
    #[test]
    fn body_text_is_visible_in_the_scrolling_area() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);
        let scroll = first_of_type(&nodes, "ScrollView").screen_bounds;
        assert!(
            scroll.height > panel.height / 2.0,
            "the body must own most of the panel height, got {scroll:?}"
        );
        assert!(
            contains(panel, scroll),
            "the body {scroll:?} must sit inside the panel {panel:?}"
        );

        let title = node_with_property(&nodes, "text", SECTIONS[0].0).screen_bounds;
        let body = node_with_property_prefix(&nodes, "text", "KeyInSight works like").screen_bounds;
        for (name, rect) in [("the first section title", title), ("its body copy", body)] {
            assert!(
                rect.width > 0.0 && rect.height > 0.0 && contains(scroll, rect),
                "{name} at {rect:?} must be visible in the viewport {scroll:?}"
            );
        }
    }

    /// Nothing outside the body's clipped content may leave the panel.
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
