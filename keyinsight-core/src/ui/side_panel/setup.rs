//! The setup block — `// MARK: - Setup` in `UI/SidePanel.swift`: input
//! source, mic level, pacing, hands, octave readout + Calibrate….
//!
//! The three `Picker("…", …).pickerStyle(.segmented)`s render on macOS as
//! a row — the picker's label in body type, then the segmented track —
//! so each is a [`FlexRow`] of a [`Label`] and a [`SegmentedControl`]
//! (callout type: Inter runs wider than SF, and "Unplugged" has to clear
//! its share of the row; the segments size to their own labels, as
//! AppKit's segmented control does). The controls are bound to an
//! index cell the root refreshes from
//! the engine every frame (`engine_index_cell`), so external changes —
//! the persisted settings on start, a mode the engine declines — show up
//! without a rebuild; the user's pick maps back through the same tables.

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widget::Widget;
use agg_gui::widgets::{Button, Conditional, FlexColumn, FlexRow, Label, SegmentedControl, Tooltip};

use crate::engine::{HandMode, InputSource, PacingMode};
use crate::ui::fonts::{size, UiFonts};
use crate::ui::help;
use crate::ui::{palette, DynamicLabel, LevelMeter};

use super::cells::{engine_index_cell, mic_cell, open_cell, tempo_cell, training_cell};
use super::{Engine, SidePanelCells};

/// The `Picker("…")` labels macOS draws to the left of each segmented
/// track, in panel order.
pub(super) const PICKER_LABELS: [&str; 3] = ["Input", "Pacing", "Hands"];

/// Gap between a picker's label and its track (the macOS labeled-control
/// row).
const LABEL_GAP: f64 = 8.0;

/// One labeled segmented picker: `Text(label)` in body type, then the
/// control at its own width.
///
/// The track is `add`ed, not `add_flex`ed: AppKit sizes a segmented
/// control to its segments, so only the four-segment Input picker
/// reaches the panel edge and the shorter Pacing/Hands tracks stop where
/// their labels do (`reference/swift/window/training-default.png`).
fn picker_row(label: &str, fonts: &UiFonts, control: Box<dyn Widget>) -> FlexRow {
    FlexRow::new()
        .with_gap(LABEL_GAP)
        .add(Box::new(
            Label::new(label, Arc::clone(&fonts.regular)).with_font_size(size::BODY),
        ))
        .add(control)
}

/// `InputSource.allCases` — segment order of the Input picker.
pub(super) const INPUT_SOURCES: [InputSource; 4] = [
    InputSource::Midi,
    InputSource::Keyboard,
    InputSource::Microphone,
    InputSource::SelfVerify,
];

/// `PacingMode.allCases` — segment order of the Pacing picker.
pub(super) const PACING_MODES: [PacingMode; 2] = [PacingMode::SelfPaced, PacingMode::Tempo];

/// Segment index of `item` in `items` (0 when absent — the first
/// segment, like a Picker whose selection left its tag set).
pub(super) fn segment_index<T: PartialEq>(items: &[T], item: T) -> usize {
    items.iter().position(|i| *i == item).unwrap_or(0)
}

pub(super) fn setup_section(engine: &Engine, fonts: &UiFonts, cells: &SidePanelCells) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(8.0);

    // Input source picker.
    {
        let selected = engine_index_cell(engine, |e| {
            segment_index(&INPUT_SOURCES, e.input_source())
        });
        let click = Rc::clone(engine);
        let labels: Vec<&str> = INPUT_SOURCES.iter().map(|s| s.label()).collect();
        let picker = SegmentedControl::new(labels, selected, Arc::clone(&fonts.regular))
            .with_font_size(size::CALLOUT)
            .with_fit_width(true)
            .on_change(move |index| click.borrow_mut().set_input_source(INPUT_SOURCES[index]));
        column = column.add(Box::new(picker_row(
            PICKER_LABELS[0],
            fonts,
            Box::new(picker),
        )));
    }

    // Mic input level (visible on the microphone source).
    {
        let visible = mic_cell(engine);
        let level = Rc::clone(engine);
        let row = FlexRow::new()
            .with_gap(8.0)
            .add(Box::new(
                Label::new("Level", Arc::clone(&fonts.regular))
                    .with_font_size(size::CALLOUT)
                    .with_dim(true),
            ))
            .add_flex(
                Box::new(Tooltip::new(
                    Box::new(LevelMeter::new(move || level.borrow().mic_level())),
                    help::MIC_LEVEL,
                    Arc::clone(&fonts.regular),
                )),
                1.0,
            );
        column = column.add(Box::new(Conditional::new(visible, Box::new(row))));
    }

    // Pacing picker; disabled unless the source has exact timing and the
    // content is monophonic (the Swift `.disabled(...)` on the Picker).
    {
        let selected = engine_index_cell(engine, |e| segment_index(&PACING_MODES, e.mode()));
        let click = Rc::clone(engine);
        let enabled = Rc::clone(engine);
        let labels: Vec<&str> = PACING_MODES.iter().map(|m| m.label()).collect();
        let picker = SegmentedControl::new(labels, selected, Arc::clone(&fonts.regular))
            .with_font_size(size::CALLOUT)
            .with_fit_width(true)
            .with_enabled_fn(move || {
                let engine = enabled.borrow();
                engine.input_source().supports_timing() && engine.content_supports_tempo()
            })
            .on_change(move |index| click.borrow_mut().set_mode(PACING_MODES[index]));
        column = column.add(Box::new(picker_row(
            PICKER_LABELS[1],
            fonts,
            Box::new(picker),
        )));
    }

    // Which hand(s) training exercises target (hidden in repertoire).
    {
        let visible = training_cell(engine);
        let selected = engine_index_cell(engine, |e| segment_index(&HandMode::ALL, e.hand_mode()));
        let click = Rc::clone(engine);
        let labels: Vec<&str> = HandMode::ALL.iter().map(|h| h.raw_value()).collect();
        let picker = SegmentedControl::new(labels, selected, Arc::clone(&fonts.regular))
            .with_font_size(size::CALLOUT)
            .with_fit_width(true)
            .on_change(move |index| click.borrow_mut().set_hand_mode(HandMode::ALL[index]));
        let row = picker_row(
            PICKER_LABELS[2],
            fonts,
            Box::new(Tooltip::new(
                Box::new(picker),
                help::HANDS,
                Arc::clone(&fonts.regular),
            )),
        );
        column = column.add(Box::new(Conditional::new(visible, Box::new(row))));
    }

    // Octave offset readout + tempo-mode latency calibration.
    {
        let octave = Rc::clone(engine);
        let octave_label = DynamicLabel::new(
            move || {
                let offset = octave.borrow().octave_offset();
                if offset != 0 {
                    format!("Octave {}{offset}", if offset > 0 { "+" } else { "" })
                } else {
                    String::new()
                }
            },
            Arc::clone(&fonts.mono),
        )
        .with_font_size(size::CALLOUT)
        .with_color(palette::BLUE);

        let tempo = tempo_cell(engine);
        let show_calibration = Rc::clone(&cells.show_calibration);
        // `.controlSize(.small)` + help.
        let calibrate = Tooltip::new(
            Box::new(
                Button::new("Calibrate…", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_compact()
                    .on_click(move || open_cell(&show_calibration)),
            ),
            help::CALIBRATE,
            Arc::clone(&fonts.regular),
        );

        let row = FlexRow::new()
            .with_gap(8.0)
            .add(Box::new(octave_label))
            .add_flex(Box::new(crate::ui::hspacer()), 1.0)
            .add(Box::new(Conditional::new(tempo, Box::new(calibrate))));
        column = column.add(Box::new(row));
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ui::side_panel::PANEL_WIDTH;

    #[test]
    fn input_segments_follow_all_cases_order_and_round_trip() {
        let labels: Vec<&str> = INPUT_SOURCES.iter().map(|s| s.label()).collect();
        assert_eq!(labels, ["MIDI", "Keys", "Mic", "Unplugged"]);
        for (i, source) in INPUT_SOURCES.iter().enumerate() {
            assert_eq!(segment_index(&INPUT_SOURCES, *source), i);
            assert_eq!(INPUT_SOURCES[segment_index(&INPUT_SOURCES, *source)], *source);
        }
    }

    #[test]
    fn pacing_and_hands_segments_round_trip() {
        let labels: Vec<&str> = PACING_MODES.iter().map(|m| m.label()).collect();
        assert_eq!(labels, ["Self-paced", "Tempo"]);
        assert_eq!(segment_index(&PACING_MODES, PacingMode::Tempo), 1);
        assert_eq!(PACING_MODES[segment_index(&PACING_MODES, PacingMode::SelfPaced)], PacingMode::SelfPaced);

        let labels: Vec<&str> = HandMode::ALL.iter().map(|h| h.raw_value()).collect();
        assert_eq!(labels, ["Right", "Left", "Both", "Auto"]);
        for (i, hand) in HandMode::ALL.iter().enumerate() {
            assert_eq!(segment_index(&HandMode::ALL, *hand), i);
        }
    }

    /// Minimum breathing room a segment must keep around its label, in
    /// logical pixels — AppKit pads a segmented control's segments well
    /// past this, so anything tighter means the track was squeezed.
    const MIN_SEGMENT_PADDING: f64 = 4.0;

    /// The per-segment widths of a laid-out control, recovered from its
    /// label children: `SegmentedControl::layout` butts the segments
    /// together from x = 0 and centers each label in its own segment, so
    /// a label's center is its segment's center.
    #[cfg(test)]
    fn segment_widths(picker: &SegmentedControl) -> Vec<f64> {
        use agg_gui::widget::Widget;

        let mut x = 0.0;
        picker
            .children()
            .iter()
            .map(|child| {
                let center = child.bounds().x + child.bounds().width / 2.0;
                let width = 2.0 * (center - x);
                x += width;
                width
            })
            .collect()
    }

    /// The widest picker (Input, four segments) must fit the room its row
    /// leaves — the 300-pt panel less its 14-pt padding, the "Input"
    /// label, and the label gap — with every segment still padded around
    /// its label rather than squeezed onto it.
    #[test]
    fn input_picker_labels_fit_the_panel_width() {
        use agg_gui::geometry::Size;
        use agg_gui::widget::Widget;
        use std::cell::Cell;

        let fonts = UiFonts::bundled();
        let labels: Vec<&str> = INPUT_SOURCES.iter().map(|s| s.label()).collect();
        let mut picker =
            SegmentedControl::new(labels, Rc::new(Cell::new(0)), Arc::clone(&fonts.regular))
                .with_font_size(size::CALLOUT)
                .with_fit_width(true);
        let inner = PANEL_WIDTH - 2.0 * 14.0 - label_width(&fonts, PICKER_LABELS[0]) - LABEL_GAP;
        let size = picker.layout(Size::new(inner, 0.0));
        assert!(
            size.width <= inner + 0.5,
            "the track must fit its row: {} of {inner}",
            size.width
        );
        let widths = segment_widths(&picker);
        assert_eq!(widths.len(), INPUT_SOURCES.len());
        for (segment, label) in widths.iter().zip(picker.children_mut().iter_mut()) {
            let natural = label.layout(Size::new(inner, size.height)).width;
            assert!(
                natural + MIN_SEGMENT_PADDING <= segment + 0.5,
                "a segment holding a {natural}-px label needs at least                  {MIN_SEGMENT_PADDING} px of padding, but is only {segment} px wide"
            );
        }
    }

    /// Measured width of a picker's leading label at body size.
    #[cfg(test)]
    fn label_width(fonts: &UiFonts, text: &str) -> f64 {
        use agg_gui::geometry::Size;
        use agg_gui::widget::Widget;

        let mut label = Label::new(text, Arc::clone(&fonts.regular)).with_font_size(size::BODY);
        label.layout(Size::new(PANEL_WIDTH, 0.0)).width
    }

    /// The setup block laid out at the panel's inner width, with the
    /// Hands row visible (training, no active piece): the flat
    /// screen-space widget snapshot.
    #[cfg(test)]
    fn setup_section_nodes() -> (UiFonts, Vec<agg_gui::widget::InspectorNode>) {
        use agg_gui::geometry::{Point, Rect, Size};
        use agg_gui::widget::{collect_inspector_nodes, InspectorNode, Widget};
        use crate::ui::side_panel::{refresh_visibility_cells, test_engine, SidePanelCells};
        use std::cell::RefCell;

        let engine: Engine = Rc::new(RefCell::new(test_engine()));
        engine.borrow_mut().resume_training();
        refresh_visibility_cells(&engine.borrow());

        let fonts = UiFonts::bundled();
        let cells = SidePanelCells::new();
        let mut column = setup_section(&engine, &fonts, &cells);
        let inner = Size::new(PANEL_WIDTH - 2.0 * 14.0, 400.0);
        let size = column.layout(inner);
        column.set_bounds(Rect::new(0.0, 0.0, inner.width, size.height));
        column.layout(inner);

        let mut nodes: Vec<InspectorNode> = Vec::new();
        collect_inspector_nodes(&column, 0, Point::new(0.0, 0.0), &mut nodes);
        (fonts, nodes)
    }

    /// The three segmented tracks, in panel order (Input, Pacing, Hands).
    #[cfg(test)]
    fn picker_tracks(nodes: &[agg_gui::widget::InspectorNode]) -> Vec<agg_gui::geometry::Rect> {
        let tracks: Vec<agg_gui::geometry::Rect> = nodes
            .iter()
            .filter(|n| n.type_name == "SegmentedControl")
            .map(|n| n.screen_bounds)
            .collect();
        assert_eq!(tracks.len(), 3, "Input, Pacing and Hands");
        tracks
    }

    /// Every segmented picker keeps the leading label macOS draws for
    /// `Picker("Input", …)` — the label sits to the left of its control,
    /// at the width its text measures.
    #[test]
    fn every_picker_row_carries_its_label() {
        let (fonts, nodes) = setup_section_nodes();
        let controls = picker_tracks(&nodes);

        for (control, text) in controls.iter().zip(PICKER_LABELS.iter()) {
            let expected = label_width(&fonts, text);
            let center_y = control.y + control.height / 2.0;
            let found = nodes.iter().any(|n| {
                n.type_name == "Label"
                    && (n.screen_bounds.width - expected).abs() < 0.5
                    && n.screen_bounds.x + n.screen_bounds.width <= control.x + 0.5
                    && n.screen_bounds.y <= center_y
                    && n.screen_bounds.y + n.screen_bounds.height >= center_y
            });
            assert!(
                found,
                "the {text} picker must carry a {expected}-px \"{text}\" label \
                 to the left of its control at {control:?}"
            );
        }
    }

    /// AppKit sizes a segmented control to its segments, so only the
    /// four-segment Input track reaches the panel edge — the two- and
    /// four-segment Pacing and Hands tracks stop where their (shorter)
    /// labels end (`reference/swift/window/training-default.png`).
    /// Stretching every track to the row width lines all three right
    /// edges up at the panel edge, which is the bug this guards.
    #[test]
    fn picker_tracks_hug_their_segments() {
        let (_, nodes) = setup_section_nodes();
        let controls = picker_tracks(&nodes);
        let panel_right = PANEL_WIDTH - 2.0 * 14.0;

        for (control, text) in controls.iter().zip(PICKER_LABELS.iter()) {
            assert!(
                control.x + control.width <= panel_right + 0.5,
                "the {text} track {control:?} must fit inside the {panel_right}-px panel"
            );
        }
        // Input is the widest: four segments, one of them "Unplugged".
        let input_right = controls[0].x + controls[0].width;
        assert!(
            input_right >= panel_right - 2.0,
            "the Input track must reach the panel edge, ends at {input_right} of {panel_right}"
        );
        for (control, text) in controls[1..].iter().zip(PICKER_LABELS[1..].iter()) {
            let right = control.x + control.width;
            assert!(
                right < panel_right - 8.0,
                "the {text} track must stop short of the panel edge, \
                 ends at {right} of {panel_right} — it is stretching, not hugging"
            );
        }
    }

    #[test]
    fn pickers_track_the_engine_each_frame() {
        use crate::ui::side_panel::{refresh_visibility_cells, test_engine};
        use std::cell::RefCell;

        let engine: Engine = Rc::new(RefCell::new(test_engine()));
        let selected = engine_index_cell(&engine, |e| segment_index(&INPUT_SOURCES, e.input_source()));
        engine.borrow_mut().set_input_source(InputSource::Microphone);
        refresh_visibility_cells(&engine.borrow());
        assert_eq!(selected.get(), 2);
        engine.borrow_mut().set_input_source(InputSource::SelfVerify);
        refresh_visibility_cells(&engine.borrow());
        assert_eq!(selected.get(), 3);
    }
}
