//! Exercise lifecycle for the training loop: generate (adaptively) →
//! engrave → wait for input → per-note feedback → summary → next. Two
//! pacing modes: self-paced (cursor waits) and tempo (metronome drives;
//! pitch AND timing scored). The skill model updates after every exercise
//! and drives weak-item biasing and range/accidental unlocks; tempo BPM and
//! rhythm vocabulary are their own adaptive axes.
//!
//! Ports `Engine/SessionEngine.swift`, split into focused modules
//! (the Swift file is ~1400 lines; the 800-line rule applies here):
//! - `mod.rs` — state, types, construction
//! - `lifecycle.rs` — start / user state / input source / the frame tick
//! - `generation.rs` — next_exercise, hand selection, generator config
//! - `binding.rs` — render→event binding, note-state painting
//! - `completion.rs` — finish_exercise, unlocks, attempt recording
//! - `input.rs` — event handling, matchers, feedback
//! - `modes.rs` — free play, drills, repertoire, users, playback
//! - `survival.rs` — survival runs: sliding chunk window, lives, scoring
//! - `progress.rs` — progress report entries
//! - `types.rs` — the public value types (modes, phase, summary)
//!
//! Platform adaptations (see `docs/porting.md`): `@Published` properties
//! are plain fields (agg-gui repaints on `request_draw`), every
//! `DispatchQueue.asyncAfter`/Timer becomes a deadline processed in
//! [`SessionEngine::tick`], `CACurrentMediaTime` is the injected `clock`,
//! and the system RNG is a seeded SplitMix64 (deterministic across
//! platforms).

mod binding;
mod completion;
mod generation;
mod lifecycle;
mod input;
mod modes;
mod progress;
mod survival;
mod types;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use crate::audio::{AudioOut, Metronome};
use crate::core::{InputBackend, NoteEvent, SplitMix64};
use crate::engine::{InputStormDetector, OctaveAnchor, SelfPacedMatcher, TempoMatcher, TempoPolicy};
use crate::input::{SimulatedKeyboardBackend, UnpluggedBackend};
use crate::notation::{NotationController, NotationRenderer};
use crate::persistence::{AppDatabase, UserProfile};
use crate::score::{Exercise, ExerciseGenerator, MatchEvent, RepertoirePiece, ScoreNote, Staff};
use crate::skill::SkillModel;
use crate::ui::KeyboardLayout;

pub use progress::{ChordEntry, IntervalEntry, ProgressEntry, TransitionEntry};
pub use types::{
    ExerciseSummary, HandMode, InputSource, PacingMode, Phase, SurvivalReport, TempoDebug,
};

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

/// One free-play note as recorded: onset and release in seconds from
/// the take's first note (`end` is None while the key is still held).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FreePlayRecordedNote {
    pub midi: u8,
    pub start: f64,
    pub end: Option<f64>,
}

/// A drill card owed a retrieval rep, due once `drill_cards_done` reaches
/// `due`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DrillRedo {
    pub midi: u8,
    pub staff: Staff,
    pub due: i32,
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
    /// Survival: swap the stitched window once the seam line's slide has
    /// settled (stale generations are ignored).
    SurvivalWindowSwap { generation: i64 },
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
    /// The pacing actually running this exercise: `mode` is the user's
    /// choice; content the tempo matcher can't score (chords, hands
    /// together) and drills run self-paced without overwriting it, so
    /// tempo survives Auto's mixed content.
    pub(crate) active_pacing: PacingMode,
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
    /// Micro-drill: endless flash cards until End Drill.
    pub(crate) drill_active: bool,
    pub(crate) drill_cards_done: i32,
    /// Drill correction: a wrong strike reveals the keyboard with the
    /// right key lit until the card is answered.
    pub(crate) drill_hint_keys: bool,
    /// Practice-from-here (repertoire): the match event the replay starts
    /// from; 0 = the whole piece.
    pub(crate) replay_start_event: usize,
    /// Survival mode (OQ-25): endless neutral-bias chunks at the current
    /// level with an error budget of lives; one key per run.
    pub(crate) is_survival: bool,
    pub(crate) survival_lives: i32,
    pub(crate) survival_notes: usize,
    /// Mash guard: wrong-strike storms suspend attempt recording until a
    /// clean event; the badge keeps it honest.
    pub(crate) stats_suppressed: bool,
    pub(crate) storm_detector: InputStormDetector,
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
    /// Which hand(s) training exercises target; persists per user.
    pub(crate) hand_mode: HandMode,
    /// Octave-following scaffold (per user): the exercise follows the
    /// octave the player starts in. Off = written octaves are required.
    pub(crate) follow_octave: bool,
    pub(crate) anchored_octaves: i32,

    // --- Collaborators ---
    pub notation: Rc<RefCell<NotationController>>,
    pub(crate) renderer: Rc<RefCell<NotationRenderer>>,
    pub(crate) backend: Box<dyn InputBackend>,
    pub(crate) backend_factory: BackendFactory,
    pub skill: SkillModel,
    /// Left-hand model: its own unlock ladder over the bass staff.
    pub bass_skill: SkillModel,
    /// Hold off display sleep while an exercise is running (the Swift
    /// `DisplaySleepGuard` power assertion, driven by `phase`). Shells
    /// install the platform call; `None` = no-op.
    pub display_awake: Option<Box<dyn Fn(bool)>>,
    pub(crate) display_awake_active: bool,
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
    /// Consecutive clean self-paced training exercises (rhythm advancement).
    pub(crate) rhythm_clean_streak: i32,
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
    /// Free play take: every note-on with its onset (seconds from the
    /// first note) and release, for replay at the played timing.
    pub(crate) free_play_recording: Vec<FreePlayRecordedNote>,
    pub(crate) free_play_record_start: Option<f64>,
    pub(crate) drill_totals: DrillTotals,
    /// The previous card's pitch — an identical card is invisible.
    pub(crate) last_drill_midi: Option<u8>,
    /// Retrieval reps: a missed card comes back a few cards later.
    pub(crate) drill_redo: Vec<DrillRedo>,
    /// Practice-from-here: events before this index are never expected —
    /// grayed out, excluded from counts and reports. 0 outside repertoire.
    pub(crate) start_event_index: usize,
    /// Tempo mode: beat-in-measure offset of the start spot (the sweep's
    /// beat dots stay aligned to the score when starting mid-measure).
    pub(crate) start_beat_offset: i32,
    /// Survival run state: the best score (persisted per user), the run's
    /// start on the host clock, the difficulty index of every chunk served
    /// (score multiplier), the two lookahead chunks of the sliding window,
    /// the event count of the active (top) line — crossing it schedules
    /// the window swap — the run's key, and the window generation counter
    /// (a stale scheduled swap is ignored).
    pub(crate) survival_best: i64,
    pub(crate) survival_start: f64,
    pub(crate) survival_difficulties: Vec<f64>,
    pub(crate) survival_upcoming: Vec<Exercise>,
    pub(crate) survival_seam_events: usize,
    pub(crate) survival_fifths: i32,
    pub(crate) survival_window_gen: i64,
    pub(crate) tempo_finish_scheduled: bool,
    pub(crate) playback_generation: i64,
    /// Host-clock second the current Hear It playback started (drives the
    /// keyboard strip's sounding-key highlight).
    pub(crate) playback_started_at: f64,

    // Persistence (None = running without a database; the loop still works).
    pub(crate) db: Option<AppDatabase>,
    pub(crate) session_id: Option<i64>,
    pub(crate) exercise_id: Option<i64>,

    pub(crate) started: bool,
    /// Deferred actions (deadline seconds on `clock`, action).
    pub(crate) deferred: Vec<(f64, Deferred)>,
    /// The scripted demo's `engine:` trace lines (the Swift `isDemo`
    /// prints in `start` and the survival window advance).
    pub demo_trace: bool,
}

/// Drill cards the scripted demo plays before ending the (endless) drill.
pub const DRILL_LENGTH: i32 = 12;
/// A missed drill card returns for a retrieval rep this many cards later.
pub const DRILL_REDO_DELAY_CARDS: i32 = 3;
pub const PLAYBACK_PREVIEW_BPM: f64 = 90.0;
/// Latencies past this are a break, not slowness (mastery robustness).
pub const LATENCY_OUTLIER_MS: f64 = 15_000.0;
/// MIDI mode auto-advances past the summary. Longer when an unlock
/// deserves a look.
pub const AUTO_ADVANCE_DELAY: f64 = 1.5;
pub const AUTO_ADVANCE_UNLOCK_DELAY: f64 = 3.0;
/// Below this confidence a note-on is not scored (mic mode).
pub const CONFIDENCE_THRESHOLD: f64 = 0.6;
/// Survival: the window swaps this long after the seam line's slide
/// starts (the slide has settled; the swap is imperceptible).
pub const SURVIVAL_SWAP_DELAY: f64 = 0.5;
/// Survival chunks are one two-bar line each.
pub const SURVIVAL_CHUNK_MEASURES: i32 = 2;

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
            active_pacing: PacingMode::SelfPaced,
            count_in_remaining: None,
            beat_in_measure: 0,
            tempo_bpm: TempoPolicy::START_BPM,
            rhythm_level: 0,
            input_source: InputSource::Keyboard,
            active_piece: None,
            is_free_play: false,
            free_play_count: 0,
            last_free_play_note: None,
            drill_active: false,
            drill_cards_done: 0,
            drill_hint_keys: false,
            replay_start_event: 0,
            is_survival: false,
            survival_lives: 0,
            survival_notes: 0,
            stats_suppressed: false,
            storm_detector: InputStormDetector::default(),
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
            hand_mode: HandMode::Right,
            follow_octave: true,
            anchored_octaves: 0,
            notation,
            renderer,
            backend,
            backend_factory,
            skill: SkillModel::default(),
            bass_skill: SkillModel::with_staff(Staff::Bass, crate::skill::SEED_COUNT),
            display_awake: None,
            display_awake_active: false,
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
            rhythm_clean_streak: 0,
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
            free_play_recording: Vec::new(),
            free_play_record_start: None,
            drill_totals: DrillTotals::new(),
            last_drill_midi: None,
            drill_redo: Vec::new(),
            start_event_index: 0,
            start_beat_offset: 0,
            survival_best: 0,
            survival_start: 0.0,
            survival_difficulties: Vec::new(),
            survival_upcoming: Vec::new(),
            survival_seam_events: 0,
            survival_fifths: 0,
            survival_window_gen: 0,
            tempo_finish_scheduled: false,
            playback_generation: 0,
            playback_started_at: 0.0,
            db,
            session_id: None,
            exercise_id: None,
            started: false,
            deferred: Vec::new(),
            demo_trace: false,
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
    pub fn active_pacing(&self) -> PacingMode {
        self.active_pacing
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
    pub fn drill_active(&self) -> bool {
        self.drill_active
    }
    pub fn drill_cards_done(&self) -> i32 {
        self.drill_cards_done
    }
    pub fn drill_hint_keys(&self) -> bool {
        self.drill_hint_keys
    }
    pub fn replay_start_event(&self) -> usize {
        self.replay_start_event
    }
    /// 1-based measure number of the start spot (side panel chip).
    pub fn replay_start_measure(&self) -> usize {
        if self.start_event_index < self.measure_by_event.len() {
            self.measure_by_event[self.start_event_index] + 1
        } else {
            1
        }
    }
    /// "Note k of note_count", counted from the start spot.
    pub fn current_note_number(&self) -> usize {
        (self.current_note_index + 1).saturating_sub(self.start_event_index).max(1)
    }
    pub fn stats_suppressed(&self) -> bool {
        self.stats_suppressed
    }
    pub fn is_survival(&self) -> bool {
        self.is_survival
    }
    pub fn survival_lives(&self) -> i32 {
        self.survival_lives
    }
    pub fn survival_notes(&self) -> usize {
        self.survival_notes
    }
    pub fn survival_best(&self) -> i64 {
        self.survival_best
    }
    pub fn survival_window_gen(&self) -> i64 {
        self.survival_window_gen
    }
    pub fn inspection(&self) -> Option<&str> {
        self.inspection.as_deref()
    }
    pub fn is_playing_back(&self) -> bool {
        self.is_playing_back
    }

    /// Serialized persistence document (test support: relaunch scenarios
    /// reopen a fresh engine over the same stored state).
    pub fn db_document(&self) -> Option<String> {
        self.db.as_ref().map(|db| db.serialize_document())
    }

    /// MIDI pitches sounding right now in the Hear It playback — the
    /// keyboard strip shows them as pressed keys while the piece plays.
    pub fn playback_sounding_midis(&self) -> Vec<u8> {
        if !self.is_playing_back {
            return Vec::new();
        }
        let elapsed = (self.clock)() - self.playback_started_at;
        if self.is_free_play {
            // A free-play take replays at its recorded timing.
            let mut sounding: Vec<u8> = self
                .free_play_recorded_notes()
                .iter()
                .filter(|n| elapsed >= n.start_seconds && elapsed < n.start_seconds + n.duration_seconds)
                .map(|n| n.midi)
                .collect();
            sounding.sort_unstable();
            sounding.dedup();
            return sounding;
        }
        let Some(exercise) = &self.exercise else {
            return Vec::new();
        };
        let unit_seconds = (60.0 / PLAYBACK_PREVIEW_BPM) / 2.0;
        let mut sounding = Vec::new();
        for voice in [&exercise.notes, &exercise.bass_notes] {
            for span in crate::score::Exercise::voice_note_spans(voice) {
                let start = span.start_units as f64 * unit_seconds;
                let end = start + span.length_units as f64 * unit_seconds;
                if elapsed >= start && elapsed < end {
                    sounding.push(span.midi);
                }
            }
        }
        sounding.sort_unstable();
        sounding.dedup();
        sounding
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
    pub fn hand_mode(&self) -> HandMode {
        self.hand_mode
    }
    pub fn follow_octave(&self) -> bool {
        self.follow_octave
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

    /// The one place `phase` changes (the Swift `didSet`): the display
    /// sleep guard follows it — awake while playing, released otherwise.
    pub(crate) fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
        let active = self.phase == Phase::Playing;
        if active != self.display_awake_active {
            self.display_awake_active = active;
            if let Some(hook) = &self.display_awake {
                hook(active);
            }
        }
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
