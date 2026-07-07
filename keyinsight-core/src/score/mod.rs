//! The score layer: internal notation model, difficulty descriptors, the
//! adaptive exercise generator, free-play mirror scores, MusicXML
//! import/export, and the bundled repertoire library.
//!
//! Ports `Sources/KeyInSight/Score/` from the Swift reference.

#[cfg(test)]
mod tests;

mod difficulty;
mod free_play;
mod generator;
mod model;
mod musicxml_encoder;
mod musicxml_importer;
mod repertoire;

pub use difficulty::DifficultyDescriptors;
pub use free_play::FreePlayScore;
pub use generator::{ExerciseGenerator, GeneratorConfig, PitchOption};
pub use model::{Exercise, MatchEvent, NoteDuration, NoteSpan, ScoreNote, Staff, TieRole};
pub use musicxml_encoder::MusicXmlEncoder;
pub use musicxml_importer::{ImportError, ImportedPiece, MusicXmlImporter};
pub use repertoire::{RepertoireLibrary, RepertoirePiece};
