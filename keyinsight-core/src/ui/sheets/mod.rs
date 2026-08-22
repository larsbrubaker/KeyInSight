//! The modal sheets and dialogs over the training view.
//!
//! Ports the SwiftUI `.sheet`/`.alert` presentations from
//! `UI/TrainingView.swift` + `UI/BottomBar.swift`: each becomes an
//! agg-gui [`ModalSheet`] (centered fixed-size panel over a scrim)
//! stacked above the app — Library, Progress, latency Calibration, About,
//! the player Profile, and the add/rename player dialogs.

mod about;
mod calibration;
#[cfg(test)]
pub(crate) mod layout_test_support;
mod library;
mod player_dialogs;
mod profile;
mod progress;

pub use player_dialogs::{ADD_NAME_FOCUS, RENAME_NAME_FOCUS};

use std::cell::RefCell;
use std::rc::Rc;

use agg_gui::widget::Widget;
use agg_gui::widgets::Stack;

use crate::engine::SessionEngine;
use crate::ui::app::SharedPlatform;
use crate::ui::fonts::UiFonts;
use crate::ui::side_panel::SidePanelCells;

pub(crate) type Engine = Rc<RefCell<SessionEngine>>;
pub(crate) type Clock = Rc<dyn Fn() -> f64>;

/// All sheets, stacked above the training view. Each is scrim-gated by
/// its own visibility cell, so at most one is interactable at a time.
pub fn build_sheet_overlay(
    engine: &Engine,
    fonts: &UiFonts,
    clock: &Clock,
    cells: &SidePanelCells,
    platform: &SharedPlatform,
) -> Box<dyn Widget> {
    Box::new(
        Stack::new()
            // The overlay layer must not eat clicks meant for the training
            // view while every sheet is hidden.
            .with_hit_children_only(true)
            .add(library::build_library_sheet(engine, fonts, cells, platform))
            .add(progress::build_progress_sheet(engine, fonts, clock, cells))
            .add(calibration::build_calibration_sheet(engine, fonts, clock, cells))
            .add(about::build_about_sheet(fonts, cells))
            .add(profile::build_profile_sheet(engine, fonts, cells))
            .add(player_dialogs::build_add_player_dialog(engine, fonts, cells))
            .add(player_dialogs::build_rename_player_dialog(engine, fonts, cells)),
    )
}
