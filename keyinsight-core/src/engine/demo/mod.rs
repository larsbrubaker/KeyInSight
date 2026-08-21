//! Scripted event playback (the second half of the simulated backend,
//! 03-architecture.md): drives the whole training loop deterministically
//! for end-to-end verification and demos, no keyboard needed.
//!
//!   keyinsight-native --demo
//!
//! Ports `Engine/DemoDriver.swift`. The acts run in the Swift order:
//! 1 self-paced to the first unlock, 2 one scripted tempo exercise,
//! 3 a bundled piece, 3.5 practice-from-here, 4 the Free Play mirror,
//! 5 a micro-drill, 6 the reference-playback smoke test + follow-cursor
//! audit, 7 Unplugged self-verification, 8 a survival run. Every
//! `demo:` log line is the Swift line verbatim; exit 0 on success, 1 on
//! a script mismatch, 2 on the watchdog.
//!
//! Platform adaptations: the Swift driver chained `DispatchQueue.main
//! .asyncAfter` closures on the wall clock; here the same intervals
//! advance a scripted clock shared with the engine ([`DemoDriver::after`])
//! in painted-frame steps, ticking the engine and pumping the notation
//! follow cursor exactly as the frame loop does — so the whole demo runs
//! headless, in virtual time, and `cargo test` covers it act by act.
//! `--snapshot-dir` (WKWebView PNG snapshots) and the Act 6 window
//! occlusion check are not ported — see `docs/platform-substitutions.md`.

mod acts_early;
mod acts_late;

pub use acts_early::{MAX_SELF_PACED_EXERCISES, PARTIAL_START_EVENT};
pub use acts_late::{SURVIVAL_NOTE_GAP, SURVIVAL_TARGET_NOTES};

use std::cell::RefCell;
use std::rc::Rc;

use crate::audio::AudioOut;
use crate::core::{NoteEvent, NoteEventKind};
use crate::engine::session::{default_backend_factory, Phase, SessionEngine};
use crate::notation::NoteState;
use crate::persistence::AppDatabase;
use crate::score::Staff;
use crate::skill::UNLOCK_ORDER;

/// Process exit code for a script mismatch (the Swift `exit(1)`).
pub const EXIT_FAILED: i32 = 1;
/// Process exit code for the watchdog (the Swift `exit(2)`).
pub const EXIT_TIMEOUT: i32 = 2;
/// Watchdog: a stall anywhere must fail loudly, not hang forever
/// (180 s of scripted time, as the Swift 180 s wall-clock watchdog).
pub const WATCHDOG_SECONDS: f64 = 180.0;
/// The scripted clock advances in painted-frame steps: every `after`
/// interval ticks the engine and the follow cursor the way a 60 Hz frame
/// loop would (the Swift rAF page loop the follow audit depends on).
pub const FRAME_SECONDS: f64 = 1.0 / 60.0;
/// Seed of the headless demo engine: fixed so every run is the same
/// playthrough (the Swift demo ran on the system RNG).
pub const DEMO_SEED: u64 = 42;
/// `now_ms` of the demo's throwaway database.
pub const DEMO_EPOCH_MS: i64 = 1_700_000_000_000;
/// The scripted clock's starting host second.
pub const DEMO_CLOCK_START: f64 = 1000.0;

/// `Ok(value)` or the process exit code to stop with.
pub type DemoResult<T> = Result<T, i32>;

/// Silent output that ACCEPTS playback — the demo's Act 6 needs
/// `toggle_playback` to start (the Swift demo ran against the real
/// sampler; headless runs have no audio device, and `NullAudioOut`
/// declines SMF playback, which would skip the act).
pub struct DemoAudio;

impl DemoAudio {
    /// What Act 6 reports as the instrument (the Swift line printed the
    /// `PlaybackEngine.instrumentDescription`).
    pub const INSTRUMENT_DESCRIPTION: &'static str = "silent headless output";
}

impl AudioOut for DemoAudio {
    fn play_click(&self, _at_host_seconds: f64, _accent: bool) {}
    fn play_smf(&self, _smf: &[u8]) -> bool {
        true
    }
    fn stop_smf(&self) {}
    fn play_note(&self, _midi: u8, _duration_seconds: f64) {}
}

/// The headless demo engine: a throwaway in-memory database (the Swift
/// `AppDatabase.temporary()` — scripted playthroughs never pollute real
/// training stats), the accepting silent audio, the scripted clock, the
/// core backend factory, the fixed seed, and the `engine:` trace on.
/// Not yet started; [`run_demo`] starts it.
pub fn headless_demo_engine() -> (SessionEngine, Rc<RefCell<f64>>) {
    let clock = Rc::new(RefCell::new(DEMO_CLOCK_START));
    let reader = Rc::clone(&clock);
    let mut engine = SessionEngine::new(
        Some(AppDatabase::in_memory(DEMO_EPOCH_MS)),
        Rc::new(DemoAudio),
        Rc::new(move || *reader.borrow()),
        default_backend_factory(),
        DEMO_SEED,
    );
    engine.demo_trace = true;
    (engine, clock)
}

/// Run the whole scripted playthrough against `engine` (started here if
/// not already), advancing `clock` — the engine's clock — by the script's
/// intervals. Returns the process exit code: 0 on success.
pub fn run_demo(engine: &mut SessionEngine, clock: Rc<RefCell<f64>>) -> i32 {
    let mut driver = DemoDriver::new(engine, clock);
    match driver.run() {
        Ok(()) => 0,
        Err(code) => code,
    }
}

/// The scripted playthrough, act by act (`DemoDriver.swift`). Each act is
/// its own method so tests can drive them individually.
pub struct DemoDriver<'a> {
    pub(crate) engine: &'a mut SessionEngine,
    clock: Rc<RefCell<f64>>,
    started_at: f64,
    /// Every `demo:` line printed, in order (tests read it back).
    pub log: Vec<String>,
}

impl<'a> DemoDriver<'a> {
    pub fn new(engine: &'a mut SessionEngine, clock: Rc<RefCell<f64>>) -> Self {
        let started_at = *clock.borrow();
        Self {
            engine,
            clock,
            started_at,
            log: Vec::new(),
        }
    }

    /// All acts in order (the Swift callback chain from
    /// `startIfRequested` through `survivalStep`'s `exit(0)`).
    pub fn run(&mut self) -> DemoResult<()> {
        self.engine.start();
        self.say("demo: starting scripted playthrough".to_string());
        // Give the WebView a beat to finish initial layout.
        self.after(1.0)?;
        self.act1_unlock()?;
        self.act2_tempo()?;
        let (full_note_count, plays_before) = self.act3_repertoire()?;
        self.act3_5_practice_from_here(full_note_count, plays_before)?;
        self.act4_free_play()?;
        self.act5_drill()?;
        self.act6_playback()?;
        self.act7_self_verify()?;
        self.act8_survival()
    }

    // --- Plumbing ---

    pub(crate) fn now(&self) -> f64 {
        *self.clock.borrow()
    }

    /// The Swift `after(seconds) { … }`: let `seconds` of scripted time
    /// pass, frame by frame — each frame ticks the engine (input queue,
    /// deferred actions, metronome sweep) and paints the follow cursor —
    /// then the watchdog is checked.
    pub(crate) fn after(&mut self, seconds: f64) -> DemoResult<()> {
        let mut remaining = seconds;
        while remaining > 0.0 {
            let step = remaining.min(FRAME_SECONDS);
            remaining -= step;
            *self.clock.borrow_mut() += step;
            self.engine.tick();
            // The notation widget's paint: the follow cursor advances
            // (and logs what it painted) every painted frame.
            let now = self.now();
            let mut notation = self.engine.notation.borrow_mut();
            if notation.is_following() {
                notation.follow_ids_at(now);
            }
        }
        if self.now() - self.started_at >= WATCHDOG_SECONDS {
            let line = format!(
                "demo: TIMEOUT — phase {}, note {}",
                phase_label(self.engine.phase()),
                self.engine.current_note_index()
            );
            self.say(line);
            return Err(EXIT_TIMEOUT);
        }
        Ok(())
    }

    /// A struck key: note-on and an immediate note-off, both stamped now.
    pub(crate) fn inject(&mut self, midi: u8) {
        let now = self.now();
        self.engine.handle(NoteEvent {
            kind: NoteEventKind::On,
            midi,
            velocity: Some(80),
            timestamp: now,
            confidence: 1.0,
        });
        self.engine.handle(NoteEvent {
            kind: NoteEventKind::Off,
            midi,
            velocity: None,
            timestamp: now,
            confidence: 1.0,
        });
    }

    pub(crate) fn say(&mut self, line: String) {
        println!("{line}");
        self.log.push(line);
    }

    /// `demo: FAILED — …` and the exit code to return (the Swift
    /// `print(...); exit(1)`).
    pub(crate) fn fail(&mut self, message: &str) -> i32 {
        self.say(format!("demo: FAILED — {message}"));
        EXIT_FAILED
    }

    /// The progress dump after the repertoire and drill acts.
    pub(crate) fn report(&mut self) {
        self.say("demo: --- progress report ---".to_string());
        let entries = self.engine.progress_entries(Staff::Treble);
        for entry in entries.into_iter().filter(|e| e.unlocked) {
            // `name.padding(toLength: 4, withPad: " ", startingAt: 0)`:
            // padded or truncated to four characters.
            let name: String = format!("{:<4}", entry.name).chars().take(4).collect();
            let line = format!(
                "demo:   {} heat={} attempts={} err={}{}",
                name,
                heat_raw_value(entry.heat),
                entry.attempts,
                entry
                    .error_percent
                    .map(|p| format!("{p}%"))
                    .unwrap_or_else(|| "—".to_string()),
                if entry.mastered { " ✓mastered" } else { "" }
            );
            self.say(line);
        }
        let line = format!(
            "demo: unlocked items: {}/{}, tempo {} BPM, rhythm level {}",
            self.engine.skill.unlocked_count(),
            UNLOCK_ORDER.len(),
            self.engine.tempo_bpm() as i32,
            self.engine.rhythm_level()
        );
        self.say(line);
    }
}

/// The Swift `NoteState: String` raw value (the heat column of the
/// progress report).
fn heat_raw_value(state: NoteState) -> &'static str {
    match state {
        NoteState::Current => "current",
        NoteState::Correct => "correct",
        NoteState::Wrong => "wrong",
        NoteState::Missed => "missed",
        NoteState::Mastered => "mastered",
        NoteState::Learning => "learning",
        NoteState::Weak => "weak",
        NoteState::Locked => "locked",
    }
}

/// The Swift `"\(engine.phase)"` interpolation of the watchdog line.
fn phase_label(phase: &Phase) -> String {
    match phase {
        Phase::Loading => "loading".to_string(),
        Phase::Playing => "playing".to_string(),
        Phase::Summary(summary) => format!("summary({summary:?})"),
        Phase::Failed(message) => format!("failed({message:?})"),
    }
}

/// `String(format: "%.0f ms", x)` / `"n/a"`.
pub(crate) fn ms_or_na(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.0} ms"))
        .unwrap_or_else(|| "n/a".to_string())
}
