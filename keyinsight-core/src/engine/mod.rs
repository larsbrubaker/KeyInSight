//! The training engine: matchers, the octave anchor, tempo/rhythm policies,
//! and (as the port progresses) the session engine and demo driver.
//!
//! Ports `Sources/KeyInSight/Engine/` from the Swift reference.

#[cfg(test)]
mod tests;

mod matcher;
mod octave_anchor;
mod session;
mod tempo_matcher;

pub use matcher::{SelfPacedMatcher, SelfPacedOutcome};
pub use octave_anchor::OctaveAnchor;
pub use session::{
    default_backend_factory, BackendFactory, ExerciseSummary, InputSource, IntervalEntry,
    PacingMode, Phase, ProgressEntry, SessionEngine, DRILL_LENGTH,
};
pub use tempo_matcher::{
    RhythmPolicy, TempoExpected, TempoMatcher, TempoOutcome, TempoPolicy, TempoReport,
    TempoResolution, Timing,
};
