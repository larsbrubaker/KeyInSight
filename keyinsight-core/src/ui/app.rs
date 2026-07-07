//! The application root: builds the full training UI (ports
//! `UI/TrainingView.swift` + `KeyInSightApp.swift`) and owns the
//! per-frame engine tick + computer-keyboard routing.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::draw_ctx::DrawCtx;
use agg_gui::event::{Event, EventResult, Key, Modifiers};
use agg_gui::geometry::{Rect, Size};
use agg_gui::text::Font;
use agg_gui::widget::Widget;
use agg_gui::widgets::{FlexColumn, FlexRow, Stack};
use agg_gui::App;

use crate::audio::{AudioOut, NullAudioOut};
use crate::engine::{default_backend_factory, SessionEngine};
use crate::notation::NotationWidget;
use crate::persistence::{AppDatabase, Storage};
use crate::ui::side_panel::{self, SidePanelCells};
use crate::ui::{bottom_bar, sheets, PianoStripWidget};

/// Platform capability surface. The native and WASM shells implement this
/// so the core can obtain platform services without `cfg`-gating
/// (`docs/platform-substitutions.md`). Grows as platform backends land
/// (MIDI port enumeration, mic capture).
pub trait KeyInSightPlatform: 'static {
    /// Persistent storage for the app database. `None` runs without
    /// persistence — the training loop still works (Swift behaved the
    /// same when SQLite failed to open).
    fn storage(&self) -> Option<Box<dyn Storage>> {
        None
    }

    /// Audio output (metronome clicks + reference playback). Default is
    /// silent.
    fn audio(&self) -> Rc<dyn AudioOut> {
        Rc::new(NullAudioOut)
    }
}

/// Handles the platform shells keep: tick the engine every frame.
pub struct KeyInSightHandles {
    pub engine: Rc<RefCell<SessionEngine>>,
}

impl KeyInSightHandles {
    /// Advance the engine (input queue, deferred actions, metronome sweep).
    /// Shells call this once per painted frame.
    pub fn tick(&self) {
        self.engine.borrow_mut().tick();
    }
}

/// The monotonic host clock every NoteEvent carries (`CACurrentMediaTime`
/// in Swift). `web_time` keeps it wasm-clean.
fn host_clock() -> Rc<dyn Fn() -> f64> {
    let start = web_time::Instant::now();
    Rc::new(move || start.elapsed().as_secs_f64())
}

/// Root widget: hosts the whole tree, ticks the engine every painted
/// frame, refreshes the Conditional visibility cells, and routes computer
/// keyboard input to the simulated backend (the Swift NSEvent monitor).
struct TrainingRoot {
    engine: Rc<RefCell<SessionEngine>>,
    children: Vec<Box<dyn Widget>>,
    bounds: Rect,
}

impl Widget for TrainingRoot {
    fn type_name(&self) -> &'static str {
        "TrainingRoot"
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

    fn layout(&mut self, available: Size) -> Size {
        {
            let mut engine = self.engine.borrow_mut();
            engine.tick();
            side_panel::refresh_visibility_cells(&engine);
        }
        let child = &mut self.children[0];
        let size = child.layout(available);
        child.set_bounds(Rect::new(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&mut self, _ctx: &mut dyn DrawCtx) {}

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::KeyDown {
                key: Key::Char(ch),
                modifiers,
            } if !modifiers.ctrl && !modifiers.alt && !modifiers.meta => {
                if self.engine.borrow_mut().handle_simulated_key(*ch, true, false) {
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            Event::KeyUp {
                key: Key::Char(ch),
                ..
            } => {
                if self.engine.borrow_mut().handle_simulated_key(*ch, false, false) {
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    /// Piano keys arrive here when no text field has focus — the port of
    /// the Swift backend's `isTextInputActive` guard (focused text inputs
    /// consume keys first, so mapped keys type letters there).
    fn on_unconsumed_key(&mut self, key: &Key, modifiers: Modifiers) -> EventResult {
        if let Key::Char(ch) = key {
            if !modifiers.ctrl
                && !modifiers.alt
                && !modifiers.meta
                && self.engine.borrow_mut().handle_simulated_key(*ch, true, false)
            {
                return EventResult::Consumed;
            }
        }
        EventResult::Ignored
    }

    /// Focusable so KeyUp events (note-offs) route here by default; a
    /// future text field stealing focus pauses piano input, matching the
    /// Swift `isTextInputActive` behavior.
    fn is_focusable(&self) -> bool {
        true
    }

    fn focus_id(&self) -> Option<agg_gui::focus::FocusId> {
        Some(TRAINING_ROOT_FOCUS_ID)
    }
}

/// Focus channel id for the training root (piano key routing).
pub const TRAINING_ROOT_FOCUS_ID: agg_gui::focus::FocusId = 0x4B49_5349; // "KISI"

/// Build the shared KeyInSight application. Both shells call this and
/// forward platform input into the returned [`App`].
pub fn build_keyinsight_app<P: KeyInSightPlatform>(
    font: Arc<Font>,
    platform: P,
) -> (App, KeyInSightHandles) {
    // KeyInSight is a light-themed app on every platform — sheet music is
    // black ink on a light page, and the chrome follows (see CLAUDE.md).
    agg_gui::set_visuals(agg_gui::Visuals::light());

    let clock = host_clock();
    let now_ms = ((clock)() * 1000.0) as i64;
    let db = platform.storage().map(|storage| AppDatabase::open(storage, now_ms));
    let audio = platform.audio();

    let engine = Rc::new(RefCell::new(SessionEngine::new(
        db,
        audio,
        Rc::clone(&clock),
        default_backend_factory(),
        // Session seed: wall-clock-derived so exercises vary run to run,
        // while any single run stays reproducible from its log.
        now_ms as u64,
    )));

    // Vocabulary hover: notation → engine.
    {
        let engine_for_inspect = Rc::clone(&engine);
        let notation = engine.borrow().notation.clone();
        notation.borrow_mut().on_inspect = Some(Box::new(move |kind, id| {
            if let Ok(mut engine) = engine_for_inspect.try_borrow_mut() {
                engine.inspect(kind, id);
            }
        }));
    }

    let cells = SidePanelCells {
        show_library: Rc::new(std::cell::Cell::new(false)),
        show_progress: Rc::new(std::cell::Cell::new(false)),
    };

    // Center: notation above the beginner keys strip.
    let notation_widget = {
        let controller = engine.borrow().notation.clone();
        NotationWidget::new(controller, Rc::clone(&clock))
    };
    let center = FlexColumn::new()
        .with_gap(0.0)
        .add_flex(Box::new(notation_widget), 1.0)
        .add(Box::new(PianoStripWidget::new(
            Rc::clone(&engine),
            Arc::clone(&font),
        )));

    // Main row: notation + side panel.
    let main_row = FlexRow::new()
        .with_gap(0.0)
        .add_flex(Box::new(center), 1.0)
        .add(side_panel::build_side_panel(&engine, &font, &cells));

    // Full window: main + bottom bar, with the sheets overlaid.
    let training = FlexColumn::new()
        .with_gap(0.0)
        .add_flex(Box::new(main_row), 1.0)
        .add(bottom_bar::build_bottom_bar(&engine, &font, &cells));

    let stack = Stack::new()
        .add(Box::new(training))
        .add(sheets::build_sheet_overlay(&engine, &font, &cells));

    let root = TrainingRoot {
        engine: Rc::clone(&engine),
        children: vec![Box::new(stack)],
        bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
    };

    engine.borrow_mut().start();
    // Give the root keyboard focus so piano keys sound immediately.
    agg_gui::focus::request_focus(TRAINING_ROOT_FOCUS_ID);

    (
        App::new(Box::new(root)),
        KeyInSightHandles { engine },
    )
}
