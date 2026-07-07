//! Right-hand panel: what you're doing, how you're doing, what to do, and
//! the controls for it — per activity and input source.
//!
//! Ports `UI/SidePanel.swift`. SwiftUI's observed re-rendering maps to
//! [`DynamicLabel`]s reading the engine each frame; pickers map to button
//! rows with active states.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::text::Font;
use agg_gui::widget::Widget;
use agg_gui::widgets::{Button, Checkbox, FlexColumn, FlexRow, Spacer};

use crate::engine::{InputSource, PacingMode, Phase, SessionEngine};
use crate::ui::DynamicLabel;

type Engine = Rc<RefCell<SessionEngine>>;

pub struct SidePanelCells {
    pub show_library: Rc<Cell<bool>>,
    pub show_progress: Rc<Cell<bool>>,
}

pub fn build_side_panel(
    engine: &Engine,
    font: &Arc<Font>,
    cells: &SidePanelCells,
) -> Box<dyn Widget> {
    let mut column = FlexColumn::new().with_gap(8.0).with_padding(14.0);

    column = column
        .add(Box::new(header_label(engine, font)))
        .add(Box::new(subheader_label(engine, font)))
        .add(Box::new(info_label(engine, font)))
        .add(Box::new(status_labels(engine, font)))
        .add(Box::new(instructions_label(engine, font)))
        .add(Box::new(controls_section(engine, font)));

    column = column.add_flex(Box::new(Spacer::new()), 1.0);
    column = column
        .add(Box::new(setup_section(engine, font)))
        .add(Box::new(footer_buttons(engine, font, cells)));

    Box::new(column)
}

fn header_label(engine: &Engine, font: &Arc<Font>) -> DynamicLabel {
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
        Arc::clone(font),
    )
    .with_font_size(18.0)
}

fn subheader_label(engine: &Engine, font: &Arc<Font>) -> DynamicLabel {
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
        Arc::clone(font),
    )
    .with_font_size(13.0)
    .with_dim(true)
}

fn info_label(engine: &Engine, font: &Arc<Font>) -> DynamicLabel {
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
        Arc::clone(font),
    )
    .with_font_size(13.0)
    .with_dim(true)
}

/// The status block: one multi-line dynamic label mirroring the Swift
/// status section's cases (playing / free play / summary / failed).
fn status_labels(engine: &Engine, font: &Arc<Font>) -> DynamicLabel {
    let engine = Rc::clone(engine);
    DynamicLabel::new(
        move || {
            let engine = engine.borrow();
            status_text(&engine)
        },
        Arc::clone(font),
    )
    .with_font_size(13.0)
}

fn status_text(engine: &SessionEngine) -> String {
    match engine.phase() {
        Phase::Loading => "Preparing…".to_string(),
        Phase::Playing if engine.is_free_play() => {
            let mut lines = vec![format!("{} notes played", engine.free_play_count())];
            if let Some(last) = engine.last_free_play_note() {
                lines.push(format!("Last note: {last}"));
            }
            lines.join("\n")
        }
        Phase::Playing => {
            let mut lines = Vec::new();
            if engine.input_source() == InputSource::SelfVerify {
                lines.push(format!("Pass {}", engine.self_verify_attempts() + 1));
            } else {
                lines.push(format!(
                    "Note {} of {}",
                    engine.current_note_index() + 1,
                    engine.note_count()
                ));
            }
            if engine.errors_this_exercise() > 0 {
                lines.push(format!("{} wrong", engine.errors_this_exercise()));
            }
            if engine.streak() >= 5 {
                lines.push(format!("🔥 {} first-try streak", engine.streak()));
            }
            if engine.anchored_octaves() != 0 {
                let sign = if engine.anchored_octaves() > 0 { "+" } else { "" };
                lines.push(format!(
                    "Following your octave ({sign}{})",
                    engine.anchored_octaves()
                ));
            }
            if engine.heard_uncertain() {
                lines.push("Heard something — couldn't tell what".to_string());
            }
            if engine.mode() == PacingMode::Tempo {
                if let Some(count_in) = engine.count_in_remaining() {
                    lines.push(format!("Ready… {count_in}"));
                } else {
                    let dots: String = (0..4)
                        .map(|beat| {
                            if beat == engine.beat_in_measure() {
                                '●'
                            } else {
                                '○'
                            }
                        })
                        .collect();
                    lines.push(dots);
                }
                lines.push(format!("{} BPM", engine.tempo_bpm() as i64));
            }
            lines.join("\n")
        }
        Phase::Summary(summary) => {
            let mut lines = Vec::new();
            if summary.drill {
                lines.push("Micro-drill complete".to_string());
            } else if summary.self_verified {
                lines.push("✓ Self-verified".to_string());
            }
            if let Some(timing) = &summary.timing {
                lines.push(format!(
                    "{}% in the window",
                    (timing.hit_rate() * 100.0).round() as i64
                ));
                lines.push(format!(
                    "{} on time · {} early · {} late · {} missed",
                    timing.on_time, timing.early, timing.late, timing.missed
                ));
                if let Some(offset) = timing.mean_abs_offset_ms {
                    lines.push(format!("±{offset:.0} ms mean offset"));
                }
            } else {
                lines.push(format!("{}% first try", summary.accuracy_percent()));
                lines.push(format!(
                    "{} of {} notes",
                    summary.first_try_correct, summary.note_count
                ));
                if let Some(latency) = summary.mean_latency_ms {
                    lines.push(format!("{:.1} s per note", latency / 1000.0));
                }
            }
            if summary.error_count > 0 {
                lines.push(if summary.self_verified {
                    format!(
                        "{} repeated {}",
                        summary.error_count,
                        if summary.error_count == 1 { "pass" } else { "passes" }
                    )
                } else {
                    format!("{} wrong notes", summary.error_count)
                });
            }
            if let Some((number, errors)) = summary.worst_measure {
                lines.push(format!("Measure {number} is your trouble spot ({errors})"));
            }
            if let Some(unlocked) = &summary.newly_unlocked {
                lines.push(format!("🔓 {unlocked} unlocked!"));
            }
            if let Some(rhythm) = &summary.rhythm_unlocked {
                lines.push(format!("♪ New rhythm: {rhythm}!"));
            }
            lines.join("\n")
        }
        Phase::Failed(message) => format!("⚠ {message}"),
    }
}

fn instructions_label(engine: &Engine, font: &Arc<Font>) -> DynamicLabel {
    let engine = Rc::clone(engine);
    DynamicLabel::new(
        move || instruction_text(&engine.borrow()),
        Arc::clone(font),
    )
    .with_font_size(12.0)
    .with_dim(true)
}

fn instruction_text(engine: &SessionEngine) -> String {
    if engine.is_free_play() {
        return "Play anything — it appears as notation. Rhythm is simplified; the staff shows your most recent notes.".to_string();
    }
    if engine.drill_remaining().is_some() {
        return "One note at a time, biased toward your weak spots. Hit it as quickly as you can.".to_string();
    }
    match engine.input_source() {
        InputSource::SelfVerify => "Play the phrase on your instrument. Use Hear It to compare, then grade yourself honestly — repeated passes still count as practice.".to_string(),
        InputSource::Microphone => "Play single notes on your instrument near the mic. Uncertain notes are never marked wrong.".to_string(),
        InputSource::Midi => {
            if engine.mode() == PacingMode::Tempo {
                "Wait for the count-in, then play with the clicks. ◂ early · ▸ late · amber = missed.".to_string()
            } else {
                "Play the blue note on your keyboard; the cursor waits for you. Hover over any symbol to learn its name.".to_string()
            }
        }
        InputSource::Keyboard => {
            if engine.mode() == PacingMode::Tempo {
                "Wait for the count-in, then play with the clicks. ◂ early · ▸ late · amber = missed. A S D F G H J K = C–C, W E T Y U = sharps.".to_string()
            } else {
                "Play the blue note; the cursor waits for you. A S D F G H J K = C–C, W E T Y U = sharps, Z/X shift octave. Hover over any symbol to learn its name.".to_string()
            }
        }
    }
}

fn controls_section(engine: &Engine, font: &Arc<Font>) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(6.0);

    // Hear It / Stop.
    {
        let active = Rc::clone(engine);
        let click = Rc::clone(engine);
        column = column.add(Box::new(
            Button::new("Hear It", Arc::clone(font))
                .with_active_fn(move || active.borrow().is_playing_back())
                .on_click(move || click.borrow_mut().toggle_playback()),
        ));
    }
    // Free play: Clear + Exit.
    {
        let visible = free_play_cell(engine);
        let clear = Rc::clone(engine);
        let exit = Rc::clone(engine);
        let row = FlexRow::new()
            .with_gap(6.0)
            .add(Box::new(
                Button::new("Clear", Arc::clone(font))
                    .on_click(move || clear.borrow_mut().clear_free_play()),
            ))
            .add(Box::new(
                Button::new("Exit Free Play", Arc::clone(font))
                    .on_click(move || exit.borrow_mut().exit_free_play()),
            ));
        column = column.add(Box::new(agg_gui::widgets::Conditional::new(
            visible,
            Box::new(row),
        )));
    }
    // Unplugged grading.
    {
        let visible = self_verify_cell(engine);
        let nailed = Rc::clone(engine);
        let again = Rc::clone(engine);
        let row = FlexColumn::new()
            .with_gap(6.0)
            .add(Box::new(
                Button::new("Nailed It", Arc::clone(font))
                    .on_click(move || nailed.borrow_mut().self_verify_grade(true)),
            ))
            .add(Box::new(
                Button::new("Try Again", Arc::clone(font))
                    .on_click(move || again.borrow_mut().self_verify_grade(false)),
            ));
        column = column.add(Box::new(agg_gui::widgets::Conditional::new(
            visible,
            Box::new(row),
        )));
    }
    // Summary: Next Exercise (doubles as Replay in repertoire) + Back.
    {
        let visible = summary_cell(engine);
        let next = Rc::clone(engine);
        let back_visible = repertoire_cell(engine);
        let back = Rc::clone(engine);
        let column_inner = FlexColumn::new()
            .with_gap(6.0)
            .add(Box::new(
                Button::new("Next Exercise", Arc::clone(font))
                    .on_click(move || next.borrow_mut().next_exercise()),
            ))
            .add(Box::new(agg_gui::widgets::Conditional::new(
                back_visible,
                Box::new(
                    Button::new("Back to Training", Arc::clone(font))
                        .on_click(move || back.borrow_mut().exit_repertoire()),
                ),
            )));
        column = column.add(Box::new(agg_gui::widgets::Conditional::new(
            visible,
            Box::new(column_inner),
        )));
    }
    column
}

fn setup_section(engine: &Engine, font: &Arc<Font>) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(6.0);

    // Input source picker (segmented row).
    let mut input_row = FlexRow::new().with_gap(4.0);
    for source in [
        InputSource::Midi,
        InputSource::Keyboard,
        InputSource::Microphone,
        InputSource::SelfVerify,
    ] {
        let active = Rc::clone(engine);
        let click = Rc::clone(engine);
        input_row = input_row.add(Box::new(
            Button::new(source.label(), Arc::clone(font))
                .with_subtle()
                .with_compact()
                .with_active_fn(move || active.borrow().input_source() == source)
                .on_click(move || click.borrow_mut().set_input_source(source)),
        ));
    }
    column = column.add(Box::new(input_row));

    // Pacing picker.
    let mut pacing_row = FlexRow::new().with_gap(4.0);
    for mode in [PacingMode::SelfPaced, PacingMode::Tempo] {
        let active = Rc::clone(engine);
        let click = Rc::clone(engine);
        pacing_row = pacing_row.add(Box::new(
            Button::new(mode.label(), Arc::clone(font))
                .with_subtle()
                .with_compact()
                .with_active_fn(move || active.borrow().mode() == mode)
                .on_click(move || {
                    let mut engine = click.borrow_mut();
                    // Tempo needs an exact source and monophonic content
                    // (the Swift picker was disabled in those states).
                    if mode == PacingMode::Tempo
                        && !(engine.input_source().supports_timing()
                            && engine.content_supports_tempo())
                    {
                        return;
                    }
                    engine.set_mode(mode);
                }),
        ));
    }
    column = column.add(Box::new(pacing_row));

    // Two hands + beginner keys toggles.
    {
        let click = Rc::clone(engine);
        let initial = engine.borrow().two_handed();
        column = column.add(Box::new(
            Checkbox::new("Two hands", Arc::clone(font), initial)
                .on_change(move |on| click.borrow_mut().set_two_handed(on)),
        ));
    }
    {
        let click = Rc::clone(engine);
        let initial = engine.borrow().keys_user_default();
        column = column.add(Box::new(
            Checkbox::new("Show keys by default", Arc::clone(font), initial)
                .on_change(move |on| click.borrow_mut().set_keys_user_default(on)),
        ));
    }

    // Octave offset readout (keyboard input).
    {
        let engine = Rc::clone(engine);
        column = column.add(Box::new(
            DynamicLabel::new(
                move || {
                    let offset = engine.borrow().octave_offset();
                    if offset != 0 {
                        format!("Octave {}{offset}", if offset > 0 { "+" } else { "" })
                    } else {
                        String::new()
                    }
                },
                Arc::clone(font),
            )
            .with_font_size(12.0)
            .with_color_fn(|| Some(Color::from_rgb8(0x1D, 0x6F, 0xD6))),
        ));
    }
    column
}

fn footer_buttons(engine: &Engine, font: &Arc<Font>, cells: &SidePanelCells) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(6.0);
    let mut row = FlexRow::new().with_gap(6.0);
    {
        let show_library = Rc::clone(&cells.show_library);
        row = row.add(Box::new(Button::new("Library", Arc::clone(font)).on_click(
            move || {
                show_library.set(true);
                agg_gui::animation::request_draw();
            },
        )));
    }
    {
        let click = Rc::clone(engine);
        row = row.add(Box::new(Button::new("Drill", Arc::clone(font)).on_click(
            move || click.borrow_mut().start_drill(),
        )));
    }
    column = column.add(Box::new(row));
    {
        let click = Rc::clone(engine);
        column = column.add(Box::new(
            Button::new("Free Play", Arc::clone(font)).on_click(move || {
                let mut engine = click.borrow_mut();
                if engine.input_source().supports_timing() {
                    engine.enter_free_play();
                }
            }),
        ));
    }
    column
}

// --- Visibility cells, refreshed by the root widget each frame ---
// (agg-gui `Conditional` takes a `Rc<Cell<bool>>`; the root's tick keeps
// them in sync with the engine.)

pub fn free_play_cell(engine: &Engine) -> Rc<Cell<bool>> {
    let cell = Rc::new(Cell::new(false));
    register_cell(engine, Rc::clone(&cell), |e| {
        e.is_free_play() && *e.phase() == Phase::Playing
    });
    cell
}

pub fn self_verify_cell(engine: &Engine) -> Rc<Cell<bool>> {
    let cell = Rc::new(Cell::new(false));
    register_cell(engine, Rc::clone(&cell), |e| {
        e.input_source() == InputSource::SelfVerify
            && *e.phase() == Phase::Playing
            && !e.is_free_play()
    });
    cell
}

pub fn summary_cell(engine: &Engine) -> Rc<Cell<bool>> {
    let cell = Rc::new(Cell::new(false));
    register_cell(engine, Rc::clone(&cell), |e| {
        matches!(e.phase(), Phase::Summary(_))
    });
    cell
}

pub fn repertoire_cell(engine: &Engine) -> Rc<Cell<bool>> {
    let cell = Rc::new(Cell::new(false));
    register_cell(engine, Rc::clone(&cell), |e| e.active_piece().is_some());
    cell
}

pub fn diverted_cell(engine: &Engine) -> Rc<Cell<bool>> {
    let cell = Rc::new(Cell::new(false));
    register_cell(engine, Rc::clone(&cell), |e| e.is_diverted());
    cell
}

/// Conditional-visibility plumbing: closures evaluated once per frame by
/// the root widget (see `ui/app.rs`).
type CellRefresher = Box<dyn Fn(&SessionEngine)>;

thread_local! {
    static CELL_REFRESHERS: RefCell<Vec<CellRefresher>> = const { RefCell::new(Vec::new()) };
}

fn register_cell(
    _engine: &Engine,
    cell: Rc<Cell<bool>>,
    predicate: impl Fn(&SessionEngine) -> bool + 'static,
) {
    CELL_REFRESHERS.with(|refreshers| {
        refreshers.borrow_mut().push(Box::new(move |engine| {
            cell.set(predicate(engine));
        }));
    });
}

/// Run every registered visibility predicate against the engine state.
pub fn refresh_visibility_cells(engine: &SessionEngine) {
    CELL_REFRESHERS.with(|refreshers| {
        for refresh in refreshers.borrow().iter() {
            refresh(engine);
        }
    });
}
