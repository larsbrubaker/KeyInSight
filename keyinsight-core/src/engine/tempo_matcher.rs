//! Tempo-mode matcher: expected notes carry target times; incoming
//! note-ons match the nearest unconsumed expected note of the same pitch
//! within the tolerance window; classified hit (on-time / early / late),
//! wrong, or — when the window closes unconsumed — missed. Re-strikes of
//! already-consumed events are ignored (grace-note-like double strikes).
//! Onsets only in v1.
//!
//! Also ports the tempo/rhythm adaptation policies.
//!
//! Ports `Engine/TempoMatcher.swift`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoExpected {
    pub midi: u8,
    pub target_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    OnTime,
    Early,
    Late,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TempoResolution {
    Hit { timing: Timing, offset_ms: f64 },
    Missed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TempoOutcome {
    Hit {
        index: usize,
        timing: Timing,
        offset_ms: f64,
        exercise_complete: bool,
    },
    /// No in-window match; `nearest_index` anchors the visual feedback.
    Wrong { nearest_index: usize, played: u8 },
    /// Double strike of an already-resolved event.
    Ignored,
}

pub struct TempoMatcher {
    pub expected: Vec<TempoExpected>,
    pub resolutions: Vec<Option<TempoResolution>>,
}

impl TempoMatcher {
    /// Tolerance window (start at ±120 ms; tightening with mastery is
    /// future work).
    pub const TOLERANCE_MS: f64 = 120.0;
    /// Within ±this the hit is "on time" — no early/late tick.
    pub const ON_TIME_MS: f64 = 45.0;

    pub fn new(expected: Vec<TempoExpected>) -> Self {
        let resolutions = vec![None; expected.len()];
        Self {
            expected,
            resolutions,
        }
    }

    pub fn is_complete(&self) -> bool {
        !self.resolutions.iter().any(|r| r.is_none())
    }

    /// The cursor: first unresolved event, in score order.
    pub fn first_unresolved_index(&self) -> Option<usize> {
        self.resolutions.iter().position(|r| r.is_none())
    }

    pub fn consume_note_on(&mut self, midi: u8, now_ms: f64) -> TempoOutcome {
        if self.is_complete() {
            return TempoOutcome::Ignored;
        }

        // Best unresolved same-pitch event inside the window.
        let mut best: Option<(usize, f64)> = None;
        for (i, event) in self.expected.iter().enumerate() {
            if self.resolutions[i].is_some() || event.midi != midi {
                continue;
            }
            let offset = now_ms - event.target_ms;
            if offset.abs() > Self::TOLERANCE_MS {
                continue;
            }
            if best.is_none() || offset.abs() < best.unwrap().1.abs() {
                best = Some((i, offset));
            }
        }
        if let Some((index, offset)) = best {
            let timing = if offset.abs() <= Self::ON_TIME_MS {
                Timing::OnTime
            } else if offset < 0.0 {
                Timing::Early
            } else {
                Timing::Late
            };
            self.resolutions[index] = Some(TempoResolution::Hit {
                timing,
                offset_ms: offset,
            });
            return TempoOutcome::Hit {
                index,
                timing,
                offset_ms: offset,
                exercise_complete: self.is_complete(),
            };
        }

        // Same pitch, already consumed, still near its window: double strike.
        let is_double_strike = self.expected.iter().enumerate().any(|(i, event)| {
            matches!(self.resolutions[i], Some(r) if r != TempoResolution::Missed)
                && event.midi == midi
                && (now_ms - event.target_ms).abs() <= Self::TOLERANCE_MS * 2.0
        });
        if is_double_strike {
            return TempoOutcome::Ignored;
        }

        // Wrong (bad pitch, or right pitch far outside its window): anchor
        // feedback on the temporally nearest unresolved event.
        let nearest = (0..self.expected.len())
            .filter(|&i| self.resolutions[i].is_none())
            .min_by(|&a, &b| {
                (self.expected[a].target_ms - now_ms)
                    .abs()
                    .partial_cmp(&(self.expected[b].target_ms - now_ms).abs())
                    .expect("target offsets are finite")
            })
            .expect("!is_complete guarantees an unresolved event");
        TempoOutcome::Wrong {
            nearest_index: nearest,
            played: midi,
        }
    }

    /// Marks events whose window has closed as missed; returns their indices.
    pub fn sweep(&mut self, now_ms: f64) -> Vec<usize> {
        let mut newly_missed = Vec::new();
        for (i, event) in self.expected.iter().enumerate() {
            if self.resolutions[i].is_none() && now_ms > event.target_ms + Self::TOLERANCE_MS {
                self.resolutions[i] = Some(TempoResolution::Missed);
                newly_missed.push(i);
            }
        }
        newly_missed
    }

    pub fn report(&self) -> TempoReport {
        let mut on_time = 0;
        let mut early = 0;
        let mut late = 0;
        let mut missed = 0;
        let mut offsets: Vec<f64> = Vec::new();
        for resolution in &self.resolutions {
            match resolution {
                Some(TempoResolution::Hit { timing, offset_ms }) => {
                    offsets.push(offset_ms.abs());
                    match timing {
                        Timing::OnTime => on_time += 1,
                        Timing::Early => early += 1,
                        Timing::Late => late += 1,
                    }
                }
                Some(TempoResolution::Missed) => missed += 1,
                None => {}
            }
        }
        TempoReport {
            expected_count: self.expected.len(),
            on_time,
            early,
            late,
            missed,
            mean_abs_offset_ms: if offsets.is_empty() {
                None
            } else {
                Some(offsets.iter().sum::<f64>() / offsets.len() as f64)
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TempoReport {
    pub expected_count: usize,
    pub on_time: usize,
    pub early: usize,
    pub late: usize,
    pub missed: usize,
    pub mean_abs_offset_ms: Option<f64>,
}

impl TempoReport {
    pub fn hit_count(&self) -> usize {
        self.on_time + self.early + self.late
    }

    pub fn hit_rate(&self) -> f64 {
        if self.expected_count == 0 {
            0.0
        } else {
            self.hit_count() as f64 / self.expected_count as f64
        }
    }
}

/// Tempo as an adaptive axis: accuracy first, then speed.
pub struct TempoPolicy;

impl TempoPolicy {
    pub const MIN_BPM: f64 = 48.0;
    pub const MAX_BPM: f64 = 132.0;
    pub const STEP_BPM: f64 = 6.0;
    pub const START_BPM: f64 = 60.0;

    pub fn next(current: f64, report: &TempoReport) -> f64 {
        if report.hit_rate() >= 0.9 && report.missed == 0 {
            return (current + Self::STEP_BPM).min(Self::MAX_BPM);
        }
        if report.hit_rate() < 0.6 {
            return (current - Self::STEP_BPM).max(Self::MIN_BPM);
        }
        current
    }
}

/// Rhythm vocabulary advances on a clean, reasonably fast tempo exercise.
pub struct RhythmPolicy;

impl RhythmPolicy {
    pub const MAX_LEVEL: i32 = 3;

    pub fn should_advance(level: i32, report: &TempoReport, bpm: f64) -> bool {
        level < Self::MAX_LEVEL && report.hit_rate() >= 0.9 && report.missed == 0 && bpm >= 80.0
    }

    pub fn unlock_name(level: i32) -> Option<&'static str> {
        match level {
            1 => Some("dotted half notes"),
            2 => Some("eighth notes"),
            3 => Some("rests"),
            _ => None,
        }
    }
}
