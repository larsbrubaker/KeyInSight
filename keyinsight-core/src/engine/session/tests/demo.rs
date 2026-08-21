//! The ported `--demo` DemoDriver, act by act, against the headless demo
//! engine (the survival act lives in `survival`). The driver itself is
//! what runs here: `cargo test` covers the same scripted playthrough the
//! `keyinsight-native --demo` shell runs.

use std::cell::RefCell;
use std::rc::Rc;

use crate::engine::demo::{headless_demo_engine, run_demo, DemoDriver};
use crate::engine::session::{InputSource, PacingMode, Phase, SessionEngine};

fn started_demo_engine() -> (SessionEngine, Rc<RefCell<f64>>) {
    let (mut engine, clock) = headless_demo_engine();
    engine.start();
    (engine, clock)
}

fn assert_logged(driver: &DemoDriver<'_>, line: &str) {
    assert!(
        driver.log.iter().any(|l| l == line),
        "missing {line:?} in log {:#?}",
        driver.log
    );
}

/// DemoDriver Act 1: one wrong note in each of the first two exercises,
/// then clean play until the first unlock (within 14 exercises).
#[test]
fn demo_act1() {
    let (mut engine, clock) = started_demo_engine();
    let mut driver = DemoDriver::new(&mut engine, clock);
    assert_eq!(driver.act1_unlock(), Ok(()), "log: {:#?}", driver.log);
    assert_eq!(
        driver
            .log
            .iter()
            .filter(|l| l.starts_with("demo: injected wrong note (expected "))
            .count(),
        2
    );
    assert_logged(
        &driver,
        "demo: act 1 passed (unlock earned) — switching to tempo mode",
    );
    assert!(driver.log.iter().any(|l| l.contains(", UNLOCKED ")));
    assert!(engine.exercises_completed() <= 14);
}

/// DemoDriver Act 2: the scripted timing profile (early, late, missed,
/// wrong pitch) is classified by the timing report.
#[test]
fn demo_act2() {
    let (mut engine, clock) = started_demo_engine();
    let mut driver = DemoDriver::new(&mut engine, clock);
    assert_eq!(driver.act2_tempo(), Ok(()), "log: {:#?}", driver.log);
    assert_logged(&driver, "demo: injected wrong pitch before note 5");
    assert_logged(
        &driver,
        "demo: act 2 passed (tempo classification) — starting repertoire act",
    );
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected the tempo summary");
    };
    let timing = summary.timing.as_ref().expect("tempo summary carries timing");
    assert_eq!(timing.missed, 1);
    assert!(timing.early >= 1 && timing.late >= 1);
    assert_eq!(timing.hit_count(), timing.expected_count - 1);
    assert!(summary.error_count >= 1);
    assert_eq!(engine.mode(), PacingMode::Tempo);
}

/// DemoDriver Act 3: Minuet in G with one wrong note at index 6 — piece
/// title, worst measure, and a recorded play.
#[test]
fn demo_act3() {
    let (mut engine, clock) = started_demo_engine();
    let mut driver = DemoDriver::new(&mut engine, clock);
    let (full_note_count, plays) = driver.act3_repertoire().expect("act 3 passes");
    assert_eq!(plays, 1);
    assert!(full_note_count > 8);
    assert_logged(&driver, "demo: --- progress report ---");
    assert_logged(
        &driver,
        "demo: act 3 passed (repertoire) — starting practice-from-here act",
    );
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected the piece summary");
    };
    assert_eq!(summary.piece_title.as_deref(), Some("Minuet in G"));
    assert_eq!(summary.note_count, summary.first_try_correct + 1);
    assert!(summary.worst_measure.is_some());
}

/// DemoDriver Act 3.5: practice-from-here (partial replay) on the piece
/// the repertoire act just played.
#[test]
fn demo_practice_from_here_act() {
    let (mut engine, clock) = started_demo_engine();
    let mut driver = DemoDriver::new(&mut engine, clock);
    let (full_note_count, plays_before) = driver.act3_repertoire().expect("act 3 passes");
    assert_eq!(
        driver.act3_5_practice_from_here(full_note_count, plays_before),
        Ok(()),
        "log: {:#?}",
        driver.log
    );
    assert_logged(
        &driver,
        "demo: act 3.5 passed (practice-from-here) — starting free-play act",
    );
    // Section practice did not count as a play; the whole piece is back.
    assert_eq!(engine.piece_stats("minuet-in-g").map(|s| s.0), Some(plays_before));
    assert_eq!(engine.replay_start_event(), 0);
    assert_eq!(engine.note_count(), full_note_count);
    assert_eq!(*engine.phase(), Phase::Playing);
}

/// DemoDriver Act 4: the Free Play mirror counts the riff and names the
/// last note.
#[test]
fn demo_act4() {
    let (mut engine, clock) = started_demo_engine();
    let mut driver = DemoDriver::new(&mut engine, clock);
    assert_eq!(driver.act4_free_play(), Ok(()), "log: {:#?}", driver.log);
    assert_logged(&driver, "demo: free play — 5 notes mirrored, last E5");
    assert_logged(
        &driver,
        "demo: act 4 passed (free play) — starting micro-drill act",
    );
    assert!(!engine.is_free_play());
    assert_eq!(*engine.phase(), Phase::Playing);
}

/// DemoDriver Act 5: `DRILL_LENGTH` cards, never the same card twice in
/// a row, one aggregated summary.
#[test]
fn demo_act5() {
    let (mut engine, clock) = started_demo_engine();
    let mut driver = DemoDriver::new(&mut engine, clock);
    assert_eq!(driver.act5_drill(), Ok(()), "log: {:#?}", driver.log);
    assert!(driver
        .log
        .iter()
        .any(|l| l.starts_with("demo: drill complete — 12 cards, 12 first try, mean ")));
    assert_logged(&driver, "demo: act 5 passed (drill) — playback smoke test");
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected the drill summary");
    };
    assert!(summary.drill);
}

/// DemoDriver Act 6: Hear It completes on the drill's last card and the
/// follow cursor paints every note of a fresh exercise, in order.
#[test]
fn demo_act6() {
    let (mut engine, clock) = started_demo_engine();
    let mut driver = DemoDriver::new(&mut engine, clock);
    driver.act5_drill().expect("act 5 passes");
    assert_eq!(driver.act6_playback(), Ok(()), "log: {:#?}", driver.log);
    assert_logged(&driver, "demo: playback started (silent headless output)");
    assert!(driver
        .log
        .iter()
        .any(|l| l.starts_with("demo: follow audit — ")));
    let count = driver.engine.note_count();
    assert_logged(
        &driver,
        &format!("demo: act 6 passed (playback + follow cursor painted all {count} notes)"),
    );
    drop(driver);
    assert!(!engine.is_playing_back());
    let expected: Vec<usize> = (0..count).collect();
    assert_eq!(engine.notation.borrow().follow_log(), expected.as_slice());
}

/// DemoDriver Act 7: Unplugged self-verification — Try Again, then
/// Nailed It — records one repeated pass.
#[test]
fn demo_act7() {
    let (mut engine, clock) = started_demo_engine();
    let mut driver = DemoDriver::new(&mut engine, clock);
    assert_eq!(driver.act7_self_verify(), Ok(()), "log: {:#?}", driver.log);
    assert_logged(&driver, "demo: self-verify — graded Try Again (pass 1)");
    assert!(driver.log.iter().any(|l| {
        l.starts_with("demo: self-verify complete — ")
            && l.ends_with(" notes, 1 repeated pass, recorded to item stats")
    }));
    assert_eq!(engine.input_source(), InputSource::SelfVerify);
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected the self-verified summary");
    };
    assert!(summary.self_verified);
    assert_eq!(summary.error_count, 1);
    assert_eq!(summary.first_try_correct, 0);
}

/// The whole playthrough, exactly as `keyinsight-native --demo` runs it:
/// every act in the Swift order, exit code 0, the closing OK line last.
#[test]
fn demo_full_run_exits_zero() {
    let (mut engine, clock) = headless_demo_engine();
    let code = run_demo(&mut engine, clock);
    assert_eq!(code, 0);
    let Phase::Summary(summary) = engine.phase() else {
        panic!("the demo ends on the survival summary");
    };
    assert!(summary.survival.is_some());
}
