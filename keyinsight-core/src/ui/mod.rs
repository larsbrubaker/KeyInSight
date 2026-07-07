//! The user interface layer: agg-gui widgets mirroring the SwiftUI views.
//!
//! Ports `Sources/KeyInSight/UI/`: the training root (`app.rs`), side
//! panel, bottom bar, piano strip, and the Library/Progress sheets. The
//! CalibrationSheet flow (tempo latency measurement) arrives in Phase 2.

pub(crate) mod app;
pub(crate) mod bottom_bar;
mod dynamic_label;
mod keyboard_layout;
mod piano_strip;
pub(crate) mod sheets;
pub(crate) mod side_panel;

pub use app::{build_keyinsight_app, KeyInSightHandles, KeyInSightPlatform};
pub use dynamic_label::DynamicLabel;
pub use keyboard_layout::{KeyboardKey, KeyboardLayout};
pub use piano_strip::PianoStripWidget;

/// Median of a sample list — `CalibrationSheet.median` in Swift (the
/// calibration flow's latency estimator). Lives here until the full
/// CalibrationSheet widget is ported.
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
