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
    /// A bare `Text` in the section (callout secondary).
    Paragraph(&'static str),
}

pub(crate) const SECTIONS: &[(&str, &[Block])] = &[
    (
        "The model",
        &[Block::Paragraph(
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
                Button::new("Done", Arc::clone(&fonts.regular))
                    .with_subtle()
                    .with_active_fn(|| false)
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
                    Block::Paragraph(text) => ("", *text),
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
                Block::Paragraph(_) => None,
            })
            .collect();
        assert_eq!(
            modes,
            ["Training", "Drill", "Survival", "Tempo pacing", "Free Play", "Library"]
        );
    }
}
