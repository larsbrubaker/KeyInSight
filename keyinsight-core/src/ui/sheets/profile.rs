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
                // `.keyboardShortcut(.cancelAction)`: Esc closes.
                Button::new("Done", Arc::clone(&fonts.regular))
                    .with_subtle()
                    .with_active_fn(|| false)
                    .with_cancel_action()
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

/// Layout regression tests: header, both helper blocks and the footer
/// caption all fit the 440×320 panel (the Library search-box failure
/// class, where a chrome row swells and shoves content off the sheet).
#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::ui::sheets::layout_test_support::{
        all_of_type, contains, describe, laid_out_nodes, node_with_property,
        node_with_property_prefix, nodes_outside_panel, panel_rect, shared_test_engine, WINDOW,
    };
    use crate::ui::side_panel::{open_cell, SidePanelCells};
    use agg_gui::geometry::Rect;
    use agg_gui::widget::InspectorNode;

    /// Build the sheet and open it exactly as the bar's profile button does.
    fn opened_sheet_nodes() -> Vec<InspectorNode> {
        let engine = shared_test_engine();
        let fonts = UiFonts::bundled();
        let cells = SidePanelCells::new();
        let mut sheet = build_profile_sheet(&engine, &fonts, &cells);
        open_cell(&cells.show_profile);
        laid_out_nodes(&mut sheet)
    }

    /// The Swift `.frame(width: 440, height: 320)`, centered.
    #[test]
    fn panel_is_the_swift_fixed_frame() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);
        assert_eq!(panel.width, SHEET_SIZE.width);
        assert_eq!(panel.height, SHEET_SIZE.height);
        assert!(
            contains(Rect::new(0.0, 0.0, WINDOW.width, WINDOW.height), panel),
            "panel {panel:?} must sit inside the window"
        );
    }

    /// Both helper switches and their labels are on the panel, each with
    /// a real size — the switches are the whole point of the sheet.
    #[test]
    fn both_helper_toggles_sit_inside_the_panel() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);
        let toggles = all_of_type(&nodes, "ToggleSwitch");
        assert_eq!(toggles.len(), 2, "one switch per helper");
        for toggle in toggles {
            let rect = toggle.screen_bounds;
            assert!(
                rect.width > 0.0 && rect.height > 0.0 && contains(panel, rect),
                "a helper switch {rect:?} must sit inside the panel {panel:?}"
            );
        }
        for text in ["Follow my octave", "Show keys by default"] {
            let label = node_with_property(&nodes, "text", text).screen_bounds;
            assert!(
                label.width > 0.0 && label.height > 0.0 && contains(panel, label),
                "{text:?} at {label:?} must sit inside the panel {panel:?}"
            );
        }
    }

    /// The wrapped captions and the fading-scaffolds footer stay on the
    /// panel below the switches.
    #[test]
    fn captions_and_footer_stay_on_the_panel() {
        let nodes = opened_sheet_nodes();
        let panel = panel_rect(&nodes);
        for caption in [FOLLOW_OCTAVE_CAPTION, KEYS_DEFAULT_CAPTION, FOOTER_CAPTION] {
            let head: String = caption.chars().take(24).collect();
            let rect = node_with_property_prefix(&nodes, "text", &head).screen_bounds;
            assert!(
                rect.width > 0.0 && rect.height > 0.0 && contains(panel, rect),
                "the caption {head:?} at {rect:?} must sit inside the panel {panel:?}"
            );
        }
    }

    /// Nothing may leave the panel (this sheet has no scrolling area).
    #[test]
    fn nothing_is_laid_out_off_the_panel() {
        let nodes = opened_sheet_nodes();
        let outside = nodes_outside_panel(&nodes);
        assert!(
            outside.is_empty(),
            "widgets laid out off the panel:\n{}",
            describe(&outside)
        );
    }
}
