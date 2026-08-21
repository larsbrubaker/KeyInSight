//! The footer grid — `footerButtons` in `UI/SidePanel.swift`: Library +
//! Drill, then Free Play + Survival (2×2, equal columns).

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widgets::{Button, FlexColumn, FlexRow};

use crate::engine::InputSource;
use crate::ui::fonts::{icon, UiFonts};

use super::cells::open_cell;
use super::{Engine, SidePanelCells};

pub(super) fn footer_buttons(engine: &Engine, fonts: &UiFonts, cells: &SidePanelCells) -> FlexColumn {
    let mut column = FlexColumn::new().with_gap(8.0);
    let mut row = FlexRow::new().with_gap(8.0);
    {
        let show_library = Rc::clone(&cells.show_library);
        let generation = Rc::clone(&cells.library_generation);
        row = row.add_flex(
            Box::new(
                Button::new("Library", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::BOOKS, Arc::clone(&fonts.icons))
                    .on_click(move || {
                        generation.set(generation.get() + 1);
                        open_cell(&show_library);
                    }),
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

    let mut row = FlexRow::new().with_gap(8.0);
    {
        let enabled = Rc::clone(engine);
        let click = Rc::clone(engine);
        row = row.add_flex(
            Box::new(
                Button::new("Free Play", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::KEYBOARD, Arc::clone(&fonts.icons))
                    .with_enabled_fn(move || enabled.borrow().input_source().supports_timing())
                    .on_click(move || click.borrow_mut().enter_free_play()),
            ),
            1.0,
        );
    }
    // Survival needs detected input (help "Endless reading, 3 lives —
    // beat your best score").
    {
        let enabled = Rc::clone(engine);
        let click = Rc::clone(engine);
        row = row.add_flex(
            Box::new(
                Button::new("Survival", Arc::clone(&fonts.regular))
                    .with_subtle().with_active_fn(|| false)
                    .with_icon(icon::FLAG_CHECKERED, Arc::clone(&fonts.icons))
                    .with_enabled_fn(move || {
                        enabled.borrow().input_source() != InputSource::SelfVerify
                    })
                    .on_click(move || click.borrow_mut().enter_survival()),
            ),
            1.0,
        );
    }
    column.add(Box::new(row))
}
