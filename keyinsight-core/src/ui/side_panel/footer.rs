//! The footer grid — `footerButtons` in `UI/SidePanel.swift`: Library +
//! Drill, then Free Play + Survival (2×2, equal columns). Every
//! `panelButton` label is `.frame(maxWidth: .infinity)`: the button fills
//! its grid cell with the label centered.

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::layout_props::HAnchor;
use agg_gui::widgets::{Button, FlexColumn, FlexRow, Tooltip};

use crate::engine::InputSource;
use crate::ui::fonts::{icon, UiFonts};
use crate::ui::help;

use super::cells::open_cell;
use super::{Engine, SidePanelCells};

/// `panelButton(title, icon)`: a bordered button spanning its cell.
fn panel_button(title: &str, glyph: char, fonts: &UiFonts) -> Button {
    Button::new(title, Arc::clone(&fonts.regular))
        .with_subtle()
        .with_active_fn(|| false)
        .with_icon(glyph, Arc::clone(&fonts.icons))
        .with_h_anchor(HAnchor::STRETCH)
}

pub(super) fn footer_buttons(engine: &Engine, fonts: &UiFonts, cells: &SidePanelCells) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(8.0);
    let mut row = FlexRow::new().with_gap(8.0);
    {
        let show_library = Rc::clone(&cells.show_library);
        let generation = Rc::clone(&cells.library_generation);
        row = row.add_flex(
            Box::new(panel_button("Library", icon::BOOKS, fonts).on_click(move || {
                generation.set(generation.get() + 1);
                open_cell(&show_library);
            })),
            1.0,
        );
    }
    {
        let click = Rc::clone(engine);
        row = row.add_flex(
            Box::new(
                panel_button("Drill", icon::BOLT, fonts)
                    .on_click(move || click.borrow_mut().start_drill()),
            ),
            1.0,
        );
    }
    column = column.add(Box::new(row));

    let mut row = FlexRow::new().with_gap(8.0);
    {
        let enabled = Rc::clone(engine);
        let click = Rc::clone(engine);
        row = row.add_flex(
            Box::new(
                panel_button("Free Play", icon::KEYBOARD, fonts)
                    .with_enabled_fn(move || enabled.borrow().input_source().supports_timing())
                    .on_click(move || click.borrow_mut().enter_free_play()),
            ),
            1.0,
        );
    }
    // Survival needs detected input.
    {
        let enabled = Rc::clone(engine);
        let click = Rc::clone(engine);
        row = row.add_flex(
            Box::new(Tooltip::new(
                Box::new(
                    panel_button("Survival", icon::FLAG_CHECKERED, fonts)
                        .with_enabled_fn(move || {
                            enabled.borrow().input_source() != InputSource::SelfVerify
                        })
                        .on_click(move || click.borrow_mut().enter_survival()),
                ),
                help::survival(),
                Arc::clone(&fonts.regular),
            )),
            1.0,
        );
    }
    column.add(Box::new(row))
}
