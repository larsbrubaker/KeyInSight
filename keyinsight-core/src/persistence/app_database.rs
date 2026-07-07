//! Port of `Persistence/AppDatabase.swift`: identical API surface and
//! semantics (per-user scoping, EWMA math, upserts, "last active user is
//! the default"), stored as one serde document behind [`Storage`] instead
//! of SQLite tables. Timestamps are Unix milliseconds supplied by callers
//! (the Swift methods took `Date` parameters the same way).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::persistence::Storage;

#[derive(Debug, Clone, PartialEq)]
pub struct PitchItemStat {
    pub item: String,
    pub attempts: i64,
    pub errors: i64,
    /// EWMA of the error indicator (0/1 per attempt) — the mastery input.
    pub ewma_error: f64,
    /// EWMA of response latency on correct plays, ms.
    pub ewma_latency_ms: Option<f64>,
    pub last_seen_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProfile {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExerciseRecord {
    pub id: i64,
    pub started_at_ms: i64,
    pub note_count: i64,
    pub error_count: i64,
    pub spec_json: String,
}

// --- The stored document (the "tables") ---

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserRow {
    id: i64,
    name: String,
    created_at_ms: i64,
    last_active_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionRow {
    id: i64,
    user_id: i64,
    started_at_ms: i64,
    ended_at_ms: Option<i64>,
    input_backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExerciseRow {
    id: i64,
    session_id: i64,
    seq: i64,
    spec_json: String,
    started_at_ms: i64,
    completed_at_ms: Option<i64>,
    note_count: Option<i64>,
    error_count: Option<i64>,
    descriptors_json: Option<String>,
    targeted_items: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NoteEventRow {
    id: i64,
    exercise_id: i64,
    at_ms: i64,
    kind: String,
    midi: i64,
    velocity: Option<i64>,
    classification: String,
    expected_index: Option<i64>,
    offset_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatRow {
    user_id: i64,
    item: String,
    attempts: i64,
    errors: i64,
    ewma_error: f64,
    ewma_latency_ms: Option<f64>,
    last_seen_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PiecePlayRow {
    user_id: i64,
    slug: String,
    title: String,
    at_ms: i64,
    mode: String,
    note_count: i64,
    error_count: i64,
    accuracy: f64,
    errors_by_measure_json: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Document {
    next_id: i64,
    users: Vec<UserRow>,
    sessions: Vec<SessionRow>,
    exercises: Vec<ExerciseRow>,
    note_events: Vec<NoteEventRow>,
    stats: Vec<StatRow>,
    /// user_id → unlocked count.
    progression: BTreeMap<i64, i64>,
    /// (user_id, key) → value, keyed as "user_id\u{1F}key".
    settings: BTreeMap<String, String>,
    piece_plays: Vec<PiecePlayRow>,
}

impl Document {
    fn allocate_id(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }

    fn setting_key(user_id: i64, key: &str) -> String {
        format!("{user_id}\u{1F}{key}")
    }
}

pub struct AppDatabase {
    storage: Box<dyn Storage>,
    document: Document,
    active_user_id: i64,
}

impl AppDatabase {
    /// EWMA smoothing factor for item stats.
    pub const EWMA_ALPHA: f64 = 0.25;

    /// Open over any storage backend; creates the default profile on first
    /// run (the v5 migration's "Player 1"). The most recently active
    /// profile becomes the default, matching the Swift init.
    pub fn open(storage: Box<dyn Storage>, now_ms: i64) -> Self {
        let mut document: Document = storage
            .load()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default();
        if document.users.is_empty() {
            let id = document.allocate_id();
            document.users.push(UserRow {
                id,
                name: "Player 1".to_string(),
                created_at_ms: now_ms,
                last_active_at_ms: now_ms,
            });
        }
        // Most recently active, ties broken by lowest id (Swift's
        // `ORDER BY last_active_at DESC, id ASC LIMIT 1`).
        let active_user_id = document
            .users
            .iter()
            .max_by(|a, b| {
                a.last_active_at_ms
                    .cmp(&b.last_active_at_ms)
                    .then(b.id.cmp(&a.id))
            })
            .map(|u| u.id)
            .unwrap_or(1);
        let mut db = Self {
            storage,
            document,
            active_user_id,
        };
        db.persist();
        db
    }

    /// In-memory database (tests) — `AppDatabase.inMemory()`.
    pub fn in_memory(now_ms: i64) -> Self {
        Self::open(Box::new(crate::persistence::MemoryStorage::new()), now_ms)
    }

    pub fn active_user_id(&self) -> i64 {
        self.active_user_id
    }

    fn persist(&mut self) {
        let contents = self.serialize_document();
        self.storage.save(&contents);
    }

    /// The full stored document as JSON (what `persist` writes) — exposed
    /// for shells that snapshot/export state and for reopen tests.
    pub fn serialize_document(&self) -> String {
        serde_json::to_string(&self.document).expect("document serializes")
    }

    // --- Users ---

    pub fn users(&self) -> Vec<UserProfile> {
        let mut users: Vec<&UserRow> = self.document.users.iter().collect();
        users.sort_by(|a, b| a.name.cmp(&b.name));
        users
            .into_iter()
            .map(|u| UserProfile {
                id: u.id,
                name: u.name.clone(),
            })
            .collect()
    }

    pub fn create_user(&mut self, name: &str, now_ms: i64) -> i64 {
        let id = self.document.allocate_id();
        self.document.users.push(UserRow {
            id,
            name: name.to_string(),
            created_at_ms: now_ms,
            last_active_at_ms: now_ms,
        });
        self.persist();
        id
    }

    pub fn rename_user(&mut self, id: i64, name: &str) {
        if let Some(user) = self.document.users.iter_mut().find(|u| u.id == id) {
            user.name = name.to_string();
        }
        self.persist();
    }

    /// Makes `id` the active (and therefore next-launch default) user.
    pub fn activate_user(&mut self, id: i64, now_ms: i64) {
        if let Some(user) = self.document.users.iter_mut().find(|u| u.id == id) {
            user.last_active_at_ms = now_ms;
        }
        self.active_user_id = id;
        self.persist();
    }

    /// The active user's most recent completed exercises, newest first.
    /// Single-note drill cards are excluded — they aren't worth revisiting.
    pub fn recent_exercises(&self, limit: usize) -> Vec<ExerciseRecord> {
        let session_ids: Vec<i64> = self
            .document
            .sessions
            .iter()
            .filter(|s| s.user_id == self.active_user_id)
            .map(|s| s.id)
            .collect();
        let mut records: Vec<&ExerciseRow> = self
            .document
            .exercises
            .iter()
            .filter(|e| {
                session_ids.contains(&e.session_id)
                    && e.completed_at_ms.is_some()
                    && e.note_count.unwrap_or(0) >= 2
            })
            .collect();
        records.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms));
        records
            .into_iter()
            .take(limit)
            .map(|e| ExerciseRecord {
                id: e.id,
                started_at_ms: e.started_at_ms,
                note_count: e.note_count.unwrap_or(0),
                error_count: e.error_count.unwrap_or(0),
                spec_json: e.spec_json.clone(),
            })
            .collect()
    }

    /// Completed exercises across all of the active user's sessions — the
    /// persistent "Exercise N" counter.
    pub fn lifetime_completed_exercise_count(&self) -> i64 {
        let session_ids: Vec<i64> = self
            .document
            .sessions
            .iter()
            .filter(|s| s.user_id == self.active_user_id)
            .map(|s| s.id)
            .collect();
        self.document
            .exercises
            .iter()
            .filter(|e| session_ids.contains(&e.session_id) && e.completed_at_ms.is_some())
            .count() as i64
    }

    // --- Sessions & exercises ---

    pub fn create_session(&mut self, started_at_ms: i64, input_backend: &str) -> i64 {
        let id = self.document.allocate_id();
        self.document.sessions.push(SessionRow {
            id,
            user_id: self.active_user_id,
            started_at_ms,
            ended_at_ms: None,
            input_backend: input_backend.to_string(),
        });
        // Session start keeps the "last user" default current.
        let active = self.active_user_id;
        if let Some(user) = self.document.users.iter_mut().find(|u| u.id == active) {
            user.last_active_at_ms = started_at_ms;
        }
        self.persist();
        id
    }

    pub fn end_session(&mut self, id: i64, now_ms: i64) {
        if let Some(session) = self.document.sessions.iter_mut().find(|s| s.id == id) {
            session.ended_at_ms = Some(now_ms);
        }
        self.persist();
    }

    pub fn create_exercise(
        &mut self,
        session_id: i64,
        seq: i64,
        spec_json: &str,
        started_at_ms: i64,
        descriptors_json: Option<String>,
        targeted_items: Option<String>,
    ) -> i64 {
        let id = self.document.allocate_id();
        self.document.exercises.push(ExerciseRow {
            id,
            session_id,
            seq,
            spec_json: spec_json.to_string(),
            started_at_ms,
            completed_at_ms: None,
            note_count: None,
            error_count: None,
            descriptors_json,
            targeted_items,
        });
        self.persist();
        id
    }

    // --- Progression ---

    pub fn unlocked_item_count(&self) -> Option<i64> {
        self.document.progression.get(&self.active_user_id).copied()
    }

    pub fn set_unlocked_item_count(&mut self, count: i64, _now_ms: i64) {
        self.document.progression.insert(self.active_user_id, count);
        self.persist();
    }

    pub fn complete_exercise(&mut self, id: i64, now_ms: i64, note_count: i64, error_count: i64) {
        if let Some(exercise) = self.document.exercises.iter_mut().find(|e| e.id == id) {
            exercise.completed_at_ms = Some(now_ms);
            exercise.note_count = Some(note_count);
            exercise.error_count = Some(error_count);
        }
        self.persist();
    }

    // --- Event log & item stats ---

    #[allow(clippy::too_many_arguments)]
    pub fn log_event(
        &mut self,
        exercise_id: i64,
        at_ms: i64,
        kind: &str,
        midi: i64,
        velocity: Option<i64>,
        classification: &str,
        expected_index: Option<i64>,
        offset_ms: Option<f64>,
    ) {
        let id = self.document.allocate_id();
        self.document.note_events.push(NoteEventRow {
            id,
            exercise_id,
            at_ms,
            kind: kind.to_string(),
            midi,
            velocity,
            classification: classification.to_string(),
            expected_index,
            offset_ms,
        });
        self.persist();
    }

    // --- Repertoire ---

    #[allow(clippy::too_many_arguments)]
    pub fn record_piece_play(
        &mut self,
        slug: &str,
        title: &str,
        mode: &str,
        note_count: i64,
        error_count: i64,
        accuracy: f64,
        errors_by_measure_json: &str,
        at_ms: i64,
    ) {
        self.document.piece_plays.push(PiecePlayRow {
            user_id: self.active_user_id,
            slug: slug.to_string(),
            title: title.to_string(),
            at_ms,
            mode: mode.to_string(),
            note_count,
            error_count,
            accuracy,
            errors_by_measure_json: errors_by_measure_json.to_string(),
        });
        self.persist();
    }

    pub fn piece_stats(&self, slug: &str) -> Option<(i64, f64)> {
        let plays: Vec<&PiecePlayRow> = self
            .document
            .piece_plays
            .iter()
            .filter(|p| p.slug == slug && p.user_id == self.active_user_id)
            .collect();
        if plays.is_empty() {
            return None;
        }
        let best = plays.iter().map(|p| p.accuracy).fold(f64::MIN, f64::max);
        Some((plays.len() as i64, best))
    }

    // --- Settings ---

    pub fn setting(&self, key: &str) -> Option<String> {
        self.document
            .settings
            .get(&Document::setting_key(self.active_user_id, key))
            .cloned()
    }

    pub fn set_setting(&mut self, key: &str, value: &str, _now_ms: i64) {
        self.document
            .settings
            .insert(Document::setting_key(self.active_user_id, key), value.to_string());
        self.persist();
    }

    /// One attempt = one expected note resolved (correctly played, possibly
    /// after errors). `was_error` is "any wrong note before the correct one".
    pub fn record_item_attempt(
        &mut self,
        item: &str,
        was_error: bool,
        latency_ms: Option<f64>,
        now_ms: i64,
    ) {
        let error_value = if was_error { 1.0 } else { 0.0 };
        let alpha = Self::EWMA_ALPHA;
        let active = self.active_user_id;
        if let Some(row) = self
            .document
            .stats
            .iter_mut()
            .find(|s| s.item == item && s.user_id == active)
        {
            row.ewma_error = alpha * error_value + (1.0 - alpha) * row.ewma_error;
            if let Some(latency_ms) = latency_ms {
                row.ewma_latency_ms = Some(match row.ewma_latency_ms {
                    Some(previous) => alpha * latency_ms + (1.0 - alpha) * previous,
                    None => latency_ms,
                });
            }
            row.attempts += 1;
            row.errors += if was_error { 1 } else { 0 };
            row.last_seen_at_ms = now_ms;
        } else {
            self.document.stats.push(StatRow {
                user_id: active,
                item: item.to_string(),
                attempts: 1,
                errors: if was_error { 1 } else { 0 },
                ewma_error: error_value,
                ewma_latency_ms: latency_ms,
                last_seen_at_ms: now_ms,
            });
        }
        self.persist();
    }

    pub fn item_stats(&self) -> Vec<PitchItemStat> {
        let mut stats: Vec<&StatRow> = self
            .document
            .stats
            .iter()
            .filter(|s| s.user_id == self.active_user_id)
            .collect();
        stats.sort_by(|a, b| a.item.cmp(&b.item));
        stats
            .into_iter()
            .map(|s| PitchItemStat {
                item: s.item.clone(),
                attempts: s.attempts,
                errors: s.errors,
                ewma_error: s.ewma_error,
                ewma_latency_ms: s.ewma_latency_ms,
                last_seen_at_ms: s.last_seen_at_ms,
            })
            .collect()
    }

    pub fn event_count(&self, exercise_id: i64) -> i64 {
        self.document
            .note_events
            .iter()
            .filter(|e| e.exercise_id == exercise_id)
            .count() as i64
    }
}
