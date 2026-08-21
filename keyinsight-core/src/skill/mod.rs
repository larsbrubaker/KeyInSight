//! The adaptive skill model.
//!
//! Ports `Sources/KeyInSight/Skill/`.

#[cfg(test)]
mod tests;

mod ladder;
mod skill_model;

pub use ladder::{
    ChordThresholds, IntervalThresholds, BASE_INTERVAL_SIZES, CHORD_SHAPE_LADDER,
    INTERVAL_SIZE_LADDER, TRANSITION_BIAS_ATTEMPTS_FLOOR,
};
pub use skill_model::{
    IntervalState, ItemState, KeyOption, SkillModel, Thresholds, BASS_UNLOCK_ORDER,
    INTERVAL_DELTAS, SEED_COUNT, UNLOCK_ORDER,
};
