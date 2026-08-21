//! The user interface layer: agg-gui widgets mirroring the SwiftUI views.
//!
//! Ports `Sources/KeyInSight/UI/`: the training root (`app.rs`), side
//! panel, bottom bar, piano strip, and the Library / Progress /
//! Calibration sheets plus the player dialogs (`sheets/`). Typography
//! and icons come from the bundled faces in [`fonts`].

pub(crate) mod app;
pub(crate) mod bottom_bar;
mod dynamic_label;
pub(crate) mod fonts;
mod info_rows;
mod keyboard_layout;
mod level_meter;
pub(crate) mod palette;
mod piano_strip;
pub(crate) mod sheets;
pub(crate) mod side_panel;
mod toggle_row;

pub use app::{build_keyinsight_app, KeyInSightHandles, KeyInSightPlatform};
pub use dynamic_label::DynamicLabel;
pub use fonts::UiFonts;
pub use info_rows::{InfoRow, InfoRows, RowStyle};
pub use keyboard_layout::{KeyboardKey, KeyboardLayout};
pub use level_meter::LevelMeter;
pub use piano_strip::PianoStripWidget;
pub use toggle_row::toggle_row;

/// A flexible spacer for `FlexRow`s (the SwiftUI `Spacer()` in an
/// `HStack`). A plain [`Spacer`](agg_gui::widgets::Spacer) claims all the
/// available *height* too, blowing up rows laid out inside tall parents —
/// the height cap keeps it a purely horizontal spring.
pub(crate) fn hspacer() -> agg_gui::widgets::Spacer {
    agg_gui::widgets::Spacer::new()
        .with_max_size(agg_gui::geometry::Size::new(f64::INFINITY, 1.0))
}

/// Median of a sample list — `CalibrationSheet.median` in Swift (the
/// calibration flow's latency estimator, used by `sheets/calibration`).
pub fn median(values: &[f64]) -> f64 {
    assert!(!values.is_empty(), "median needs at least one sample");
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite samples"));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

#[cfg(test)]
mod tests {
    /// From TimelineTests in `TempoTests.swift` (`medianCalculation`).
    #[test]
    fn median_calculation() {
        assert_eq!(super::median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(super::median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
        assert_eq!(super::median(&[10.0]), 10.0);
    }
}
