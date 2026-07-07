//! The adaptive skill model.
//!
//! Ports `Sources/KeyInSight/Skill/`.

#[cfg(test)]
mod tests;

mod skill_model;

pub use skill_model::{
    IntervalState, ItemState, KeyOption, SkillModel, Thresholds, INTERVAL_DELTAS, SEED_COUNT,
    UNLOCK_ORDER,
};
