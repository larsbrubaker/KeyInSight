//! Foundation types shared by every layer: the normalized [`NoteEvent`]
//! input seam, pitch-spelling helpers, and the deterministic RNG.
//!
//! Ports `Sources/KeyInSight/Core/` from the Swift reference.

mod note_event;
mod pitch_spelling;
mod split_mix64;

pub use note_event::{InputBackend, NoteEvent, NoteEventKind};
pub use pitch_spelling::PitchSpelling;
pub use split_mix64::{Rng64, SplitMix64};
