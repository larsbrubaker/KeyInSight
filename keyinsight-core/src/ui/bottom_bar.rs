//! Window-wide bottom bar: who is playing (picker + rename + add +
//! profile), plus session-level navigation (Keys, Resume Training,
//! Progress, About).
//!
//! Ports `UI/BottomBar.swift`: the SwiftUI `Picker` maps to a `ComboBox`
//! rebuilt when the player list changes, the borderless icon buttons map
//! to ghost buttons with Font Awesome glyphs, and the add/rename alerts
//! map to the modal dialogs in `ui/sheets/player_dialogs.rs` (shared
//! visibility cells + name buffer).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::geometry::Size;
use agg_gui::layout_props::Insets;
use agg_gui::widget::Widget;
use agg_gui::widgets::{
    Button, ComboBox, Conditional, Container, FlexRow, Label, Rebuilder,
};

use crate::engine::SessionEngine;
use crate::ui::fonts::{icon, size, UiFonts};
use crate::ui::side_panel::{self, open_cell, SidePanelCells};

type Engine = Rc<RefCell<SessionEngine>>;

/// Bar height (the Swift bar's padding + control height).
pub const BAR_HEIGHT: f64 = 44.0;

pub fn build_bottom_bar(
    engine: &Engine,
    fonts: &UiFonts,
    cells: &SidePanelCells,
) -> Box<dyn Widget> {
    let mut row = FlexRow::new().with_gap(10.0);

    // Player glyph + picker + rename + add.
    row = row.add(Box::new(
        Label::new(icon::USER_CIRCLE.to_string(), Arc::clone(&fonts.icons))
            .with_font_size(14.0)
            .with_dim(true),
    ));
    // The Swift picker is `.fixedSize()`; ComboBox fills its slot, so cap
    // the slot instead.
    row = row.add(Box::new(
        agg_gui::widgets::SizedBox::new()
            .with_width(150.0)
            .with_child(Box::new(player_picker(engine, fonts))),
    ));
    {
        let click = Rc::clone(engine);
        let name = Rc::clone(&cells.player_name);
        let generation = Rc::clone(&cells.dialog_generation);
        let show = Rc::clone(&cells.show_rename_player);
        let enabled = Rc::clone(engine);
        row = row.add(Box::new(
            Button::new("", Arc::clone(&fonts.regular))
                .with_ghost().with_active_fn(|| false)
                .with_compact()
                .with_icon(icon::PENCIL, Arc::clone(&fonts.icons))
                .with_enabled_fn(move || enabled.borrow().current_user().is_some())
                .on_click(move || {
                    let current = click
                        .borrow()
                        .current_user()
                        .map(|user| user.name.clone())
                        .unwrap_or_default();
                    *name.borrow_mut() = current;
                    generation.set(generation.get() + 1);
                    open_cell(&show);
                    agg_gui::focus::request_focus(crate::ui::sheets::RENAME_NAME_FOCUS);
                }),
        ));
    }
    {
        let name = Rc::clone(&cells.player_name);
        let generation = Rc::clone(&cells.dialog_generation);
        let show = Rc::clone(&cells.show_add_player);
        row = row.add(Box::new(
            Button::new("", Arc::clone(&fonts.regular))
                .with_ghost().with_active_fn(|| false)
                .with_compact()
                .with_icon(icon::USER_PLUS, Arc::clone(&fonts.icons))
                .on_click(move || {
                    name.borrow_mut().clear();
                    generation.set(generation.get() + 1);
                    open_cell(&show);
                    agg_gui::focus::request_focus(crate::ui::sheets::ADD_NAME_FOCUS);
                }),
        ));
    }

    // Player settings (help "Player settings — octave following, keys
    // strip"); needs a current player, like Rename.
    {
        let enabled = Rc::clone(engine);
        let show = Rc::clone(&cells.show_profile);
        row = row.add(Box::new(
            Button::new("", Arc::clone(&fonts.regular))
                .with_ghost().with_active_fn(|| false)
                .with_compact()
                .with_icon(icon::SLIDERS, Arc::clone(&fonts.icons))
                .with_enabled_fn(move || enabled.borrow().current_user().is_some())
                .on_click(move || open_cell(&show)),
        ));
    }

    row = row.add_flex(Box::new(crate::ui::hspacer()), 1.0);

    // Keys toggle for the current context (hidden in free play; the
    // active state marks the strip being shown).
    {
        let visible = side_panel::keys_button_cell(engine);
        let active = Rc::clone(engine);
        let click = Rc::clone(engine);
        row = row.add(Box::new(Conditional::new(
            visible,
            Box::new(
                Button::new("Keys", Arc::clone(&fonts.regular))
                    .with_ghost()
                    .with_compact()
                    .with_icon(icon::KEYBOARD, Arc::clone(&fonts.icons))
                    .with_active_fn(move || active.borrow().show_keys())
                    .on_click(move || click.borrow_mut().toggle_keys_for_context()),
            ),
        )));
    }
    // Resume Training (only when diverted).
    {
        let diverted = side_panel::diverted_cell(engine);
        let click = Rc::clone(engine);
        row = row.add(Box::new(Conditional::new(
            diverted,
            Box::new(
                Button::new("Resume Training", Arc::clone(&fonts.regular))
                    .with_ghost().with_active_fn(|| false)
                    .with_compact()
                    .with_icon(icon::UNDO, Arc::clone(&fonts.icons))
                    .on_click(move || click.borrow_mut().resume_training()),
            ),
        )));
    }
    // Progress sheet (bump the generation so the sheet re-queries the
    // engine, like the SwiftUI onAppear).
    {
        let show_progress = Rc::clone(&cells.show_progress);
        let generation = Rc::clone(&cells.progress_generation);
        row = row.add(Box::new(
            Button::new("Progress", Arc::clone(&fonts.regular))
                .with_ghost().with_active_fn(|| false)
                .with_compact()
                .with_icon(icon::CHART_BAR, Arc::clone(&fonts.icons))
                .on_click(move || {
                    generation.set(generation.get() + 1);
                    open_cell(&show_progress);
                }),
        ));
    }
    // About (help "How this trainer works — the skills, the vocabulary,
    // the modes").
    {
        let show_about = Rc::clone(&cells.show_about);
        row = row.add(Box::new(
            Button::new("About", Arc::clone(&fonts.regular))
                .with_ghost().with_active_fn(|| false)
                .with_compact()
                .with_icon(icon::QUESTION_CIRCLE, Arc::clone(&fonts.icons))
                .on_click(move || open_cell(&show_about)),
        ));
    }

    // `.background(.bar)` + `.padding(h14, v8)`, capped at the bar height
    // (without the cap a container child of a FlexColumn expands to the
    // full available height).
    Box::new(
        Container::new()
            .with_background(Color::rgba(0.5, 0.5, 0.5, 0.06))
            .with_inner_padding(Insets {
                left: 14.0,
                right: 14.0,
                top: 8.0,
                bottom: 8.0,
            })
            .with_min_size(Size::new(0.0, BAR_HEIGHT))
            .with_max_size(Size::new(f64::INFINITY, BAR_HEIGHT))
            .add(Box::new(row)),
    )
}

/// The player `Picker`: a ComboBox rebuilt whenever the user list or the
/// selection changes from outside (adds, renames, initial load).
fn player_picker(engine: &Engine, fonts: &UiFonts) -> Rebuilder {
    let version_engine = Rc::clone(engine);
    let build_engine = Rc::clone(engine);
    let font = Arc::clone(&fonts.regular);
    Rebuilder::new(
        move || {
            let engine = version_engine.borrow();
            let mut hash: u64 = engine.current_user().map(|u| u.id as u64).unwrap_or(0);
            for user in engine.users() {
                for byte in user.name.bytes() {
                    hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
                }
                hash = hash.wrapping_mul(31).wrapping_add(user.id as u64);
            }
            hash
        },
        move || {
            let engine_ref = build_engine.borrow();
            let users = engine_ref.users();
            let names: Vec<String> = users.iter().map(|u| u.name.clone()).collect();
            let ids: Vec<i64> = users.iter().map(|u| u.id).collect();
            let selected = engine_ref
                .current_user()
                .and_then(|current| ids.iter().position(|&id| id == current.id))
                .unwrap_or(0);
            drop(engine_ref);
            let switch = Rc::clone(&build_engine);
            let names = if names.is_empty() {
                vec!["—".to_string()]
            } else {
                names
            };
            Box::new(
                ComboBox::new(names, selected, Arc::clone(&font))
                    .with_font_size(size::BODY)
                    .on_change(move |index| {
                        let mut engine = switch.borrow_mut();
                        let users: Vec<i64> = engine.users().iter().map(|u| u.id).collect();
                        if let Some(&id) = users.get(index) {
                            engine.switch_user(id);
                        }
                    }),
            )
        },
    )
}
