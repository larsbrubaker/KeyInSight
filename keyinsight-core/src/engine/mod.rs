//! The training engine: matchers, the octave anchor, tempo/rhythm policies,
//! the session engine, and the scripted demo driver.
//!
//! Ports `Sources/KeyInSight/Engine/` from the Swift reference.

#[cfg(test)]
mod tests;

pub mod demo;
mod input_storm;
mod matcher;
mod octave_anchor;
mod session;
mod tempo_matcher;

pub use demo::{headless_demo_engine, run_demo, DemoAudio, DemoDriver};
pub use input_storm::InputStormDetector;
pub use matcher::{SelfPacedMatcher, SelfPacedOutcome};
pub use octave_anchor::OctaveAnchor;
pub use session::{
    default_backend_factory, BackendFactory, ChordEntry, ExerciseSummary, HandMode, InputSource,
    IntervalEntry, PacingMode, Phase, ProgressEntry, SessionEngine, SurvivalReport,
    TempoDebug, TransitionEntry, DRILL_LENGTH, LATENCY_OUTLIER_MS,
};
pub use tempo_matcher::{
    RhythmPolicy, SurvivalPolicy, TempoExpected, TempoMatcher, TempoOutcome, TempoPolicy,
    TempoReport, TempoResolution, Timing,
};
