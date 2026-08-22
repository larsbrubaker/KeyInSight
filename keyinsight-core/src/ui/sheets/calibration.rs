//! The latency Calibration sheet — `UI/CalibrationSheet.swift` as a
//! 440-wide modal: tap along with the metronome; the median offset
//! between taps and beats becomes the input-latency compensation applied
//! to tempo-mode scoring.
//!
//! The Swift sheet's tap handler ran inside the engine's input path; here
//! `engine.calibration_tap` only queues timestamps (the engine is
//! mid-borrow when it fires) and [`CalibrationDriver`] drains the queue
//! once per frame, outside the engine tick.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::draw_ctx::DrawCtx;
use agg_gui::event::{Event, EventResult};
use agg_gui::geometry::{Rect, Size};
use agg_gui::layout_props::HAnchor;
use agg_gui::widget::Widget;
use agg_gui::widgets::{Button, Conditional, FlexColumn, Label, ModalSheet};

use crate::ui::fonts::{size, UiFonts};
use crate::ui::side_panel::{watch_cell, SidePanelCells};
use crate::ui::{median, InfoRow, InfoRows, RowStyle};

use super::{Clock, Engine};

/// The Swift constants.
const BPM: f64 = 90.0;
const WARMUP_TAPS: usize = 4;
const MEASURED_TAPS: usize = 12;
/// `.frame(width: 440)` (+ height to fit the copy).
const SHEET_SIZE: Size = Size {
    width: 440.0,
    height: 260.0,
};

/// Shared calibration state (the Swift `@State` block).
struct CalibState {
    running: bool,
    warmups_left: usize,
    offsets: Vec<f64>,
    result: Option<f64>,
    /// Tap timestamps queued by `engine.calibration_tap`, drained per
    /// frame by the driver.
    taps: Vec<f64>,
}

impl CalibState {
    fn new() -> Self {
        Self {
            running: false,
            warmups_left: WARMUP_TAPS,
            offsets: Vec::new(),
            result: None,
            taps: Vec::new(),
        }
    }
}

type State = Rc<RefCell<CalibState>>;

pub fn build_calibration_sheet(
    engine: &Engine,
    fonts: &UiFonts,
    clock: &Clock,
    cells: &SidePanelCells,
) -> Box<dyn Widget> {
    let visible = Rc::clone(&cells.show_calibration);
    let state: State = Rc::new(RefCell::new(CalibState::new()));

    let mut column = FlexColumn::new().with_gap(14.0).with_padding(28.0);

    column = column.add(Box::new(
        Label::new("Latency Calibration", Arc::clone(&fonts.bold))
            .with_font_size(size::TITLE2)
            .with_h_anchor(HAnchor::CENTER),
    ));
    column = column.add(Box::new(
        Label::new(
            format!(
                "Tap any piano key on each click. The first {WARMUP_TAPS} taps warm up; the next {MEASURED_TAPS} are measured."
            ),
            Arc::clone(&fonts.regular),
        )
        .with_font_size(size::BODY)
        .with_dim(true)
        .with_wrap(true)
        .with_align(agg_gui::widgets::LabelAlign::Center),
    ));

    // Status readout (result / warm-up / measuring).
    {
        let state = Rc::clone(&state);
        column = column.add(Box::new(InfoRows::new(fonts, move || {
            let state = state.borrow();
            if let Some(result) = state.result {
                vec![
                    InfoRow::text(
                        format!("Measured input latency: {result:.0} ms"),
                        size::BODY,
                    )
                    .with_style(RowStyle::Bold),
                    InfoRow::text(
                        "Saved — tempo scoring now compensates for it.",
                        size::BODY,
                    )
                    .with_dim(),
                ]
            } else if state.running {
                if state.warmups_left > 0 {
                    vec![InfoRow::text(
                        format!("Warm-up: {} taps left", state.warmups_left),
                        size::BODY,
                    )
                    .with_style(RowStyle::Bold)]
                } else {
                    vec![InfoRow::text(
                        format!(
                            "Measuring: {} taps left",
                            MEASURED_TAPS - state.offsets.len()
                        ),
                        size::BODY,
                    )
                    .with_style(RowStyle::Bold)]
                }
            } else {
                Vec::new()
            }
        }).with_centered(true)));
    }

    // Start (idle) / Done (finished) / Cancel.
    {
        let idle = {
            let state = Rc::clone(&state);
            watch_cell(move || {
                let state = state.borrow();
                !state.running && state.result.is_none()
            })
        };
        let start_state = Rc::clone(&state);
        let start_engine = Rc::clone(engine);
        let start_clock = Rc::clone(clock);
        column = column.add(Box::new(
            Conditional::new(
                idle,
                // `.keyboardShortcut(.defaultAction)`: Return starts.
                Box::new(
                    Button::new("Start", Arc::clone(&fonts.regular))
                        .with_default_action()
                        .on_click(move || {
                            start(&start_engine, &start_state, &start_clock);
                        }),
                ),
            )
            .with_h_anchor(HAnchor::CENTER),
        ));
    }
    {
        let finished = {
            let state = Rc::clone(&state);
            watch_cell(move || state.borrow().result.is_some())
        };
        let done_visible = Rc::clone(&visible);
        let done_engine = Rc::clone(engine);
        column = column.add(Box::new(
            Conditional::new(
                finished,
                // `.keyboardShortcut(.defaultAction)`: Return dismisses.
                Box::new(
                    Button::new("Done", Arc::clone(&fonts.regular))
                        .with_default_action()
                        .on_click(move || {
                            done_visible.set(false);
                            // Restart the training loop with the new
                            // compensation applied (the Swift onDisappear).
                            done_engine.borrow_mut().next_exercise();
                            agg_gui::animation::request_draw();
                        }),
                ),
            )
            .with_h_anchor(HAnchor::CENTER),
        ));
    }
    {
        let unfinished = {
            let state = Rc::clone(&state);
            watch_cell(move || state.borrow().result.is_none())
        };
        let cancel_visible = Rc::clone(&visible);
        let cancel_engine = Rc::clone(engine);
        let cancel_state = Rc::clone(&state);
        column = column.add(Box::new(
            Conditional::new(
                unfinished,
                // `.keyboardShortcut(.cancelAction)`: Esc cancels (the
                // sheet's own Esc close covers the finished state).
                Box::new(
                    Button::new("Cancel", Arc::clone(&fonts.regular))
                        .with_subtle()
                        .with_active_fn(|| false)
                        .with_cancel_action()
                        .on_click(move || {
                            stop(&cancel_engine, &cancel_state);
                            cancel_visible.set(false);
                            cancel_engine.borrow_mut().next_exercise();
                            agg_gui::animation::request_draw();
                        }),
                ),
            )
            .with_h_anchor(HAnchor::CENTER),
        ));
    }

    // The per-frame tap drain rides along as an invisible child.
    let driver = CalibrationDriver {
        engine: Rc::clone(engine),
        state: Rc::clone(&state),
        bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
        children: Vec::new(),
    };
    column = column.add(Box::new(driver));

    // Esc while running = Cancel (the Swift onDisappear stop).
    let esc_engine = Rc::clone(engine);
    let esc_state = Rc::clone(&state);
    Box::new(
        ModalSheet::new(visible, Box::new(column))
            .with_panel_size(SHEET_SIZE)
            .with_key_passthrough(true)
            .with_on_close(move || {
                stop(&esc_engine, &esc_state);
                esc_engine.borrow_mut().next_exercise();
            }),
    )
}

fn start(engine: &Engine, state: &State, clock: &Clock) {
    {
        let mut state = state.borrow_mut();
        state.offsets.clear();
        state.taps.clear();
        state.warmups_left = WARMUP_TAPS;
        state.result = None;
        state.running = true;
    }
    let mut engine_mut = engine.borrow_mut();
    engine_mut.prepare_for_calibration();
    let now = (clock)();
    engine_mut
        .metronome
        .start(BPM, 4, now + 0.35, now);
    let queue = Rc::clone(state);
    engine_mut.calibration_tap = Some(Box::new(move |timestamp| {
        queue.borrow_mut().taps.push(timestamp);
        // Wake the frame loop: taps arrive while nothing else animates.
        agg_gui::animation::request_draw();
    }));
    agg_gui::animation::request_draw();
}

fn stop(engine: &Engine, state: &State) {
    let mut engine = engine.borrow_mut();
    engine.calibration_tap = None;
    engine.metronome.stop();
    let mut state = state.borrow_mut();
    state.running = false;
    state.taps.clear();
}

/// Invisible widget draining queued taps once per frame — the Swift
/// `handleTap`, run outside the engine's input borrow.
struct CalibrationDriver {
    engine: Engine,
    state: State,
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
}

impl CalibrationDriver {
    fn drain(&self) {
        let taps: Vec<f64> = {
            let mut state = self.state.borrow_mut();
            if !state.running {
                state.taps.clear();
                return;
            }
            std::mem::take(&mut state.taps)
        };
        if taps.is_empty() {
            return;
        }
        let beat_ms = 60_000.0 / BPM;
        for timestamp in taps {
            let ms = self
                .engine
                .borrow()
                .metronome
                .milliseconds_since_start(timestamp);
            if ms <= -beat_ms / 2.0 {
                continue;
            }
            // Offset from the nearest beat, in [-beat_ms/2, beat_ms/2).
            let mut offset = ms.rem_euclid(beat_ms);
            if offset >= beat_ms / 2.0 {
                offset -= beat_ms;
            }
            let mut state = self.state.borrow_mut();
            if state.warmups_left > 0 {
                state.warmups_left -= 1;
                continue;
            }
            state.offsets.push(offset);
            if state.offsets.len() >= MEASURED_TAPS {
                let measured = median(&state.offsets);
                state.result = Some(measured);
                state.running = false;
                drop(state);
                let mut engine = self.engine.borrow_mut();
                engine.set_input_latency(measured);
                engine.calibration_tap = None;
                engine.metronome.stop();
                return;
            }
        }
    }
}

impl Widget for CalibrationDriver {
    fn type_name(&self) -> &'static str {
        "CalibrationDriver"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }
    fn layout(&mut self, _available: Size) -> Size {
        Size::new(0.0, 0.0)
    }
    // Drain in paint, not layout: `App::paint` clears the draw-request
    // flag at its start, so only requests made DURING paint schedule the
    // next frame — the convention every agg-gui animation follows.
    fn paint(&mut self, _ctx: &mut dyn DrawCtx) {
        self.drain();
        // Taps keep arriving without other UI activity — keep frames
        // coming while a run is active.
        if self.state.borrow().running {
            agg_gui::animation::request_draw();
        }
    }
    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::default_backend_factory;
    use crate::persistence::AppDatabase;

    /// The full sheet flow, headless: Start installs the tap hook, piano
    /// keys queue taps, the driver's per-frame drain consumes warm-ups
    /// then measurements, and the median lands in `set_input_latency`.
    #[test]
    fn calibration_flow_measures_offsets_from_simulated_keys() {
        let time = Rc::new(RefCell::new(1000.0));
        let reader = Rc::clone(&time);
        let clock: Clock = Rc::new(move || *reader.borrow());
        let engine: Engine = Rc::new(RefCell::new(crate::engine::SessionEngine::new(
            Some(AppDatabase::in_memory(1_700_000_000_000)),
            Rc::new(crate::audio::NullAudioOut),
            Rc::clone(&clock),
            default_backend_factory(),
            42,
        )));
        engine.borrow_mut().start();

        let state: State = Rc::new(RefCell::new(CalibState::new()));
        start(&engine, &state, &clock);
        assert!(state.borrow().running);

        let driver = CalibrationDriver {
            engine: Rc::clone(&engine),
            state: Rc::clone(&state),
            bounds: agg_gui::geometry::Rect::new(0.0, 0.0, 0.0, 0.0),
            children: Vec::new(),
        };

        // Tap on every beat, 30 ms late (constant device latency); the
        // metronome started at now + 0.35.
        let beat = 60.0 / BPM;
        for i in 0..(WARMUP_TAPS + MEASURED_TAPS) {
            let tap_at = 1000.35 + i as f64 * beat + 0.030;
            *time.borrow_mut() = tap_at;
            assert!(engine.borrow_mut().handle_simulated_key('a', true, false));
            engine.borrow_mut().handle_simulated_key('a', false, false);
            driver.drain(); // the per-frame paint drain
        }

        let state = state.borrow();
        assert!(!state.running, "run completes after the measured taps");
        let measured = state.result.expect("median offset recorded");
        assert!(
            (measured - 30.0).abs() < 1.0,
            "median ≈ 30 ms, got {measured}"
        );
    }
}
