//! Ports `Tests/KeyInSightTests/AppDatabaseTests.swift`.

use crate::persistence::{AppDatabase, MemoryStorage};

const NOW: i64 = 1_700_000_000_000;

fn in_memory() -> AppDatabase {
    AppDatabase::in_memory(NOW)
}

#[test]
fn session_exercise_event_round_trip() {
    let mut db = in_memory();
    let session_id = db.create_session(NOW, "test");
    let exercise_id = db.create_exercise(session_id, 1, "{}", NOW, None, None);

    db.log_event(exercise_id, NOW, "on", 60, Some(80), "correct", Some(0), None);
    db.log_event(exercise_id, NOW, "off", 60, None, "off", Some(1), None);
    assert_eq!(db.event_count(exercise_id), 2);

    db.complete_exercise(exercise_id, NOW, 8, 1);
    db.end_session(session_id, NOW);
}

#[test]
fn item_stat_ewma() {
    let mut db = in_memory();
    let alpha = AppDatabase::EWMA_ALPHA;

    db.record_item_attempt("treble:C4", false, Some(800.0), NOW);
    let stats = db.item_stats();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].attempts, 1);
    assert_eq!(stats[0].errors, 0);
    assert!(stats[0].ewma_error.abs() < 1e-9);
    assert!((stats[0].ewma_latency_ms.unwrap() - 800.0).abs() < 1e-9);

    db.record_item_attempt("treble:C4", true, None, NOW);
    let stats = db.item_stats();
    assert_eq!(stats[0].attempts, 2);
    assert_eq!(stats[0].errors, 1);
    assert!((stats[0].ewma_error - alpha).abs() < 1e-9);
    // Latency EWMA untouched by error attempts (no latency supplied).
    assert!((stats[0].ewma_latency_ms.unwrap() - 800.0).abs() < 1e-9);

    db.record_item_attempt("treble:C4", false, Some(400.0), NOW);
    let stats = db.item_stats();
    assert!((stats[0].ewma_error - (1.0 - alpha) * alpha).abs() < 1e-9);
    assert!(
        (stats[0].ewma_latency_ms.unwrap() - (alpha * 400.0 + (1.0 - alpha) * 800.0)).abs()
            < 1e-9
    );
}

#[test]
fn progression_round_trip() {
    let mut db = in_memory();
    assert_eq!(db.unlocked_item_count(), None);
    db.set_unlocked_item_count(6, NOW);
    assert_eq!(db.unlocked_item_count(), Some(6));
    // Upsert overwrites.
    db.set_unlocked_item_count(7, NOW);
    assert_eq!(db.unlocked_item_count(), Some(7));
}

#[test]
fn exercise_stores_descriptors_and_targets() {
    let mut db = in_memory();
    let session_id = db.create_session(NOW, "test");
    let exercise_id = db.create_exercise(
        session_id,
        1,
        "{}",
        NOW,
        Some(r#"{"rangeSemitones":7}"#.to_string()),
        Some(r#"["treble:C4"]"#.to_string()),
    );
    assert!(exercise_id > 0);
}

#[test]
fn distinct_items_tracked_separately() {
    let mut db = in_memory();
    db.record_item_attempt("treble:C4", false, Some(500.0), NOW);
    db.record_item_attempt("treble:G4", true, None, NOW);
    let stats = db.item_stats();
    let items: Vec<&str> = stats.iter().map(|s| s.item.as_str()).collect();
    assert_eq!(items, ["treble:C4", "treble:G4"]);
}

// --- Users ---

#[test]
fn fresh_database_has_default_user() {
    let db = in_memory();
    let users = db.users();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Player 1");
    assert_eq!(db.active_user_id(), users[0].id);
}

#[test]
fn progress_is_scoped_per_user() {
    let mut db = in_memory();
    db.record_item_attempt("treble:C4", true, Some(900.0), NOW);
    db.set_unlocked_item_count(8, NOW);
    db.set_setting("tempo_bpm", "72", NOW);

    let second = db.create_user("Kid", NOW);
    db.activate_user(second, NOW);
    // A fresh profile sees none of user 1's progress.
    assert!(db.item_stats().is_empty());
    assert_eq!(db.unlocked_item_count(), None);
    assert_eq!(db.setting("tempo_bpm"), None);

    db.record_item_attempt("treble:D4", false, Some(500.0), NOW);
    db.set_unlocked_item_count(5, NOW);

    // Switching back restores user 1's state untouched.
    let first = db
        .users()
        .into_iter()
        .find(|u| u.name == "Player 1")
        .unwrap();
    db.activate_user(first.id, NOW);
    let stats = db.item_stats();
    assert_eq!(
        stats.iter().map(|s| s.item.as_str()).collect::<Vec<_>>(),
        ["treble:C4"]
    );
    assert_eq!(stats[0].errors, 1);
    assert_eq!(db.unlocked_item_count(), Some(8));
    assert_eq!(db.setting("tempo_bpm"), Some("72".to_string()));
}

#[test]
fn last_active_user_is_default_on_reopen() {
    let mut db = in_memory();
    let second_id = db.create_user("Kid", NOW);
    db.activate_user(second_id, NOW + 60_000);
    let snapshot = db.serialize_document();

    // Reopen: the most recently active profile is the default.
    let reopened = AppDatabase::open(
        Box::new(MemoryStorage::with_contents(snapshot)),
        NOW + 120_000,
    );
    assert_eq!(reopened.active_user_id(), second_id);
}

#[test]
fn lifetime_exercise_count_persists_across_sessions_per_user() {
    let mut db = in_memory();
    let s1 = db.create_session(NOW, "test");
    let e1 = db.create_exercise(s1, 1, "{}", NOW, None, None);
    db.complete_exercise(e1, NOW, 8, 0);
    // Incomplete exercises don't count.
    let _ = db.create_exercise(s1, 2, "{}", NOW, None, None);
    db.end_session(s1, NOW);

    let s2 = db.create_session(NOW, "test");
    let e2 = db.create_exercise(s2, 1, "{}", NOW, None, None);
    db.complete_exercise(e2, NOW, 8, 1);
    assert_eq!(db.lifetime_completed_exercise_count(), 2);

    // Another user's count is independent.
    let second = db.create_user("Kid", NOW);
    db.activate_user(second, NOW);
    assert_eq!(db.lifetime_completed_exercise_count(), 0);
}

#[test]
fn rename_user_persists() {
    let mut db = in_memory();
    let user = db.users()[0].clone();
    db.rename_user(user.id, "Kevin");
    assert_eq!(
        db.users().iter().map(|u| u.name.as_str()).collect::<Vec<_>>(),
        ["Kevin"]
    );
}

#[test]
fn recent_exercises_are_scoped_completed_and_non_drill() {
    let mut db = in_memory();
    let session = db.create_session(NOW, "test");
    // Completed multi-note exercise: listed.
    let e1 = db.create_exercise(session, 1, r#"{"n":1}"#, NOW - 300_000, None, None);
    db.complete_exercise(e1, NOW, 12, 2);
    // Incomplete: not listed.
    let _ = db.create_exercise(session, 2, "{}", NOW, None, None);
    // Single-note drill card: not listed.
    let drill = db.create_exercise(session, 3, "{}", NOW, None, None);
    db.complete_exercise(drill, NOW, 1, 0);
    // Newer completed exercise: listed first.
    let e2 = db.create_exercise(session, 4, r#"{"n":2}"#, NOW, None, None);
    db.complete_exercise(e2, NOW, 8, 0);

    let records = db.recent_exercises(10);
    let specs: Vec<&str> = records.iter().map(|r| r.spec_json.as_str()).collect();
    assert_eq!(specs, [r#"{"n":2}"#, r#"{"n":1}"#]);
    assert_eq!(records[0].note_count, 8);
    assert_eq!(records[1].error_count, 2);

    // Another user sees none of it.
    let second = db.create_user("Kid", NOW);
    db.activate_user(second, NOW);
    assert!(db.recent_exercises(10).is_empty());
}

#[test]
fn piece_plays_are_scoped_per_user() {
    let mut db = in_memory();
    db.record_piece_play("ode", "Ode to Joy", "Self-paced", 30, 3, 0.9, "[]", NOW);
    assert_eq!(db.piece_stats("ode").map(|s| s.0), Some(1));

    let second = db.create_user("Kid", NOW);
    db.activate_user(second, NOW);
    assert_eq!(db.piece_stats("ode"), None);
}
