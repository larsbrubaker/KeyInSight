//! Persistence: the storage trait the platform shells implement, and the
//! [`AppDatabase`] port.
//!
//! The Swift app used SQLite via GRDB. Per `docs/platform-substitutions.md`
//! the port keeps the **schema semantics** (users, sessions, exercises, the
//! append-only event log, EWMA item stats, progression, settings, piece
//! plays — all scoped per user) but stores them as one serde-serialized
//! document behind the [`Storage`] trait: native = file-backed, WASM =
//! localStorage, tests = in-memory.

#[cfg(test)]
mod tests;

mod app_database;
mod storage;

pub use app_database::{AppDatabase, ExerciseRecord, PitchItemStat, UserProfile};
pub use storage::{MemoryStorage, Storage};
