//! Pitch-pair key shared by the skill model (which produces pair weights)
//! and the exercise generator (which consumes them). Lives in `core` so
//! `score` never depends on `skill`; `SkillModel::transition_key`
//! delegates here.

/// Packs an ordered `(from, to)` MIDI pair into one integer key:
/// `(from << 8) | to` — the same encoding Swift's
/// `SkillModel.transitionKey` uses.
pub fn transition_key(from: u8, to: u8) -> i32 {
    (from as i32) << 8 | to as i32
}
