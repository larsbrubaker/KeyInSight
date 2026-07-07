//! Right-hand panel: what you're doing, how you're doing, what to do, and
//! the controls for it — per activity and input source.
//!
//! Ports `UI/SidePanel.swift` at its exact geometry: 300pt wide, 14pt
//! padding and section spacing, dividers around the status block and the
//! setup section. SwiftUI's observed re-rendering maps to [`DynamicLabel`]
//! and [`InfoRows`] closures reading the engine each frame; `.sheet`
//! bindings map to shared visibility cells.

mod status;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::geometry::Size;
use agg_gui::widget::Widget;
use agg_gui::widgets::{
    Button, Conditional, Container, FlexColumn, FlexRow, Label, Separator, Spacer, ToggleSwitch,
};

use crate::engine::{InputSource, PacingMode, Phase, SessionEngine};
use crate::ui::fonts::{icon, size, UiFonts};
use crate::ui::{palette, DynamicLabel, InfoRows, LevelMeter};

type Engine = Rc<RefCell<SessionEngine>>;

/// The Swift `.frame(width: 300)`.
pub const PANEL_WIDTH: f64 = 300.0;

/// Shared visibility state for the sheets and dialogs (the SwiftUI
/// `@State` booleans in `TrainingView` / `BottomBar`).
pub struct SidePanelCells {
    pub show_library: Rc<Cell<bool>>,
    pub show_progress: Rc<Cell<bool>>,
    pub show_calibration: Rc<Cell<bool>>,
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
}

impl SidePanelCells {
    pub fn new() -> Self {
        Self {
            show_library: Rc::new(Cell::new(false)),
            show_progress: Rc::new(Cell::new(false)),
            show_calibration: Rc::new(Cell::new(false)),
            show_add_player: Rc::new(Cell::new(false)),
            show_rename_player: Rc::new(Cell::new(false)),
            player_name: Rc::new(RefCell::new(String::new())),
            dialog_generation: Rc::new(Cell::new(0)),
            progress_generation: Rc::new(Cell::new(0)),
        }
    }
}

/// Open a sheet/dialog cell and schedule a repaint.
pub fn open_cell(cell: &Rc<Cell<bool>>) {
    cell.set(true);
    agg_gui::animation::request_draw();
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
        .add(Box::new(header(engine, fonts)))
        .add(Box::new(Separator::horizontal().with_line_inset(0.0)))
        .add(Box::new(status_section(engine, fonts)))
        .add(Box::new(instructions_box(engine, fonts)))
        .add(Box::new(controls_section(engine, fonts)))
        .add_flex(Box::new(Spacer::new()), 1.0)
        .add(Box::new(Separator::horizontal().with_line_inset(0.0)))
        .add(Box::new(setup_section(engine, fonts, cells)))
        .add(Box::new(footer_buttons(engine, fonts, cells)));
    Box::new(column)
}

// MARK: - Header

/// Title + activity subtitle + optional exercise info (spacing 3).
fn header(engine: &Engine, fonts: &UiFonts) -> FlexColumn {
    let title = {
        let engine = Rc::clone(engine);
        DynamicLabel::new(
            move || {
                let engine = engine.borrow();
                if engine.is_free_play() {
                    "Free Play".to_string()
                } else if let Some(piece) = engine.active_piece() {
                    piece.title.clone()
                } else if engine.drill_remaining().is_some() {
                    "Micro-drill".to_string()
                } else {
                    format!("Exercise {}", engine.exercises_completed() + 1)
                }
            },
            Arc::clone(&fonts.bold),
        )
        .with_font_size(size::TITLE3)
    };
    let subtitle = {
        let engine = Rc::clone(engine);
        DynamicLabel::new(
            move || {
                let engine = engine.borrow();
                if engine.is_free_play() {
                    "Live notation mirror".to_string()
                } else if engine.active_piece().is_some() {
                    "Repertoire".to_string()
                } else if let Some(remaining) = engine.drill_remaining() {
                    format!(
                        "Card {} of {}",
                        crate::engine::DRILL_LENGTH - remaining + 1,
                        crate::engine::DRILL_LENGTH
                    )
                } else {
                    "Adaptive training".to_string()
                }
            },
            Arc::clone(&fonts.regular),
        )
        .with_font_size(size::CALLOUT)
        .with_dim(true)
    };
    let info = {
        let engine = Rc::clone(engine);
        DynamicLabel::new(
            move || {
                let engine = engine.borrow();
                if engine.is_free_play() {
                    String::new()
                } else {
                    engine.exercise_info().unwrap_or("").to_string()
                }
            },
            Arc::clone(&fonts.regular),
        )
        .with_font_size(size::CALLOUT)
        .with_dim(true)
    };
    FlexColumn::new()
        .with_gap(3.0)
        .add(Box::new(title))
        .add(Box::new(subtitle))
        .add(Box::new(info))
}

// MARK: - Status

fn status_section(engine: &Engine, fonts: &UiFonts) -> InfoRows {
    let engine = Rc::clone(engine);
    InfoRows::new(fonts, move || status::status_rows(&engine.borrow()))
}

// MARK: - Instructions

fn instruction_text(engine: &SessionEngine) -> String {
    if engine.is_free_play() {
        return "Play anything — it appears as notation. Rhythm is simplified; the staff shows your most recent notes.".to_string();
    }
    if engine.drill_remaining().is_some() {
        return "One note at a time, biased toward your weak spots. Hit it as quickly as you can.".to_string();
    }
    match engine.input_source() {
        InputSource::SelfVerify => "Play the phrase on your instrument. Use Hear It to compare, then grade yourself honestly — repeated passes still count as practice.".to_string(),
        InputSource::Microphone => "Play single notes on your instrument near the mic. The meter below shows what it hears; uncertain notes are never marked wrong.".to_string(),
        InputSource::Midi => {
            if engine.mode() == PacingMode::Tempo {
                "Wait for the count-in, then play with the clicks. ◀ early · ▶ late · amber = missed.".to_string()
            } else {
                "Play the blue note on your keyboard; the cursor waits for you. Hover over any symbol to learn its name.".to_string()
            }
        }
        InputSource::Keyboard => {
            if engine.mode() == PacingMode::Tempo {
                "Wait for the count-in, then play with the clicks. ◀ early · ▶ late · amber = missed. A S D F G H J K = C–C, W E T Y U = sharps.".to_string()
            } else {
                "Play the blue note; the cursor waits for you. A S D F G H J K = C–C, W E T Y U = sharps, Z/X shift octave. Hover over any symbol to learn its name.".to_string()
            }
        }
    }
}

/// The rounded gray callout box (`Color.gray.opacity(0.08)`, radius 8,
/// padding 10).
fn instructions_box(engine: &Engine, fonts: &UiFonts) -> Container {
    let engine = Rc::clone(engine);
    let label = DynamicLabel::new(
        move || instruction_text(&engine.borrow()),
        Arc::clone(&fonts.regular),
    )
    .with_font_size(size::CALLOUT)
    .with_dim(true)
    .with_wrap(true);
    Container::new()
        .with_background(Color::rgba(0.5, 0.5, 0.5, 0.08))
        .with_corner_radius(8.0)
        .with_padding(10.0)
        .with_fit_height(true)
        .add(Box::new(label))
}

// MARK: - Controls

fn controls_section(engine: &Engine, fonts: &UiFonts) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(8.0);

    // Hear It / Stop (visible while playback content exists; label and
    // icon swap while playing back — two conditional buttons).
    {
        let visible = hear_it_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(
                Button::new("Hear It", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::PLAY, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().toggle_playback()),
            ),
        )));
    }
    {
        let visible = stop_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(
                Button::new("Stop", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::STOP, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().toggle_playback()),
            ),
        )));
    }
    // Repertoire: start the song over from the top at any point.
    {
        let visible = repertoire_playing_cell(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(Conditional::new(
            visible,
            Box::new(
                Button::new("Restart", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::UNDO, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().next_exercise()),
            ),
        )));
    }
    // Free play: Clear + Exit.
    {
        let visible = free_play_cell(engine);
        let clear = Rc::clone(engine);
        let exit = Rc::clone(engine);
        let row = FlexRow::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Clear", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || clear.borrow_mut().clear_free_play()),
            ))
            .add(Box::new(
                Button::new("Exit Free Play", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || exit.borrow_mut().exit_free_play()),
            ));
        column = column.add(Box::new(Conditional::new(visible, Box::new(row))));
    }
    // Unplugged grading: Nailed It is the prominent default action.
    {
        let visible = self_verify_cell(engine);
        let nailed = Rc::clone(engine);
        let again = Rc::clone(engine);
        let grading = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Nailed It", Arc::clone(&fonts.regular))
                    .with_icon(icon::CHECK, Arc::clone(&fonts.icons))
                    .on_click(move || nailed.borrow_mut().self_verify_grade(true)),
            ))
            .add(Box::new(
                Button::new("Try Again", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::UNDO, Arc::clone(&fonts.icons))
                    .on_click(move || again.borrow_mut().self_verify_grade(false)),
            ));
        column = column.add(Box::new(Conditional::new(visible, Box::new(grading))));
    }
    // Summary, repertoire: Replay + Back to Training.
    {
        let visible = summary_repertoire_cell(engine);
        let replay = Rc::clone(engine);
        let back = Rc::clone(engine);
        let repertoire = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Replay", Arc::clone(&fonts.regular))
                    .on_click(move || replay.borrow_mut().next_exercise()),
            ))
            .add(Box::new(
                Button::new("Back to Training", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .on_click(move || back.borrow_mut().exit_repertoire()),
            ));
        column = column.add(Box::new(Conditional::new(visible, Box::new(repertoire))));
    }
    // Summary, training: Next Exercise (+ auto-continue note on MIDI).
    {
        let visible = summary_training_cell(engine);
        let next = Rc::clone(engine);
        let caption_visible = summary_midi_caption_cell(engine);
        let training = FlexColumn::new()
            .with_gap(8.0)
            .add(Box::new(
                Button::new("Next Exercise", Arc::clone(&fonts.regular))
                    .on_click(move || next.borrow_mut().next_exercise()),
            ))
            .add(Box::new(Conditional::new(
                caption_visible,
                Box::new(
                    Label::new("Continuing automatically…", Arc::clone(&fonts.regular))
                        .with_font_size(size::CAPTION)
                        .with_dim(true),
                ),
            )));
        column = column.add(Box::new(Conditional::new(visible, Box::new(training))));
    }
    column
}

// MARK: - Setup

fn setup_section(engine: &Engine, fonts: &UiFonts, cells: &SidePanelCells) -> FlexColumn {
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

    // Two-hand training exercises (hidden in repertoire, like Swift).
    {
        let visible = training_cell(engine);
        let state = engine_state_cell(engine, |e| e.two_handed());
        let click = Rc::clone(engine);
        let row = toggle_row(
            "Two hands",
            fonts,
            ToggleSwitch::new(engine.borrow().two_handed())
                .with_state_cell(state)
                .on_change(move |on| click.borrow_mut().set_two_handed(on)),
        );
        column = column.add(Box::new(Conditional::new(visible, Box::new(row))));
    }
    // Beginner keys strip user default.
    {
        let state = engine_state_cell(engine, |e| e.keys_user_default());
        let click = Rc::clone(engine);
        let row = toggle_row(
            "Show keys by default",
            fonts,
            ToggleSwitch::new(engine.borrow().keys_user_default())
                .with_state_cell(state)
                .on_change(move |on| click.borrow_mut().set_keys_user_default(on)),
        );
        column = column.add(Box::new(row));
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

/// `[switch] label` — the macOS `.toggleStyle(.switch)` row.
fn toggle_row(label: &str, fonts: &UiFonts, toggle: ToggleSwitch) -> FlexRow {
    FlexRow::new()
        .with_gap(6.0)
        .add(Box::new(toggle))
        .add(Box::new(
            Label::new(label, Arc::clone(&fonts.regular)).with_font_size(size::BODY),
        ))
}

// MARK: - Footer

/// The 2×2 footer grid: Library + Drill, then Free Play across both
/// columns.
fn footer_buttons(engine: &Engine, fonts: &UiFonts, cells: &SidePanelCells) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(8.0);
    let mut row = FlexRow::new().with_gap(8.0);
    {
        let show_library = Rc::clone(&cells.show_library);
        row = row.add_flex(
            Box::new(
                Button::new("Library", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::BOOKS, Arc::clone(&fonts.icons))
                    .on_click(move || open_cell(&show_library)),
            ),
            1.0,
        );
    }
    {
        let click = Rc::clone(engine);
        row = row.add_flex(
            Box::new(
                Button::new("Drill", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::BOLT, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().start_drill()),
            ),
            1.0,
        );
    }
    column = column.add(Box::new(row));
    {
        let enabled = Rc::clone(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(
            Button::new("Free Play", Arc::clone(&fonts.regular))
                .with_subtle().with_active_fn(|| false)
                .with_icon(icon::KEYBOARD, Arc::clone(&fonts.icons))
                .with_enabled_fn(move || enabled.borrow().input_source().supports_timing())
                .on_click(move || click.borrow_mut().enter_free_play()),
        ));
    }
    column
}

// --- Visibility cells, refreshed by the root widget each frame ---
// (agg-gui `Conditional`/`ToggleSwitch` take `Rc<Cell<_>>`; the root's
// tick keeps them in sync with the engine.)

pub fn free_play_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        e.is_free_play() && *e.phase() == Phase::Playing
    })
}

pub fn self_verify_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        e.input_source() == InputSource::SelfVerify
            && *e.phase() == Phase::Playing
            && !e.is_free_play()
    })
}

pub fn diverted_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.is_diverted())
}

pub fn keys_button_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| !e.is_free_play())
}

fn hear_it_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.can_playback() && !e.is_playing_back())
}

fn stop_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.can_playback() && e.is_playing_back())
}

fn repertoire_playing_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        e.active_piece().is_some() && *e.phase() == Phase::Playing && !e.is_free_play()
    })
}

fn summary_repertoire_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        matches!(e.phase(), Phase::Summary(_)) && e.active_piece().is_some()
    })
}

fn summary_training_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        matches!(e.phase(), Phase::Summary(_)) && e.active_piece().is_none()
    })
}

fn summary_midi_caption_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.input_source() == InputSource::Midi)
}

fn mic_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.input_source() == InputSource::Microphone)
}

fn tempo_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.mode() == PacingMode::Tempo)
}

fn training_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.active_piece().is_none())
}

/// A `Cell<bool>` kept in sync with an engine-independent predicate once
/// per frame (dialog error text, external state).
pub fn watch_cell(predicate: impl Fn() -> bool + 'static) -> Rc<Cell<bool>> {
    let cell = Rc::new(Cell::new(predicate()));
    let sync = Rc::clone(&cell);
    register_refresher(Box::new(move |_| sync.set(predicate())));
    cell
}

/// A `Cell<bool>` kept in sync with an engine predicate once per frame.
pub fn engine_state_cell(
    engine: &Engine,
    predicate: impl Fn(&SessionEngine) -> bool + 'static,
) -> Rc<Cell<bool>> {
    let cell = Rc::new(Cell::new(predicate(&engine.borrow())));
    let sync = Rc::clone(&cell);
    register_refresher(Box::new(move |engine| {
        sync.set(predicate(engine));
    }));
    cell
}

/// Per-frame refresh plumbing: closures evaluated once per frame by the
/// root widget (see `ui/app.rs`).
type CellRefresher = Box<dyn Fn(&SessionEngine)>;

thread_local! {
    static CELL_REFRESHERS: RefCell<Vec<CellRefresher>> = const { RefCell::new(Vec::new()) };
}

fn register_refresher(refresher: CellRefresher) {
    CELL_REFRESHERS.with(|refreshers| {
        refreshers.borrow_mut().push(refresher);
    });
}

/// Run every registered refresher against the engine state.
pub fn refresh_visibility_cells(engine: &SessionEngine) {
    CELL_REFRESHERS.with(|refreshers| {
        for refresh in refreshers.borrow().iter() {
            refresh(engine);
        }
    });
}
