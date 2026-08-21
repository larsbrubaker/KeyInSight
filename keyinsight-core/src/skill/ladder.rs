//! Progression ladders on top of the pitch items (OQ-25): the interval
//! ladder (readiness probes), pair-level transition backoff, and the
//! chord-shape ladder with its classifier.
//!
//! Ports the matching sections of `Skill/SkillModel.swift`.

use std::collections::{HashMap, HashSet};

use crate::core::PitchSpelling;
use crate::persistence::PitchItemStat;

use super::skill_model::SkillModel;

// --- Interval ladder (OQ-25) ---

/// Move sizes beyond the base ±4th, in unlock order: 5ths, 6ths, octaves
/// (visually easy — same letter), 7ths last (rarest read).
pub const INTERVAL_SIZE_LADDER: [i32; 4] = [4, 5, 7, 6];
pub const BASE_INTERVAL_SIZES: [i32; 4] = [0, 1, 2, 3];

/// Laxer gates than pitch items (OQ-25 decided): interval data only
/// accrues on monophonic transitions, so pitch-grade gates would take
/// weeks. Probes must have landed in BOTH directions.
pub struct IntervalThresholds;

impl IntervalThresholds {
    pub const MIN_ATTEMPTS_PER_DIRECTION: i64 = 2;
    pub const MAX_EWMA_ERROR: f64 = 0.35;
}

// --- Transition bias (OQ-25 backoff) ---

/// Pairs need this many attempts before they may correct the shape-level
/// prior (sparse data stays reporting-only).
pub const TRANSITION_BIAS_ATTEMPTS_FLOOR: i64 = 6;

// --- Chord shape items (OQ-23/25) ---

/// Shape-classes, not root-specific chords — pitch items keep carrying
/// staff position, so the item count stays small. Unlock order: open 5ths
/// (the hand frame), 3rds, 6ths, octaves, then triads.
pub const CHORD_SHAPE_LADDER: [&str; 5] = [
    "chord:harm-5th",
    "chord:harm-3rd",
    "chord:harm-6th",
    "chord:harm-octave",
    "chord:triad",
];

pub struct ChordThresholds;

impl ChordThresholds {
    pub const MIN_ATTEMPTS: i64 = 4;
    pub const MAX_EWMA_ERROR: f64 = 0.3;
}

impl SkillModel {
    // --- Interval ladder ---

    pub fn interval_unlocked_count(&self) -> usize {
        self.interval_unlocked_count
    }

    pub fn unlocked_interval_sizes(&self) -> HashSet<i32> {
        BASE_INTERVAL_SIZES
            .iter()
            .chain(INTERVAL_SIZE_LADDER[..self.interval_unlocked_count].iter())
            .copied()
            .collect()
    }

    /// The next size to probe (sparse injections while still locked).
    pub fn next_locked_interval_size(&self) -> Option<i32> {
        INTERVAL_SIZE_LADDER
            .get(self.interval_unlocked_count)
            .copied()
    }

    pub fn set_interval_unlocked_count(&mut self, count: usize) {
        self.interval_unlocked_count = count.min(INTERVAL_SIZE_LADDER.len());
    }

    /// Probe stats earned the next size: tried both directions, low error.
    pub fn unlock_interval_if_ready(&mut self) -> Option<i32> {
        let size = self.next_locked_interval_size()?;
        for delta in [size, -size] {
            let stat = self.stats_by_name.get(&Self::interval_item_name(delta))?;
            if stat.attempts < IntervalThresholds::MIN_ATTEMPTS_PER_DIRECTION
                || stat.ewma_error > IntervalThresholds::MAX_EWMA_ERROR
            {
                return None;
            }
        }
        self.interval_unlocked_count += 1;
        Some(size)
    }

    /// "5th", "octave", … for unlock messages.
    pub fn interval_size_display_name(size: i32) -> &'static str {
        ["unison", "2nd", "3rd", "4th", "5th", "6th", "7th", "octave"][size.clamp(0, 7) as usize]
    }

    // --- Transition items ---

    /// Specific transition ("move:F#4>B4") — trouble spots that interval
    /// shapes wash out (a black-to-white geography jump reads differently
    /// from a generic "up a 3rd"). Reporting first; generator bias only
    /// once a pair has enough attempts (OQ-25).
    pub fn transition_item_name(from: u8, to: u8) -> String {
        format!(
            "move:{}>{}",
            PitchSpelling::name(from),
            PitchSpelling::name(to)
        )
    }

    /// Pair key shared with the generator (`core::transition_key`).
    pub fn transition_key(from: u8, to: u8) -> i32 {
        crate::core::transition_key(from, to)
    }

    /// Pair-level weakness weights among the given pitches, floor-gated —
    /// the bigram correction on top of the interval-shape prior.
    pub fn transition_weights(&self, midis: &[u8]) -> HashMap<i32, f64> {
        let mut result: HashMap<i32, f64> = HashMap::new();
        for &from in midis {
            for &to in midis {
                if to == from {
                    continue;
                }
                let Some(stat) = self
                    .stats_by_name
                    .get(&Self::transition_item_name(from, to))
                else {
                    continue;
                };
                if stat.attempts < TRANSITION_BIAS_ATTEMPTS_FLOOR {
                    continue;
                }
                result.insert(Self::transition_key(from, to), Self::weight(Some(stat)));
            }
        }
        result
    }

    // --- Chord shapes ---

    pub fn chord_unlocked_count(&self) -> usize {
        self.chord_unlocked_count
    }

    pub fn unlocked_chord_shapes(&self) -> &'static [&'static str] {
        &CHORD_SHAPE_LADDER[..self.chord_unlocked_count]
    }

    pub fn next_locked_chord_shape(&self) -> Option<&'static str> {
        CHORD_SHAPE_LADDER.get(self.chord_unlocked_count).copied()
    }

    pub fn set_chord_unlocked_count(&mut self, count: usize) {
        self.chord_unlocked_count = count.min(CHORD_SHAPE_LADDER.len());
    }

    pub fn unlock_chord_if_ready(&mut self) -> Option<&'static str> {
        let shape = self.next_locked_chord_shape()?;
        let stat = self.stats_by_name.get(shape)?;
        if stat.attempts < ChordThresholds::MIN_ATTEMPTS
            || stat.ewma_error > ChordThresholds::MAX_EWMA_ERROR
        {
            return None;
        }
        self.chord_unlocked_count += 1;
        Some(shape)
    }

    /// Generation weight for an unlocked shape (weak shapes drill more).
    pub fn chord_shape_weight(&self, name: &str) -> f64 {
        Self::weight(self.stats_by_name.get(name))
    }

    pub fn chord_shape_stat(&self, name: &str) -> Option<&PitchItemStat> {
        self.stats_by_name.get(name)
    }

    /// Classify a struck set into its shape item by diatonic spans from
    /// the bottom note; `None` for shapes the ladder doesn't track.
    pub fn chord_shape_name(pitches: &[u8]) -> Option<&'static str> {
        let mut sorted = pitches.to_vec();
        sorted.sort_unstable();
        if sorted.len() <= 1 {
            return None;
        }
        let bottom = PitchSpelling::diatonic_index(sorted[0]);
        let spans: Vec<i32> = sorted[1..]
            .iter()
            .map(|&p| PitchSpelling::diatonic_index(p) - bottom)
            .collect();
        if spans == [2, 4] {
            return Some("chord:triad");
        }
        if spans.len() != 1 {
            return None;
        }
        match spans[0] {
            2 => Some("chord:harm-3rd"),
            4 => Some("chord:harm-5th"),
            5 => Some("chord:harm-6th"),
            7 => Some("chord:harm-octave"),
            _ => None,
        }
    }

    pub fn chord_shape_display_name(name: &str) -> String {
        match name {
            "chord:harm-5th" => "harmonic 5ths",
            "chord:harm-3rd" => "harmonic 3rds",
            "chord:harm-6th" => "harmonic 6ths",
            "chord:harm-octave" => "octaves",
            "chord:triad" => "triads",
            other => other,
        }
        .to_string()
    }
}
