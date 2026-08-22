//! `label [switch]` — the macOS `.toggleStyle(.switch)` row shared by the
//! settings-style sheets: the title leads and the switch trails, hugging
//! the title (AppKit's `NSSwitch` sits after its label; outside a `Form`
//! nothing pushes it to the far edge).

use std::sync::Arc;

use agg_gui::widgets::{FlexRow, Label, ToggleSwitch};

use crate::ui::fonts::{size, UiFonts};

pub fn toggle_row(label: &str, fonts: &UiFonts, toggle: ToggleSwitch) -> FlexRow {
    FlexRow::new()
        .with_gap(8.0)
        .add(Box::new(
            Label::new(label, Arc::clone(&fonts.regular)).with_font_size(size::BODY),
        ))
        .add(Box::new(toggle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agg_gui::widget::Widget;

    #[test]
    fn label_leads_and_the_switch_trails() {
        let fonts = UiFonts::bundled();
        let row = toggle_row("Follow my octave", &fonts, ToggleSwitch::new(true));
        let kinds: Vec<&str> = row.children().iter().map(|c| c.type_name()).collect();
        assert_eq!(kinds, ["Label", "ToggleSwitch"]);
    }
}
