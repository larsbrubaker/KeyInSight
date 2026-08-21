//! The setup block — `// MARK: - Setup` in `UI/SidePanel.swift`: input
//! source, mic level, pacing, hands, octave readout + Calibrate….

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widgets::{Button, Conditional, FlexColumn, FlexRow, Label};

use crate::engine::{HandMode, InputSource, PacingMode};
use crate::ui::fonts::{size, UiFonts};
use crate::ui::{palette, DynamicLabel, LevelMeter};

use super::cells::{mic_cell, open_cell, tempo_cell, training_cell};
use super::{Engine, SidePanelCells};

pub(super) fn setup_section(engine: &Engine, fonts: &UiFonts, cells: &SidePanelCells) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(8.0);

    // Input source picker (segmented row, equal widths).
    let mut input_row = FlexRow::new().with_gap(6.0);
    for source in [
        InputSource::Midi,
        InputSource::Keyboard,
        InputSource::Microphone,
        InputSource::SelfVerify,
    ] {
        let active = Rc::clone(engine);
        let click = Rc::clone(engine);
        input_row = input_row.add_flex(
            Box::new(
                Button::new(source.label(), Arc::clone(&fonts.regular))
                    .with_subtle()
                    .with_compact()
                    .with_font_size(size::CALLOUT)
                    .with_label_pad_h(2.0)
                    .with_active_fn(move || active.borrow().input_source() == source)
                    .on_click(move || click.borrow_mut().set_input_source(source)),
            ),
            1.0,
        );
    }
    column = column.add(Box::new(input_row));

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
                Box::new(LevelMeter::new(move || level.borrow().mic_level())),
                1.0,
            );
        column = column.add(Box::new(Conditional::new(visible, Box::new(row))));
    }

    // Pacing picker; disabled unless the source has exact timing and the
    // content is monophonic (the Swift `.disabled(...)`).
    let mut pacing_row = FlexRow::new().with_gap(6.0);
    for mode in [PacingMode::SelfPaced, PacingMode::Tempo] {
        let active = Rc::clone(engine);
        let click = Rc::clone(engine);
        let enabled = Rc::clone(engine);
        pacing_row = pacing_row.add_flex(
            Box::new(
                Button::new(mode.label(), Arc::clone(&fonts.regular))
                    .with_subtle()
                    .with_compact()
                    .with_font_size(size::CALLOUT)
                    .with_enabled_fn(move || {
                        let engine = enabled.borrow();
                        engine.input_source().supports_timing() && engine.content_supports_tempo()
                    })
                    .with_active_fn(move || active.borrow().mode() == mode)
                    .on_click(move || click.borrow_mut().set_mode(mode)),
            ),
            1.0,
        );
    }
    column = column.add(Box::new(pacing_row));

    // Which hand(s) training exercises target (hidden in repertoire).
    // Help: "Right = treble clef, Left = bass clef, Both = hands together.
    // Auto rotates toward your weaker hand and mixes in two-hand exercises
    // once the bass range is learned."
    {
        let visible = training_cell(engine);
        let mut hands_row = FlexRow::new().with_gap(6.0);
        for hand in HandMode::ALL {
            let active = Rc::clone(engine);
            let click = Rc::clone(engine);
            hands_row = hands_row.add_flex(
                Box::new(
                    Button::new(hand.raw_value(), Arc::clone(&fonts.regular))
                        .with_subtle()
                        .with_compact()
                        .with_font_size(size::CALLOUT)
                        .with_label_pad_h(2.0)
                        .with_active_fn(move || active.borrow().hand_mode() == hand)
                        .on_click(move || click.borrow_mut().set_hand_mode(hand)),
                ),
                1.0,
            );
        }
        column = column.add(Box::new(Conditional::new(visible, Box::new(hands_row))));
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
        let calibrate = Button::new("Calibrate…", Arc::clone(&fonts.regular))
            .with_subtle().with_active_fn(|| false)
            .with_compact()
            .on_click(move || open_cell(&show_calibration));

        let row = FlexRow::new()
            .with_gap(8.0)
            .add(Box::new(octave_label))
            .add_flex(Box::new(crate::ui::hspacer()), 1.0)
            .add(Box::new(Conditional::new(tempo, Box::new(calibrate))));
        column = column.add(Box::new(row));
    }
    column
}
