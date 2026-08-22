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
        column = column.add(Box::new(
            InfoRows::new(fonts, move || status_rows(&state.borrow())).with_centered(true),
        ));
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

/// The status readout: the Swift `if let result … else if running …`
/// block as pure rows, so the tests can assert their text and faces.
fn status_rows(state: &CalibState) -> Vec<InfoRow> {
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
            // Swift: plain `.headline` — this row's number barely moves.
            vec![InfoRow::text(
                format!("Warm-up: {} taps left", state.warmups_left),
                size::BODY,
            )
            .with_style(RowStyle::Bold)]
        } else {
            // Swift: `.font(.headline).monospacedDigit()` — the count
            // ticks down every tap, so its digits must not reflow.
            vec![InfoRow::text(
                format!(
                    "Measuring: {} taps left",
                    MEASURED_TAPS - state.offsets.len()
                ),
                size::BODY,
            )
            .with_style(RowStyle::BoldTabularDigits)]
        }
    } else {
        Vec::new()
    }
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

    /// `CalibrationSheet.swift:40` marks the measuring row
    /// `.font(.headline).monospacedDigit()`: headline weight with
    /// tabular figures, so the counting-down number never reflows the
    /// line. The warm-up and result rows are plain `.headline`.
    #[test]
    fn measuring_row_uses_headline_tabular_digits() {
        let mut state = CalibState::new();
        state.running = true;
        assert!(state.warmups_left > 0);
        let rows = status_rows(&state);
        assert_eq!(rows[0].text, "Warm-up: 4 taps left");
        assert_eq!(rows[0].style, RowStyle::Bold, "warm-up is plain headline");

        state.warmups_left = 0;
        state.offsets.push(30.0);
        let rows = status_rows(&state);
        assert_eq!(rows[0].text, "Measuring: 11 taps left");
        assert_eq!(
            rows[0].style,
            RowStyle::BoldTabularDigits,
            "the measuring count is .headline.monospacedDigit()"
        );

        state.result = Some(30.0);
        let rows = status_rows(&state);
        assert_eq!(rows[0].style, RowStyle::Bold, "the result is plain headline");
    }

}

/// Layout regression tests: the intro copy, the status readout and the
/// Start/Cancel buttons all fit the 440×260 panel (the Library
/// search-box failure class, where a chrome row swells and shoves the
/// content off the sheet).
#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::ui::sheets::layout_test_support::{
        contains, describe, laid_out_nodes, node_with_property, node_with_property_prefix,
        nodes_outside_panel, panel_rect, shared_test_engine, test_clock, WINDOW,
    };
    use crate::ui::side_panel::{open_cell, SidePanelCells};
    use agg_gui::widget::InspectorNode;

    /// Build the sheet and open it exactly as the setup panel's
    /// Calibrate… button does.
    fn opened_sheet_nodes() -> Vec<InspectorNode> {
        let engine = shared_test_engine();
        let fonts = UiFonts::bundled();
        let cells = SidePanelCells::new();
        let clock = test_clock();
        let mut sheet = build_calibration_sheet(&engine, &fonts, &clock, &cells);
        open_cell(&cells.show_calibration);
        laid_out_nodes(&mut sheet)
    }

    /// The Swift `.frame(width: 440)` panel, centered in the window.
    #[test]
    fn panel_is_the_swift_fixed_frame() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);
        assert_eq!(panel.width, SHEET_SIZE.width);
        assert_eq!(panel.height, SHEET_SIZE.height);
        assert!(
            contains(Rect::new(0.0, 0.0, WINDOW.width, WINDOW.height), panel),
            "panel {panel:?} must sit inside the window"
        );
    }

    /// The idle sheet shows Start over Cancel, both clickable on the
    /// panel — the sheet is useless if either falls off.
    #[test]
    fn start_and_cancel_sit_inside_the_panel() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);
        let start = node_with_property(&nodes, "label", "Start").screen_bounds;
        let cancel = node_with_property(&nodes, "label", "Cancel").screen_bounds;
        for (name, rect) in [("Start", start), ("Cancel", cancel)] {
            assert!(
                rect.width > 0.0 && rect.height > 0.0,
                "{name} must have a non-zero size, got {rect:?}"
            );
            assert!(
                contains(panel, rect),
                "{name} at {rect:?} must sit inside the panel {panel:?}"
            );
        }
        // Y-up: Start is the higher of the two.
        assert!(
            start.y > cancel.y,
            "Start {start:?} sits above Cancel {cancel:?}"
        );
    }

    /// The title and the wrapped instructions stay on the panel above
    /// the buttons.
    #[test]
    fn title_and_instructions_stay_above_the_buttons() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);
        let title = node_with_property(&nodes, "text", "Latency Calibration").screen_bounds;
        let copy = node_with_property_prefix(&nodes, "text", "Tap any piano key").screen_bounds;
        let start = node_with_property(&nodes, "label", "Start").screen_bounds;
        for (name, rect) in [("the title", title), ("the instructions", copy)] {
            assert!(
                rect.width > 0.0 && rect.height > 0.0 && contains(panel, rect),
                "{name} at {rect:?} must sit inside the panel {panel:?}"
            );
            assert!(
                rect.y >= start.y + start.height - 0.5,
                "{name} at {rect:?} belongs above Start {start:?}"
            );
        }
    }

    /// Nothing may leave the panel (this sheet has no scrolling area).
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

/// Key-routing regression tests: this sheet is the app's only
/// `with_key_passthrough` modal, so piano keys keep reaching the engine
/// while it measures — but Return must NOT fire the side panel's default
/// action (Next Exercise / Replay / Run It Back) behind the sheet.
#[cfg(test)]
mod key_routing_tests {
    use crate::engine::{InputSource, Phase};
    use crate::ui::{build_keyinsight_app, KeyInSightPlatform, UiFonts};
    use agg_gui::event::{Key, Modifiers};
    use agg_gui::geometry::{Point, Size};
    use agg_gui::widget::{active_modal_path, collect_inspector_nodes};
    use agg_gui::App;

    struct NoopPlatform;
    impl KeyInSightPlatform for NoopPlatform {}

    const WINDOW: Size = Size {
        width: 1180.0,
        height: 640.0,
    };

    /// Lay out a few frames: layout ticks the engine and re-syncs the
    /// side panel's visibility cells, exactly as the shell's frame loop
    /// does.
    fn frames(app: &mut App) {
        for _ in 0..3 {
            app.layout(WINDOW);
        }
    }

    fn sheet_is_up(app: &App) -> bool {
        active_modal_path(app.root()).is_some()
    }

    /// Is a button with this label laid out anywhere in the app?
    fn has_button(app: &App, label: &str) -> bool {
        let mut nodes = Vec::new();
        collect_inspector_nodes(app.root(), 0, Point::new(0.0, 0.0), &mut nodes);
        nodes.iter().any(|node| {
            node.properties
                .iter()
                .any(|(key, value)| *key == "label" && value == label)
        })
    }

    fn press(app: &mut App, key: Key) {
        app.on_key_down(key, Modifiers::default());
    }

    /// Return while the sheet is MEASURING — its Start button gone and
    /// Done not yet shown, so the sheet itself has no default action —
    /// must leave the session alone. Escape (the sheet's Cancel) then
    /// closes it, and the next Return does reach the side panel again:
    /// the positive control that proves the default action is wired.
    #[test]
    fn enter_while_measuring_does_not_reach_the_panel_default_action() {
        let (mut app, handles) = build_keyinsight_app(UiFonts::bundled(), NoopPlatform);
        frames(&mut app);
        // Park the session on the summary, where "Next Exercise" is the
        // side panel's default action. Unplugged (self-verify) input is
        // the one source that can be graded headlessly.
        handles
            .engine
            .borrow_mut()
            .set_input_source(InputSource::SelfVerify);
        frames(&mut app);
        handles.engine.borrow_mut().self_verify_grade(true);
        frames(&mut app);
        assert!(
            matches!(handles.engine.borrow().phase(), Phase::Summary(_)),
            "the summary is the state whose default action we guard"
        );

        handles.open_calibration();
        frames(&mut app);
        assert!(sheet_is_up(&app), "the Calibration sheet is showing");

        // Return #1 hits the sheet's own default action (Start).
        press(&mut app, Key::Enter);
        frames(&mut app);
        assert!(
            !has_button(&app, "Start"),
            "Return started the calibration run"
        );
        assert!(
            !has_button(&app, "Done"),
            "mid-run the sheet has no default action at all"
        );

        // Return #2 has nowhere to go inside the sheet — and must not
        // leak to the side panel behind it.
        press(&mut app, Key::Enter);
        frames(&mut app);
        assert!(
            matches!(handles.engine.borrow().phase(), Phase::Summary(_)),
            "Return under the Calibration sheet must not advance the session"
        );
        assert!(sheet_is_up(&app), "and the sheet stays open");

        // Escape is the sheet's Cancel: it stops, closes, and resumes
        // the training loop.
        press(&mut app, Key::Escape);
        frames(&mut app);
        assert!(!sheet_is_up(&app), "Escape cancels the sheet");
        assert_eq!(*handles.engine.borrow().phase(), Phase::Playing);

        // Positive control: with no sheet showing, Return fires the side
        // panel's default action again ("Nailed It" while playing).
        press(&mut app, Key::Enter);
        frames(&mut app);
        assert!(
            matches!(handles.engine.borrow().phase(), Phase::Summary(_)),
            "the panel default action fires once the sheet is gone"
        );
    }
}
