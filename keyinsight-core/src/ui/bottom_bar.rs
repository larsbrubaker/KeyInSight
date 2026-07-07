//! Window-wide bottom bar: who is playing, plus session-level navigation
//! (Keys, Resume Training, Progress).
//!
//! Ports `UI/BottomBar.swift`. The player picker maps to a cycle button +
//! add button (macOS's alert-with-textfield rename flow arrives with the
//! text-input overlay work in Phase 2).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::text::Font;
use agg_gui::widget::Widget;
use agg_gui::widgets::{Button, FlexRow, Spacer};

use crate::engine::SessionEngine;
use crate::ui::side_panel::SidePanelCells;
use crate::ui::DynamicLabel;

type Engine = Rc<RefCell<SessionEngine>>;

/// Bar height (the Swift bar's padding + control height).
pub const BAR_HEIGHT: f64 = 44.0;

pub fn build_bottom_bar(
    engine: &Engine,
    font: &Arc<Font>,
    cells: &SidePanelCells,
) -> Box<dyn Widget> {
    let mut row = FlexRow::new()
        .with_gap(10.0)
        .with_padding(10.0)
        // Fixed bar height: without the cap a container child of a
        // FlexColumn expands to the full available height.
        .with_min_size(agg_gui::geometry::Size::new(0.0, BAR_HEIGHT))
        .with_max_size(agg_gui::geometry::Size::new(f64::INFINITY, BAR_HEIGHT));

    // Player readout + switch (cycles through profiles) + add.
    {
        let engine = Rc::clone(engine);
        row = row.add(Box::new(
            DynamicLabel::new(
                move || {
                    let engine = engine.borrow();
                    format!(
                        "Player: {}",
                        engine
                            .current_user()
                            .map(|u| u.name.as_str())
                            .unwrap_or("—")
                    )
                },
                Arc::clone(font),
            )
            .with_font_size(13.0),
        ));
    }
    {
        let click = Rc::clone(engine);
        row = row.add(Box::new(
            Button::new("Switch", Arc::clone(font))
                .with_compact()
                .on_click(move || {
                    let mut engine = click.borrow_mut();
                    let users: Vec<i64> = engine.users().iter().map(|u| u.id).collect();
                    if users.len() < 2 {
                        return;
                    }
                    let current = engine.current_user().map(|u| u.id).unwrap_or(users[0]);
                    let index = users.iter().position(|&id| id == current).unwrap_or(0);
                    engine.switch_user(users[(index + 1) % users.len()]);
                }),
        ));
    }
    {
        let click = Rc::clone(engine);
        row = row.add(Box::new(
            Button::new("+ Player", Arc::clone(font))
                .with_compact()
                .on_click(move || {
                    let mut engine = click.borrow_mut();
                    let name = format!("Player {}", engine.users().len() + 1);
                    engine.add_user(&name);
                }),
        ));
    }

    row = row.add_flex(Box::new(Spacer::new()), 1.0);

    // Keys toggle for the current context.
    {
        let active = Rc::clone(engine);
        let click = Rc::clone(engine);
        row = row.add(Box::new(
            Button::new("Keys", Arc::clone(font))
                .with_subtle()
                .with_compact()
                .with_active_fn(move || active.borrow().show_keys())
                .on_click(move || {
                    let mut engine = click.borrow_mut();
                    if !engine.is_free_play() {
                        engine.toggle_keys_for_context();
                    }
                }),
        ));
    }
    // Resume Training (only when diverted).
    {
        let diverted = crate::ui::side_panel::diverted_cell(engine);
        let click = Rc::clone(engine);
        row = row.add(Box::new(agg_gui::widgets::Conditional::new(
            diverted,
            Box::new(
                Button::new("Resume Training", Arc::clone(font))
                    .with_compact()
                    .on_click(move || click.borrow_mut().resume_training()),
            ),
        )));
    }
    // Progress sheet.
    {
        let show_progress = Rc::clone(&cells.show_progress);
        row = row.add(Box::new(
            Button::new("Progress", Arc::clone(font))
                .with_compact()
                .on_click(move || {
                    show_progress.set(true);
                    agg_gui::animation::request_draw();
                }),
        ));
    }

    Box::new(row)
}
