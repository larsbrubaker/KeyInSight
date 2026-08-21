//! The per-player Profile sheet — `UI/ProfileSheet.swift`: the beginner
//! scaffolds, each an explicit on/off. These are training wheels — OQ-24's
//! plan is for them to fade automatically with mastery; until then the
//! player holds the switch.

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::geometry::Size;
use agg_gui::widget::Widget;
use agg_gui::widgets::{
    Button, FlexColumn, FlexRow, Label, ModalSheet, Separator, Spacer, ToggleSwitch,
};

use crate::ui::fonts::{icon, size, UiFonts};
use crate::ui::side_panel::{engine_state_cell, SidePanelCells};
use crate::ui::{toggle_row, DynamicLabel};

use super::Engine;

/// The Swift `.frame(width: 440, height: 320)`.
const SHEET_SIZE: Size = Size {
    width: 440.0,
    height: 320.0,
};

pub(crate) const FOLLOW_OCTAVE_CAPTION: &str = "Start an exercise an octave off and it follows you — pitch reading without the hand jump. Turn off to require the written octave.";
pub(crate) const KEYS_DEFAULT_CAPTION: &str = "A keyboard strip highlighting the next key(s) to play. The Keys button still overrides this per piece or for training.";
pub(crate) const FOOTER_CAPTION: &str = "Helpers like these are meant to be outgrown — a future version will fade them automatically as items master.";

/// `engine.currentUser?.name ?? "Player"`.
pub(crate) fn header_title(engine: &crate::engine::SessionEngine) -> String {
    engine
        .current_user()
        .map(|user| user.name.clone())
        .unwrap_or_else(|| "Player".to_string())
}

pub fn build_profile_sheet(
    engine: &Engine,
    fonts: &UiFonts,
    cells: &SidePanelCells,
) -> Box<dyn Widget> {
    let visible = Rc::clone(&cells.show_profile);
    let mut column = FlexColumn::new().with_gap(0.0);

    // Header: Label(name, person.crop.circle) title3 bold + Done.
    {
        let name_engine = Rc::clone(engine);
        let close = Rc::clone(&visible);
        let header = FlexRow::new()
            .with_gap(8.0)
            .with_padding(14.0)
            .add(Box::new(
                Label::new(icon::USER_CIRCLE.to_string(), Arc::clone(&fonts.icons))
                    .with_font_size(size::TITLE3),
            ))
            .add(Box::new(
                DynamicLabel::new(
                    move || header_title(&name_engine.borrow()),
                    Arc::clone(&fonts.bold),
                )
                .with_font_size(size::TITLE3),
            ))
            .add_flex(Box::new(crate::ui::hspacer()), 1.0)
            .add(Box::new(
                Button::new("Done", Arc::clone(&fonts.regular))
                    .with_subtle()
                    .with_active_fn(|| false)
                    .on_click(move || {
                        close.set(false);
                        agg_gui::animation::request_draw();
                    }),
            ));
        column = column.add(Box::new(header));
    }
    column = column.add(Box::new(Separator::horizontal().with_line_inset(0.0)));

    // Body: VStack(alignment: .leading, spacing: 14), padded.
    let mut body = FlexColumn::new().with_gap(14.0).with_padding(14.0);
    body = body.add(Box::new(
        Label::new("Helpers", Arc::clone(&fonts.bold)).with_font_size(size::BODY),
    ));
    {
        let state = engine_state_cell(engine, |e| e.follow_octave());
        let click = Rc::clone(engine);
        let toggle = ToggleSwitch::new(engine.borrow().follow_octave())
            .with_state_cell(state)
            .on_change(move |on| click.borrow_mut().set_follow_octave(on));
        body = body.add(Box::new(helper_block(
            fonts,
            toggle_row("Follow my octave", fonts, toggle),
            FOLLOW_OCTAVE_CAPTION,
        )));
    }
    {
        let state = engine_state_cell(engine, |e| e.keys_user_default());
        let click = Rc::clone(engine);
        let toggle = ToggleSwitch::new(engine.borrow().keys_user_default())
            .with_state_cell(state)
            .on_change(move |on| click.borrow_mut().set_keys_user_default(on));
        body = body.add(Box::new(helper_block(
            fonts,
            toggle_row("Show keys by default", fonts, toggle),
            KEYS_DEFAULT_CAPTION,
        )));
    }
    // `.font(.caption).foregroundStyle(.tertiary)` — agg-gui has one
    // secondary shade; the smaller size carries the tertiary weight.
    body = body.add(Box::new(
        Label::new(FOOTER_CAPTION, Arc::clone(&fonts.regular))
            .with_font_size(size::CAPTION)
            .with_dim(true)
            .with_wrap(true),
    ));
    column = column.add(Box::new(body));
    column = column.add_flex(Box::new(Spacer::new()), 1.0);

    Box::new(ModalSheet::new(visible, Box::new(column)).with_panel_size(SHEET_SIZE))
}

/// `VStack(alignment: .leading, spacing: 3) { Toggle; caption }`.
fn helper_block(fonts: &UiFonts, toggle: FlexRow, caption: &str) -> FlexColumn {
    FlexColumn::new()
        .with_gap(3.0)
        .add(Box::new(toggle))
        .add(Box::new(
            Label::new(caption, Arc::clone(&fonts.regular))
                .with_font_size(size::CALLOUT)
                .with_dim(true)
                .with_wrap(true),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::side_panel::test_engine;

    #[test]
    fn header_names_the_current_player_or_falls_back() {
        let mut engine = test_engine();
        let current = engine.current_user().map(|u| u.name.clone());
        match current {
            Some(name) => assert_eq!(header_title(&engine), name),
            None => assert_eq!(header_title(&engine), "Player"),
        }
        engine.add_user("Ada");
        assert_eq!(header_title(&engine), "Ada");
    }
}
