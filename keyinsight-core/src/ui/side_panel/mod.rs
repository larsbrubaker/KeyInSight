//! Right-hand panel: what you're doing, how you're doing, what to do, and
//! the controls for it — per activity and input source.
//!
//! Ports `UI/SidePanel.swift` at its exact geometry: 300pt wide, 14pt
//! padding and section spacing, dividers around the status block and the
//! setup section. SwiftUI's observed re-rendering maps to [`DynamicLabel`]
//! and [`InfoRows`] closures reading the engine each frame; `.sheet`
//! bindings map to shared visibility cells.
//!
//! One module per Swift `// MARK:` section: `header`, `status` (+
//! `summary`), `instructions`, `controls`, `setup`, `footer`; the
//! per-frame visibility cells live in `cells`.
//!
//! [`DynamicLabel`]: crate::ui::DynamicLabel
//! [`InfoRows`]: crate::ui::InfoRows

mod cells;
mod controls;
mod footer;
mod header;
mod instructions;
mod setup;
mod status;
mod summary;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use agg_gui::geometry::Size;
use agg_gui::widget::Widget;
use agg_gui::widgets::{FlexColumn, Separator, Spacer};

pub use cells::{
    diverted_cell, engine_state_cell, keys_button_cell, open_cell, refresh_visibility_cells,
    watch_cell,
};

use crate::engine::SessionEngine;
use crate::ui::fonts::UiFonts;

pub(crate) type Engine = Rc<RefCell<SessionEngine>>;

/// The Swift `.frame(width: 300)`.
pub const PANEL_WIDTH: f64 = 300.0;

/// Shared visibility state for the sheets and dialogs (the SwiftUI
/// `@State` booleans in `TrainingView` / `BottomBar`).
pub struct SidePanelCells {
    pub show_library: Rc<Cell<bool>>,
    pub show_progress: Rc<Cell<bool>>,
    pub show_calibration: Rc<Cell<bool>>,
    /// The About ("How This Trainer Works") sheet.
    pub show_about: Rc<Cell<bool>>,
    /// The per-player Profile (helpers) sheet.
    pub show_profile: Rc<Cell<bool>>,
    pub show_add_player: Rc<Cell<bool>>,
    pub show_rename_player: Rc<Cell<bool>>,
    /// The add/rename dialogs' text buffer (the Swift `@State userName`).
    pub player_name: Rc<RefCell<String>>,
    /// Bumped on every dialog open so the dialog subtree rebuilds with a
    /// freshly seeded TextField.
    pub dialog_generation: Rc<Cell<u64>>,
    /// Bumped on every Progress open so the sheet re-queries the engine
    /// (the SwiftUI `onAppear` reload).
    pub progress_generation: Rc<Cell<u64>>,
    /// Bumped on every Library open so the song rows re-read their play
    /// stats (the SwiftUI body re-evaluates on appear).
    pub library_generation: Rc<Cell<u64>>,
}

impl SidePanelCells {
    pub fn new() -> Self {
        Self {
            show_library: Rc::new(Cell::new(false)),
            show_progress: Rc::new(Cell::new(false)),
            show_calibration: Rc::new(Cell::new(false)),
            show_about: Rc::new(Cell::new(false)),
            show_profile: Rc::new(Cell::new(false)),
            show_add_player: Rc::new(Cell::new(false)),
            show_rename_player: Rc::new(Cell::new(false)),
            player_name: Rc::new(RefCell::new(String::new())),
            dialog_generation: Rc::new(Cell::new(0)),
            progress_generation: Rc::new(Cell::new(0)),
            library_generation: Rc::new(Cell::new(0)),
        }
    }
}

pub fn build_side_panel(
    engine: &Engine,
    fonts: &UiFonts,
    cells: &SidePanelCells,
) -> Box<dyn Widget> {
    let column = FlexColumn::new()
        .with_gap(14.0)
        .with_padding(14.0)
        // Fixed panel width: without the cap a container child of a
        // FlexRow expands to the full available width.
        .with_min_size(Size::new(PANEL_WIDTH, 0.0))
        .with_max_size(Size::new(PANEL_WIDTH, f64::INFINITY))
        .add(Box::new(header::header(engine, fonts)))
        .add(Box::new(Separator::horizontal().with_line_inset(0.0)))
        .add(Box::new(status::status_section(engine, fonts)))
        .add(Box::new(instructions::instructions_box(engine, fonts)))
        .add(Box::new(controls::controls_section(engine, fonts)))
        .add_flex(Box::new(Spacer::new()), 1.0)
        .add(Box::new(Separator::horizontal().with_line_inset(0.0)))
        .add(Box::new(setup::setup_section(engine, fonts, cells)))
        .add(Box::new(footer::footer_buttons(engine, fonts, cells)));
    Box::new(column)
}

/// A started, in-memory engine for the pure row/text builders' tests.
#[cfg(test)]
pub(super) fn test_engine() -> SessionEngine {
    use crate::audio::NullAudioOut;
    use crate::engine::default_backend_factory;
    use crate::persistence::AppDatabase;

    let mut engine = SessionEngine::new(
        Some(AppDatabase::in_memory(1_700_000_000_000)),
        Rc::new(NullAudioOut),
        Rc::new(|| 1000.0),
        default_backend_factory(),
        42,
    );
    engine.start();
    engine
}
