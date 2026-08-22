//! The setup block — `// MARK: - Setup` in `UI/SidePanel.swift`: input
//! source, mic level, pacing, hands, octave readout + Calibrate….
//!
//! The three `Picker(…).pickerStyle(.segmented)`s are
//! [`SegmentedControl`]s (callout type: Inter runs wider than SF, and
//! "Unplugged" has to clear its quarter of the 272-pt row) bound to an
//! index cell the root refreshes from
//! the engine every frame (`engine_index_cell`), so external changes —
//! the persisted settings on start, a mode the engine declines — show up
//! without a rebuild; the user's pick maps back through the same tables.

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::layout_props::HAnchor;
use agg_gui::widgets::{Button, Conditional, FlexColumn, FlexRow, Label, SegmentedControl, Tooltip};

use crate::engine::{HandMode, InputSource, PacingMode};
use crate::ui::fonts::{size, UiFonts};
use crate::ui::help;
use crate::ui::{palette, DynamicLabel, LevelMeter};

use super::cells::{engine_index_cell, mic_cell, open_cell, tempo_cell, training_cell};
use super::{Engine, SidePanelCells};

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
        column = column.add(Box::new(
            SegmentedControl::new(labels, selected, Arc::clone(&fonts.regular))
                .with_font_size(size::CALLOUT)
                .with_h_anchor(HAnchor::STRETCH)
                .on_change(move |index| click.borrow_mut().set_input_source(INPUT_SOURCES[index])),
        ));
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
        column = column.add(Box::new(
            SegmentedControl::new(labels, selected, Arc::clone(&fonts.regular))
                .with_font_size(size::CALLOUT)
                .with_h_anchor(HAnchor::STRETCH)
                .with_enabled_fn(move || {
                    let engine = enabled.borrow();
                    engine.input_source().supports_timing() && engine.content_supports_tempo()
                })
                .on_change(move |index| click.borrow_mut().set_mode(PACING_MODES[index])),
        ));
    }

    // Which hand(s) training exercises target (hidden in repertoire).
    {
        let visible = training_cell(engine);
        let selected = engine_index_cell(engine, |e| segment_index(&HandMode::ALL, e.hand_mode()));
        let click = Rc::clone(engine);
        let labels: Vec<&str> = HandMode::ALL.iter().map(|h| h.raw_value()).collect();
        let picker = SegmentedControl::new(labels, selected, Arc::clone(&fonts.regular))
            .with_font_size(size::CALLOUT)
            .with_h_anchor(HAnchor::STRETCH)
            .on_change(move |index| click.borrow_mut().set_hand_mode(HandMode::ALL[index]));
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(Tooltip::new(
                Box::new(picker),
                help::HANDS,
                Arc::clone(&fonts.regular),
            )),
        )));
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

    /// The widest picker (Input, four segments) must not clip its labels
    /// inside the 300-pt panel less its 14-pt padding.
    #[test]
    fn input_picker_labels_fit_the_panel_width() {
        use crate::ui::side_panel::PANEL_WIDTH;
        use agg_gui::geometry::Size;
        use agg_gui::widget::Widget;
        use std::cell::Cell;

        let fonts = UiFonts::bundled();
        let labels: Vec<&str> = INPUT_SOURCES.iter().map(|s| s.label()).collect();
        let mut picker =
            SegmentedControl::new(labels, Rc::new(Cell::new(0)), Arc::clone(&fonts.regular))
                .with_font_size(size::CALLOUT)
                .with_h_anchor(HAnchor::STRETCH);
        let inner = PANEL_WIDTH - 2.0 * 14.0;
        let size = picker.layout(Size::new(inner, 0.0));
        assert_eq!(size.width, inner);
        let segment = (inner / 4.0).floor();
        for label in picker.children_mut() {
            let natural = label.layout(Size::new(inner, size.height)).width;
            assert!(
                natural + 4.0 <= segment,
                "label {natural} px wide in a {segment} px segment"
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
