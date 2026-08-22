//! The add/rename player dialogs — the `BottomBar.swift` alerts
//! (`"New Player"` / `"Rename Player"`) as small modal sheets with a
//! text field.
//!
//! Each dialog's content sits in a [`Rebuilder`] keyed on
//! `dialog_generation`: the opening button bumps the generation after
//! seeding `player_name`, so the TextField is recreated with the fresh
//! buffer (the SwiftUI `@State userName` handoff).

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::geometry::Size;
use agg_gui::widget::Widget;
use agg_gui::widgets::{
    Button, FlexColumn, FlexRow, Label, ModalSheet, Rebuilder, TextField,
};

use crate::ui::fonts::{size, UiFonts};
use crate::ui::side_panel::SidePanelCells;

use super::Engine;

/// macOS alert proportions.
const DIALOG_SIZE: Size = Size {
    width: 300.0,
    height: 190.0,
};

/// Programmatic-focus channels: the opening buttons request these so the
/// name field is ready to type into, like the Swift alert's text field.
pub const ADD_NAME_FOCUS: agg_gui::focus::FocusId = 0x4B49_5341; // "KISA"
pub const RENAME_NAME_FOCUS: agg_gui::focus::FocusId = 0x4B49_5352; // "KISR"

pub fn build_add_player_dialog(
    engine: &Engine,
    fonts: &UiFonts,
    cells: &SidePanelCells,
) -> Box<dyn Widget> {
    let visible = Rc::clone(&cells.show_add_player);
    let commit_engine = Rc::clone(engine);
    build_dialog(
        fonts,
        cells,
        &visible,
        "New Player",
        Some("Each player gets their own progress, unlocks, and tempo."),
        "Add",
        ADD_NAME_FOCUS,
        move |name| {
            commit_engine.borrow_mut().add_user(name);
        },
    )
}

pub fn build_rename_player_dialog(
    engine: &Engine,
    fonts: &UiFonts,
    cells: &SidePanelCells,
) -> Box<dyn Widget> {
    let visible = Rc::clone(&cells.show_rename_player);
    let commit_engine = Rc::clone(engine);
    build_dialog(
        fonts,
        cells,
        &visible,
        "Rename Player",
        None,
        "Save",
        RENAME_NAME_FOCUS,
        move |name| {
            let mut engine = commit_engine.borrow_mut();
            if let Some(id) = engine.current_user().map(|user| user.id) {
                engine.rename_user(id, name);
            }
        },
    )
}

/// Title, optional message, name field, Cancel + commit buttons.
#[allow(clippy::too_many_arguments)]
fn build_dialog(
    fonts: &UiFonts,
    cells: &SidePanelCells,
    visible: &Rc<std::cell::Cell<bool>>,
    title: &'static str,
    message: Option<&'static str>,
    commit_label: &'static str,
    focus_id: agg_gui::focus::FocusId,
    commit: impl Fn(&str) + 'static,
) -> Box<dyn Widget> {
    let generation = Rc::clone(&cells.dialog_generation);
    let name = Rc::clone(&cells.player_name);
    let fonts = fonts.clone();
    let visible_for_build = Rc::clone(visible);
    let commit = Rc::new(commit);

    let content = Rebuilder::new(
        move || generation.get(),
        move || {
            let mut column = FlexColumn::new().with_gap(12.0).with_padding(16.0);
            column = column.add(Box::new(
                Label::new(title, Arc::clone(&fonts.bold)).with_font_size(size::TITLE3),
            ));
            if let Some(message) = message {
                column = column.add(Box::new(
                    Label::new(message, Arc::clone(&fonts.regular))
                        .with_font_size(size::CALLOUT)
                        .with_dim(true)
                        .with_wrap(true),
                ));
            }
            {
                let buffer = Rc::clone(&name);
                let enter_commit = Rc::clone(&commit);
                let enter_visible = Rc::clone(&visible_for_build);
                let field = TextField::new(Arc::clone(&fonts.regular))
                    .with_font_size(size::BODY)
                    .with_placeholder("Name")
                    .with_text(name.borrow().clone())
                    .with_focus_id(focus_id)
                    .on_change(move |text| {
                        *buffer.borrow_mut() = text.to_string();
                    })
                    // Return commits, like the alert's default action.
                    .on_enter(move |text| {
                        let text = text.trim();
                        if !text.is_empty() {
                            enter_commit(text);
                        }
                        enter_visible.set(false);
                        agg_gui::animation::request_draw();
                    });
                column = column.add(Box::new(field));
            }
            {
                let cancel_visible = Rc::clone(&visible_for_build);
                let commit_visible = Rc::clone(&visible_for_build);
                let commit = Rc::clone(&commit);
                let buffer = Rc::clone(&name);
                let buttons = FlexRow::new()
                    .with_gap(8.0)
                    .add_flex(Box::new(crate::ui::hspacer()), 1.0)
                    // The alert's cancel role: Esc.
                    .add(Box::new(
                        Button::new("Cancel", Arc::clone(&fonts.regular))
                            .with_subtle().with_active_fn(|| false)
                            .with_cancel_action()
                            .on_click(move || {
                                cancel_visible.set(false);
                                agg_gui::animation::request_draw();
                            }),
                    ))
                    // The alert's default button: Return (the focused text
                    // field commits through `on_enter` first, so no double
                    // fire).
                    .add(Box::new(
                        Button::new(commit_label, Arc::clone(&fonts.regular))
                            .with_default_action()
                            .on_click(move || {
                                let text = buffer.borrow().trim().to_string();
                                if !text.is_empty() {
                                    commit(&text);
                                }
                                commit_visible.set(false);
                                agg_gui::animation::request_draw();
                            }),
                    ));
                column = column.add(Box::new(buttons));
            }
            Box::new(column)
        },
    );

    Box::new(ModalSheet::new(Rc::clone(visible), Box::new(content)).with_panel_size(DIALOG_SIZE))
}
