//! Public value types of the session engine: pacing/input/hand modes, the
//! exercise phase, and the end-of-exercise summary (with the survival run
//! report).

use crate::engine::TempoReport;

/// Which hand(s) training exercises target. Auto rotates by weakness and
/// mixes in two-hand exercises once the bass seed range is mastered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandMode {
    Right,
    Left,
    Both,
    Auto,
}

impl HandMode {
    pub const ALL: [HandMode; 4] = [
        HandMode::Right,
        HandMode::Left,
        HandMode::Both,
        HandMode::Auto,
    ];

    /// Swift raw value (the persisted `hand_mode` setting).
    pub fn raw_value(self) -> &'static str {
        match self {
            HandMode::Right => "Right",
            HandMode::Left => "Left",
            HandMode::Both => "Both",
            HandMode::Auto => "Auto",
        }
    }

    pub fn from_raw_value(raw: &str) -> Option<HandMode> {
        match raw {
            "Right" => Some(HandMode::Right),
            "Left" => Some(HandMode::Left),
            "Both" => Some(HandMode::Both),
            "Auto" => Some(HandMode::Auto),
            _ => None,
        }
    }
}

/// Survival run results (OQ-25).
#[derive(Debug, Clone, PartialEq)]
pub struct SurvivalReport {
    pub score: i64,
    pub notes: usize,
    pub notes_per_minute: f64,
    pub difficulty: f64,
    pub best: i64,
    pub is_new_best: bool,
}

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
    /// Set when this summary closes a survival run.
    pub survival: Option<SurvivalReport>,
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
