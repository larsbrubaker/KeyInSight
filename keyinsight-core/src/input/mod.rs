//! Input backends that live in the core (no platform APIs): the simulated
//! computer-keyboard backend and the Unplugged (self-verified) backend.
//! The MIDI and microphone backends are platform implementations of
//! [`crate::core::InputBackend`] provided by the shells
//! (see `docs/platform-substitutions.md`).
//!
//! Ports `Sources/KeyInSight/Input/`.

mod mic;
mod simulated;
mod unplugged;

pub use mic::{MicBackend, MicSource};
pub use simulated::SimulatedKeyboardBackend;
pub use unplugged::UnpluggedBackend;
