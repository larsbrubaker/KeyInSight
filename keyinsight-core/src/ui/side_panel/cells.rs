//! Visibility cells: `Cell<bool>`s the root widget refreshes from the
//! engine once per frame (agg-gui `Conditional`/`ToggleSwitch` take
//! `Rc<Cell<_>>`; the root's tick keeps them in sync with the engine —
//! the SwiftUI `if engine.…` view conditions).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::engine::{InputSource, PacingMode, Phase, SessionEngine};

use super::Engine;

/// Open a sheet/dialog cell and schedule a repaint.
pub fn open_cell(cell: &Rc<Cell<bool>>) {
    cell.set(true);
    agg_gui::animation::request_draw();
}

pub fn free_play_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        e.is_free_play() && *e.phase() == Phase::Playing
    })
}

/// Free play's Play button (hidden while the take replays).
pub fn free_play_play_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        e.is_free_play() && *e.phase() == Phase::Playing && !e.is_playing_back()
    })
}

/// Free play's Stop button (the Play label swapped while replaying).
pub fn free_play_stop_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        e.is_free_play() && *e.phase() == Phase::Playing && e.is_playing_back()
    })
}

pub fn self_verify_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        e.input_source() == InputSource::SelfVerify
            && *e.phase() == Phase::Playing
            && !e.is_free_play()
    })
}

/// End Drill inside the self-verify grading block.
pub fn drill_self_verify_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        e.input_source() == InputSource::SelfVerify
            && *e.phase() == Phase::Playing
            && !e.is_free_play()
            && e.drill_active()
    })
}

/// `.playing where isSurvival` — the End Run button.
pub fn survival_playing_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.is_survival() && *e.phase() == Phase::Playing)
}

/// `.playing where drillActive` — the End Drill button (the self-verify
/// branch carries its own copy, so it is excluded here).
pub fn drill_playing_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        e.drill_active()
            && *e.phase() == Phase::Playing
            && !e.is_free_play()
            && e.input_source() != InputSource::SelfVerify
    })
}

pub fn diverted_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.is_diverted())
}

pub fn keys_button_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| !e.is_free_play())
}

pub fn hear_it_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.can_playback() && !e.is_playing_back())
}

pub fn stop_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.can_playback() && e.is_playing_back())
}

pub fn repertoire_playing_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        e.active_piece().is_some() && *e.phase() == Phase::Playing && !e.is_free_play()
    })
}

/// A summary that closed a survival run (Run It Back).
pub fn summary_survival_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        matches!(e.phase(), Phase::Summary(summary) if summary.survival.is_some())
    })
}

pub fn summary_repertoire_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        matches!(e.phase(), Phase::Summary(summary) if summary.survival.is_none())
            && e.active_piece().is_some()
    })
}

pub fn summary_training_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        matches!(e.phase(), Phase::Summary(summary) if summary.survival.is_none())
            && e.active_piece().is_none()
    })
}

pub fn summary_midi_caption_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.input_source() == InputSource::Midi)
}

pub fn mic_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.input_source() == InputSource::Microphone)
}

/// The Calibrate button follows the USER's tempo choice (SidePanel.swift
/// gates on `engine.mode`), not the content-dependent active pacing.
pub fn tempo_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.mode() == PacingMode::Tempo)
}

pub fn training_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| e.active_piece().is_none())
}

/// The practice-from-here chip under the repertoire header.
pub fn replay_start_cell(engine: &Engine) -> Rc<Cell<bool>> {
    engine_state_cell(engine, |e| {
        !e.is_free_play() && !e.is_survival() && e.active_piece().is_some()
            && e.replay_start_event() > 0
    })
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
