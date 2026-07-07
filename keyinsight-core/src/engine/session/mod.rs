//! Exercise lifecycle for the training loop: generate (adaptively) →
//! engrave → wait for input → per-note feedback → summary → next. Two
//! pacing modes: self-paced (cursor waits) and tempo (metronome drives;
//! pitch AND timing scored). The skill model updates after every exercise
//! and drives weak-item biasing and range/accidental unlocks; tempo BPM and
//! rhythm vocabulary are their own adaptive axes.
//!
//! Ports `Engine/SessionEngine.swift`, split into focused modules
//! (the Swift file is ~1400 lines; the 800-line rule applies here):
//! - `mod.rs` — state, types, construction, the frame tick
//! - `lifecycle.rs` — start / next_exercise / binding / completion
//! - `input.rs` — event handling, matchers, feedback
//! - `modes.rs` — free play, drills, repertoire, users, playback
//! - `progress.rs` — progress report entries
//!
//! Platform adaptations (see `docs/porting.md`): `@Published` properties
//! are plain fields (agg-gui repaints on `request_draw`), every
//! `DispatchQueue.asyncAfter`/Timer becomes a deadline processed in
//! [`SessionEngine::tick`], `CACurrentMediaTime` is the injected `clock`,
//! and the system RNG is a seeded SplitMix64 (deterministic across
//! platforms).

mod lifecycle;
mod input;
mod modes;
mod progress;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use crate::audio::{AudioOut, Metronome};
use crate::core::{InputBackend, NoteEvent, SplitMix64};
use crate::engine::{OctaveAnchor, SelfPacedMatcher, TempoMatcher, TempoPolicy, TempoReport};
use crate::input::{SimulatedKeyboardBackend, UnpluggedBackend};
use crate::notation::{NotationController, NotationRenderer};
use crate::persistence::{AppDatabase, UserProfile};
use crate::score::{Exercise, ExerciseGenerator, MatchEvent, RepertoirePiece, ScoreNote};
use crate::skill::SkillModel;
use crate::ui::KeyboardLayout;

pub use progress::{IntervalEntry, ProgressEntry};

#[derive(Debug, Clone, PartialEq)]
pub struct ExerciseSummary {
    pub exercise_number: i64,
    pub note_count: usize,
    pub first_try_correct: usize,
    pub error_count: usize,
    pub mean_latency_ms: Option<f64>,
    /// Display name of an item unlocked by this exercise (e.g. "A4"), if any.
    pub newly_unlocked: Option<String>,
    pub streak: i64,
    /// Tempo-mode only.
    pub timing: Option<TempoReport>,
    pub bpm: Option<f64>,
    /// Rhythm vocabulary unlocked by this exercise ("eighth notes"), if any.
    pub rhythm_unlocked: Option<String>,
    /// Repertoire only.
    pub piece_title: Option<String>,
    /// Worst measure of a repertoire play: (1-based measure number, errors).
    pub worst_measure: Option<(usize, i64)>,
    /// Aggregated micro-drill summary.
    pub drill: bool,
    /// Completion came from self-grading (Unplugged input), not detection.
    pub self_verified: bool,
}

impl ExerciseSummary {
    pub fn accuracy_percent(&self) -> i64 {
        if self.note_count == 0 {
            0
        } else {
            (self.first_try_correct as f64 / self.note_count as f64 * 100.0).round() as i64
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacingMode {
    SelfPaced,
    Tempo,
}

impl PacingMode {
    pub fn label(self) -> &'static str {
        match self {
            PacingMode::SelfPaced => "Self-paced",
            PacingMode::Tempo => "Tempo",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSource {
    Midi,
    Keyboard,
    Microphone,
    /// Play a real, unconnected instrument and self-grade against playback.
    SelfVerify,
}

impl InputSource {
    pub fn label(self) -> &'static str {
        match self {
            InputSource::Midi => "MIDI",
            InputSource::Keyboard => "Keys",
            InputSource::Microphone => "Mic",
            InputSource::SelfVerify => "Unplugged",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "MIDI" => Some(InputSource::Midi),
            "Keys" => Some(InputSource::Keyboard),
            "Mic" => Some(InputSource::Microphone),
            "Unplugged" => Some(InputSource::SelfVerify),
            _ => None,
        }
    }

    /// Sources with exact, low-latency note events carry tempo scoring and
    /// the Free Play mirror; mic and self-verified play are self-paced only.
    pub fn supports_timing(self) -> bool {
        matches!(self, InputSource::Midi | InputSource::Keyboard)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // one Phase lives per engine; the
// summary payload is the point of the variant
pub enum Phase {
    Loading,
    Playing,
    Summary(ExerciseSummary),
    Failed(String),
}

/// Builds a platform backend per input source. Shells override to supply
/// real MIDI / mic backends; the core default covers keyboard + unplugged
/// and substitutes the simulated backend elsewhere (documented divergence
/// until the platform backends land — the training loop stays usable).
pub type BackendFactory = Box<dyn Fn(InputSource) -> Box<dyn InputBackend>>;

pub fn default_backend_factory() -> BackendFactory {
    Box::new(|source| match source {
        InputSource::SelfVerify => Box::new(UnpluggedBackend::new()),
        _ => Box::new(SimulatedKeyboardBackend::new()),
    })
}

pub(crate) struct DrillTotals {
    pub notes: usize,
    pub first_try: usize,
    pub errors: usize,
    pub latencies_ms: Vec<f64>,
}

impl DrillTotals {
    pub fn new() -> Self {
        Self {
            notes: 0,
            first_try: 0,
            errors: 0,
            latencies_ms: Vec::new(),
        }
    }
}

/// A deadline-driven action (ports the Swift `DispatchQueue.asyncAfter`
/// calls); processed by [`SessionEngine::tick`].
pub(crate) enum Deferred {
    ClearWrongKeyFlash { midi: u8 },
    ClearHeardUncertain,
    RestoreTempoCurrent { index: usize },
    TempoFinish,
    AutoAdvance { generation: i64 },
    PlaybackDone { generation: i64 },
}

pub struct SessionEngine {
    // --- Published state (the SwiftUI @Published block) ---
    pub(crate) phase: Phase,
    pub(crate) current_note_index: usize,
    pub(crate) note_count: usize,
    pub(crate) errors_this_exercise: usize,
    pub(crate) exercises_completed: i64,
    /// Consecutive first-try-correct notes across the session.
    pub(crate) streak: i64,
    pub(crate) octave_offset: i32,
    pub(crate) mode: PacingMode,
    /// Tempo mode: beats remaining in the count-in, None once running.
    pub(crate) count_in_remaining: Option<i32>,
    /// Tempo mode: beat index within the measure (drives the beat dots).
    pub(crate) beat_in_measure: i32,
    pub(crate) tempo_bpm: f64,
    pub(crate) rhythm_level: i32,
    pub(crate) input_source: InputSource,
    /// Repertoire: the piece being played, None = adaptive training.
    pub(crate) active_piece: Option<RepertoirePiece>,
    /// Free Play mirror: live notation from played notes.
    pub(crate) is_free_play: bool,
    pub(crate) free_play_count: usize,
    pub(crate) last_free_play_note: Option<String>,
    /// Micro-drill: flash cards remaining, None = not drilling.
    pub(crate) drill_remaining: Option<i32>,
    /// Clicked-notation explainer text (vocabulary popover).
    pub(crate) inspection: Option<String>,
    /// Reference-audio playback of the current exercise in progress.
    pub(crate) is_playing_back: bool,
    /// Unplugged mode: failed self-graded passes on the current exercise.
    pub(crate) self_verify_attempts: usize,
    /// "G major · 3/4 · 12 notes" for the side panel.
    pub(crate) exercise_info: Option<String>,
    pub(crate) users: Vec<UserProfile>,
    pub(crate) current_user: Option<UserProfile>,
    /// Mic input level (0…1) for the level meter.
    pub(crate) mic_level: Rc<RefCell<f64>>,
    /// Transient "heard something — couldn't tell what" indicator.
    pub(crate) heard_uncertain: bool,
    pub(crate) content_supports_tempo: bool,
    pub(crate) keys_user_default: bool,
    pub(crate) show_keys: bool,
    /// The key(s) to play right now (unconsumed pitches of the current
    /// event) — drives the keyboard strip highlights.
    pub(crate) current_expected_midis: HashSet<u8>,
    /// Briefly set to a wrongly played key (keyboard strip red flash).
    pub(crate) wrong_key_flash: Option<u8>,
    /// Keyboard strip range for the current content.
    pub(crate) keyboard_layout: KeyboardLayout,
    /// Two-hand generated exercises (opt-in until bass skill items exist);
    /// persists per user.
    pub(crate) two_handed: bool,
    pub(crate) anchored_octaves: i32,

    // --- Collaborators ---
    pub notation: Rc<RefCell<NotationController>>,
    pub(crate) renderer: Rc<RefCell<NotationRenderer>>,
    pub(crate) backend: Box<dyn InputBackend>,
    pub(crate) backend_factory: BackendFactory,
    pub skill: SkillModel,
    pub(crate) metronome: Metronome,
    pub(crate) audio: Rc<dyn AudioOut>,
    /// Host-uptime seconds — the clock every NoteEvent carries.
    pub(crate) clock: Rc<dyn Fn() -> f64>,
    /// Incoming events (drained each tick — backends push here so their
    /// callbacks never re-enter the engine).
    pub(crate) event_queue: Rc<RefCell<VecDeque<NoteEvent>>>,
    /// When set, note-ons are routed here instead of the matcher
    /// (latency-calibration flow).
    pub calibration_tap: Option<Box<dyn Fn(f64)>>,

    // --- Private engine state ---
    pub(crate) generator: ExerciseGenerator,
    pub(crate) rng: SplitMix64,
    pub(crate) exercise: Option<Exercise>,
    /// A history exercise queued for one replay (consumed by next_exercise).
    pub(crate) pending_replay: Option<Exercise>,
    pub(crate) matcher: Option<SelfPacedMatcher>,
    pub(crate) tempo_matcher: Option<TempoMatcher>,
    pub(crate) note_ids: Vec<String>,
    pub(crate) exercise_number: i64,
    pub(crate) count_in_beats: i32,
    pub(crate) input_latency_ms: f64,
    pub(crate) sweep_running: bool,

    // Per-note bookkeeping.
    pub(crate) current_note_start: f64,
    pub(crate) errors_on_current_note: usize,
    pub(crate) first_try_correct: usize,
    pub(crate) latencies_ms: Vec<f64>,
    /// Tempo mode: indices that had a wrong-pitch strike before resolution.
    pub(crate) tempo_error_indices: HashSet<usize>,
    /// Repertoire: error count per measure (accuracy heatmap data).
    pub(crate) errors_by_measure: Vec<i64>,
    pub(crate) measure_by_event: Vec<usize>,
    /// Combined two-voice event stream and its note ids, per event.
    pub(crate) events: Vec<MatchEvent>,
    pub(crate) event_ids: Vec<Vec<String>>,
    /// Pitch positions already matched within each event (chords).
    pub(crate) consumed_positions: Vec<HashSet<usize>>,
    /// Note id → its score note (hover vocabulary).
    pub(crate) note_by_id: std::collections::HashMap<String, ScoreNote>,
    pub(crate) octave_anchor: OctaveAnchor,
    pub(crate) anchor_eligible: bool,
    /// Free play events: each entry is one chord (usually a single note).
    pub(crate) free_play_chords: Vec<Vec<u8>>,
    pub(crate) free_play_last_onset: f64,
    pub(crate) drill_totals: DrillTotals,
    pub(crate) tempo_finish_scheduled: bool,
    pub(crate) playback_generation: i64,

    // Persistence (None = running without a database; the loop still works).
    pub(crate) db: Option<AppDatabase>,
    pub(crate) session_id: Option<i64>,
    pub(crate) exercise_id: Option<i64>,

    pub(crate) started: bool,
    /// Deferred actions (deadline seconds on `clock`, action).
    pub(crate) deferred: Vec<(f64, Deferred)>,
}

pub const DRILL_LENGTH: i32 = 12;
pub const PLAYBACK_PREVIEW_BPM: f64 = 90.0;
/// MIDI mode auto-advances past the summary. Longer when an unlock
/// deserves a look.
pub const AUTO_ADVANCE_DELAY: f64 = 1.5;
pub const AUTO_ADVANCE_UNLOCK_DELAY: f64 = 3.0;
/// Below this confidence a note-on is not scored (mic mode).
pub const CONFIDENCE_THRESHOLD: f64 = 0.6;

impl SessionEngine {
    pub fn new(
        db: Option<AppDatabase>,
        audio: Rc<dyn AudioOut>,
        clock: Rc<dyn Fn() -> f64>,
        backend_factory: BackendFactory,
        rng_seed: u64,
    ) -> Self {
        let renderer = Rc::new(RefCell::new(NotationRenderer::new()));
        let notation = Rc::new(RefCell::new(NotationController::new(Rc::clone(&renderer))));
        let backend = backend_factory(InputSource::Keyboard);
        Self {
            phase: Phase::Loading,
            current_note_index: 0,
            note_count: 0,
            errors_this_exercise: 0,
            exercises_completed: 0,
            streak: 0,
            octave_offset: 0,
            mode: PacingMode::SelfPaced,
            count_in_remaining: None,
            beat_in_measure: 0,
            tempo_bpm: TempoPolicy::START_BPM,
            rhythm_level: 0,
            input_source: InputSource::Keyboard,
            active_piece: None,
            is_free_play: false,
            free_play_count: 0,
            last_free_play_note: None,
            drill_remaining: None,
            inspection: None,
            is_playing_back: false,
            self_verify_attempts: 0,
            exercise_info: None,
            users: Vec::new(),
            current_user: None,
            mic_level: Rc::new(RefCell::new(0.0)),
            heard_uncertain: false,
            content_supports_tempo: true,
            keys_user_default: false,
            show_keys: false,
            current_expected_midis: HashSet::new(),
            wrong_key_flash: None,
            keyboard_layout: KeyboardLayout::covering(48, 84),
            two_handed: false,
            anchored_octaves: 0,
            notation,
            renderer,
            backend,
            backend_factory,
            skill: SkillModel::default(),
            metronome: Metronome::new(Rc::clone(&audio)),
            audio,
            clock,
            event_queue: Rc::new(RefCell::new(VecDeque::new())),
            calibration_tap: None,
            generator: ExerciseGenerator::default(),
            rng: SplitMix64::new(rng_seed),
            exercise: None,
            pending_replay: None,
            matcher: None,
            tempo_matcher: None,
            note_ids: Vec::new(),
            exercise_number: 0,
            count_in_beats: 4,
            input_latency_ms: 0.0,
            sweep_running: false,
            current_note_start: 0.0,
            errors_on_current_note: 0,
            first_try_correct: 0,
            latencies_ms: Vec::new(),
            tempo_error_indices: HashSet::new(),
            errors_by_measure: Vec::new(),
            measure_by_event: Vec::new(),
            events: Vec::new(),
            event_ids: Vec::new(),
            consumed_positions: Vec::new(),
            note_by_id: std::collections::HashMap::new(),
            octave_anchor: OctaveAnchor::default(),
            anchor_eligible: false,
            free_play_chords: Vec::new(),
            free_play_last_onset: 0.0,
            drill_totals: DrillTotals::new(),
            tempo_finish_scheduled: false,
            playback_generation: 0,
            db,
            session_id: None,
            exercise_id: None,
            started: false,
            deferred: Vec::new(),
        }
    }

    // --- Read accessors (the @Published surface the UI observes) ---

    pub fn phase(&self) -> &Phase {
        &self.phase
    }
    pub fn current_note_index(&self) -> usize {
        self.current_note_index
    }
    pub fn note_count(&self) -> usize {
        self.note_count
    }
    pub fn errors_this_exercise(&self) -> usize {
        self.errors_this_exercise
    }
    pub fn exercises_completed(&self) -> i64 {
        self.exercises_completed
    }
    pub fn streak(&self) -> i64 {
        self.streak
    }
    pub fn octave_offset(&self) -> i32 {
        self.octave_offset
    }
    pub fn mode(&self) -> PacingMode {
        self.mode
    }
    pub fn count_in_remaining(&self) -> Option<i32> {
        self.count_in_remaining
    }
    pub fn beat_in_measure(&self) -> i32 {
        self.beat_in_measure
    }
    pub fn tempo_bpm(&self) -> f64 {
        self.tempo_bpm
    }
    pub fn rhythm_level(&self) -> i32 {
        self.rhythm_level
    }
    pub fn input_source(&self) -> InputSource {
        self.input_source
    }
    pub fn active_piece(&self) -> Option<&RepertoirePiece> {
        self.active_piece.as_ref()
    }
    pub fn is_free_play(&self) -> bool {
        self.is_free_play
    }
    pub fn free_play_count(&self) -> usize {
        self.free_play_count
    }
    pub fn last_free_play_note(&self) -> Option<&str> {
        self.last_free_play_note.as_deref()
    }
    pub fn drill_remaining(&self) -> Option<i32> {
        self.drill_remaining
    }
    pub fn inspection(&self) -> Option<&str> {
        self.inspection.as_deref()
    }
    pub fn is_playing_back(&self) -> bool {
        self.is_playing_back
    }
    pub fn self_verify_attempts(&self) -> usize {
        self.self_verify_attempts
    }
    pub fn exercise_info(&self) -> Option<&str> {
        self.exercise_info.as_deref()
    }
    pub fn users(&self) -> &[UserProfile] {
        &self.users
    }
    pub fn current_user(&self) -> Option<&UserProfile> {
        self.current_user.as_ref()
    }
    pub fn mic_level(&self) -> f64 {
        *self.mic_level.borrow()
    }
    pub fn heard_uncertain(&self) -> bool {
        self.heard_uncertain
    }
    pub fn content_supports_tempo(&self) -> bool {
        self.content_supports_tempo
    }
    pub fn keys_user_default(&self) -> bool {
        self.keys_user_default
    }
    pub fn show_keys(&self) -> bool {
        self.show_keys
    }
    pub fn current_expected_midis(&self) -> &HashSet<u8> {
        &self.current_expected_midis
    }
    pub fn wrong_key_flash(&self) -> Option<u8> {
        self.wrong_key_flash
    }
    pub fn keyboard_layout(&self) -> &KeyboardLayout {
        &self.keyboard_layout
    }
    pub fn two_handed(&self) -> bool {
        self.two_handed
    }
    pub fn anchored_octaves(&self) -> i32 {
        self.anchored_octaves
    }
    pub fn input_latency_ms(&self) -> f64 {
        self.input_latency_ms
    }
    pub fn exercise(&self) -> Option<&Exercise> {
        self.exercise.as_ref()
    }

    /// Milliseconds timestamp for persistence, derived from the host clock
    /// (the Swift code passed `Date()`; wall time is not required — only
    /// ordering and deltas are consumed).
    pub(crate) fn now_ms(&self) -> i64 {
        ((self.clock)() * 1000.0) as i64
    }

    pub(crate) fn defer_action(&mut self, delay_seconds: f64, action: Deferred) {
        let due = (self.clock)() + delay_seconds;
        self.deferred.push((due, action));
    }
}
