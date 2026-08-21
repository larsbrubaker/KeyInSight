//! `[switch] label` — the macOS `.toggleStyle(.switch)` row shared by the
//! settings-style sheets (the side panel used it for its per-user toggles
//! before they moved to the Profile sheet).

use std::sync::Arc;

use agg_gui::widgets::{FlexRow, Label, ToggleSwitch};

use crate::ui::fonts::{size, UiFonts};

pub fn toggle_row(label: &str, fonts: &UiFonts, toggle: ToggleSwitch) -> FlexRow {
    FlexRow::new()
        .with_gap(6.0)
        .add(Box::new(toggle))
        .add(Box::new(
            Label::new(label, Arc::clone(&fonts.regular)).with_font_size(size::BODY),
        ))
}
