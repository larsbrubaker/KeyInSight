//! Input backends. The simulated computer-keyboard backend and the
//! Unplugged (self-verified) backend are pure core; the MIDI and
//! microphone backends live here too but pull their packets / samples
//! from the platform shells through the [`MidiPorts`] / [`MicSource`]
//! seams (see `docs/platform-substitutions.md`).
//!
//! Ports `Sources/KeyInSight/Input/`.

mod mic;
mod midi;
mod simulated;
mod unplugged;

pub use mic::{MicBackend, MicSource};
pub use midi::{MidiBackend, MidiParser, MidiPorts};
pub use simulated::SimulatedKeyboardBackend;
pub use unplugged::UnpluggedBackend;
