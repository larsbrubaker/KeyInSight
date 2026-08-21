//! Skill-item model (pitch items; EWMA mastery).
//!
//! The user never picks a level: when every active item is mastered, the
//! next item in the staff's unlock order joins the active set (keybr's
//! model). Weakness weights feed the generator so weak items are drilled
//! harder.
//!
//! One instance per staff: the treble model drives right-hand content, the
//! bass model left-hand content. Both read the same stats table —
//! staff-prefixed item names keep them separate.
//!
//! Ports `Skill/SkillModel.swift` (pitch items, mastery, keys,
//! progression). The interval ladder, transition backoff, and chord-shape
//! ladder live in the sibling `ladder` module.

use std::collections::{HashMap, HashSet};

use crate::core::PitchSpelling;
use crate::persistence::PitchItemStat;
use crate::score::{PitchOption, Staff};

/// Treble-staff unlock order: seed range C4–G4, then outward expansion
/// alternating up/down (ledger-heavy extremes last), then sharps.
pub const UNLOCK_ORDER: [u8; 20] = [
    60, 62, 64, 65, 67, // seed: C4 D4 E4 F4 G4
    69, 59, 71, 57, 72, 55, 74, 76, 77, 79, // A4 B3 B4 A3 C5 G3 D5 E5 F5 G5
    66, 61, 68, 63, 70, // F#4 C#4 G#4 D#4 A#4
];
/// Bass-staff mirror: seed C3–G3 (the left hand's C position), then
/// outward expansion alternating down/up (low-ledger extremes last), then
/// sharps at the drilled register.
pub const BASS_UNLOCK_ORDER: [u8; 20] = [
    48, 50, 52, 53, 55, // seed: C3 D3 E3 F3 G3
    47, 57, 45, 59, 43, 60, 41, 40, 38, 36, // B2 A3 A2 B3 G2 C4 F2 E2 D2 C2
    54, 49, 56, 51, 58, // F#3 C#3 G#3 D#3 A#3
];
pub const SEED_COUNT: usize = 5;

pub struct Thresholds;

impl Thresholds {
    pub const MIN_ATTEMPTS: i64 = 4;
    pub const MAX_EWMA_ERROR: f64 = 0.2;
    pub const MAX_EWMA_LATENCY_MS: f64 = 2000.0;
    /// Weight above which an item counts as "targeted" for reporting.
    pub const TARGETED_WEIGHT: f64 = 1.5;
}

#[derive(Debug, Clone)]
pub struct ItemState {
    pub midi: u8,
    pub unlocked: bool,
    pub stat: Option<PitchItemStat>,
    pub mastered: bool,
    /// Generator bias: 1.0 = neutral, higher = drill more.
    pub weight: f64,
}

impl ItemState {
    pub fn name(&self) -> String {
        SkillModel::item_name(self.midi)
    }
}

/// Interval items: melodic intervals as *shapes* (signed diatonic deltas).
/// Unison through 4th are always active; larger sizes unlock through the
/// interval ladder (readiness probes, OQ-25).
#[derive(Debug, Clone)]
pub struct IntervalState {
    pub delta: i32,
    pub stat: Option<PitchItemStat>,
    pub weight: f64,
    pub unlocked: bool,
}

impl IntervalState {
    pub fn name(&self) -> String {
        SkillModel::interval_item_name(self.delta)
    }
}

pub const INTERVAL_DELTAS: [i32; 15] = [-7, -6, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 7];

#[derive(Debug, Clone)]
pub struct KeyOption {
    pub fifths: i32,
    /// Selection weight: keys whose signature sharps are weak/new drill
    /// harder.
    pub weight: f64,
}

pub struct SkillModel {
    staff: Staff,
    unlocked_count: usize,
    /// One state per item in the staff's unlock order; refreshed from stats.
    pub states: Vec<ItemState>,
    pub interval_states: Vec<IntervalState>,
    /// All stats by item name from the last refresh (probes, transitions,
    /// chord shapes read from here).
    pub(super) stats_by_name: HashMap<String, PitchItemStat>,
    pub(super) interval_unlocked_count: usize,
    pub(super) chord_unlocked_count: usize,
}

impl Default for SkillModel {
    fn default() -> Self {
        Self::new(SEED_COUNT)
    }
}

impl SkillModel {
    /// Treble-staff model (the Swift default `staff:` argument).
    pub fn new(unlocked_count: usize) -> Self {
        Self::with_staff(Staff::Treble, unlocked_count)
    }

    pub fn with_staff(staff: Staff, unlocked_count: usize) -> Self {
        let mut model = Self {
            staff,
            unlocked_count: unlocked_count.clamp(SEED_COUNT, Self::unlock_order(staff).len()),
            states: Vec::new(),
            interval_states: Vec::new(),
            stats_by_name: HashMap::new(),
            interval_unlocked_count: 0,
            chord_unlocked_count: 0,
        };
        model.refresh(&[]);
        model
    }

    pub fn unlock_order(staff: Staff) -> &'static [u8; 20] {
        match staff {
            Staff::Bass => &BASS_UNLOCK_ORDER,
            Staff::Treble => &UNLOCK_ORDER,
        }
    }

    pub fn staff(&self) -> Staff {
        self.staff
    }

    /// This staff's unlock order.
    pub fn order(&self) -> &'static [u8; 20] {
        Self::unlock_order(self.staff)
    }

    pub fn unlocked_count(&self) -> usize {
        self.unlocked_count
    }

    pub fn item_name(midi: u8) -> String {
        format!("treble:{}", PitchSpelling::name(midi))
    }

    /// Staff-aware item name: each staff's attempts accrue under its own
    /// prefix ("treble:C4" / "bass:C3") — same key, different reading skill.
    pub fn item_name_on(midi: u8, staff: Staff) -> String {
        let staff_name = match staff {
            Staff::Treble => "treble",
            Staff::Bass => "bass",
        };
        format!("{staff_name}:{}", PitchSpelling::name(midi))
    }

    /// "interval:unison", "interval:2nd-up", "interval:octave-down", …
    pub fn interval_item_name(delta: i32) -> String {
        if delta == 0 {
            return "interval:unison".to_string();
        }
        let size = ["", "2nd", "3rd", "4th", "5th", "6th", "7th", "octave"]
            [delta.unsigned_abs().min(7) as usize];
        format!("interval:{size}-{}", if delta > 0 { "up" } else { "down" })
    }

    pub fn refresh(&mut self, stats: &[PitchItemStat]) {
        self.stats_by_name = stats.iter().map(|s| (s.item.clone(), s.clone())).collect();
        let staff = self.staff;
        self.states = self
            .order()
            .iter()
            .enumerate()
            .map(|(index, &midi)| {
                let stat = self
                    .stats_by_name
                    .get(Self::item_name_on(midi, staff).as_str())
                    .cloned();
                ItemState {
                    midi,
                    unlocked: index < self.unlocked_count,
                    mastered: Self::is_mastered(stat.as_ref()),
                    weight: Self::weight(stat.as_ref()),
                    stat,
                }
            })
            .collect();
        let unlocked_sizes = self.unlocked_interval_sizes();
        self.interval_states = INTERVAL_DELTAS
            .iter()
            .map(|&delta| {
                let stat = self
                    .stats_by_name
                    .get(Self::interval_item_name(delta).as_str())
                    .cloned();
                // Unseen intervals are not "frontier" the way unseen pitches
                // are (unison/steps appear constantly): neutral until data
                // exists.
                let weight = match &stat {
                    None => 1.0,
                    Some(s) => Self::weight(Some(s)),
                };
                IntervalState {
                    delta,
                    stat,
                    weight,
                    unlocked: unlocked_sizes.contains(&delta.abs()),
                }
            })
            .collect();
    }

    /// Generator bias per signed diatonic move.
    pub fn interval_weights(&self) -> HashMap<i32, f64> {
        self.interval_states
            .iter()
            .map(|s| (s.delta, s.weight))
            .collect()
    }

    // --- Mastery & weakness ---

    pub fn is_mastered(stat: Option<&PitchItemStat>) -> bool {
        let Some(stat) = stat else { return false };
        if stat.attempts < Thresholds::MIN_ATTEMPTS {
            return false;
        }
        if stat.ewma_error > Thresholds::MAX_EWMA_ERROR {
            return false;
        }
        if let Some(latency) = stat.ewma_latency_ms {
            if latency > Thresholds::MAX_EWMA_LATENCY_MS {
                return false;
            }
        }
        true
    }

    /// Never-seen items are strongly biased (they are the learning
    /// frontier); seen items scale with EWMA error plus a slow-response
    /// penalty.
    pub fn weight(stat: Option<&PitchItemStat>) -> f64 {
        let Some(stat) = stat else { return 2.5 };
        if stat.attempts == 0 {
            return 2.5;
        }
        let latency_penalty = stat
            .ewma_latency_ms
            .map(|l| ((l - 800.0) / 2000.0).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        (1.0 + 3.0 * stat.ewma_error + latency_penalty).max(0.8)
    }

    // --- Derived sets ---

    pub fn active_states(&self) -> Vec<&ItemState> {
        self.states.iter().filter(|s| s.unlocked).collect()
    }

    pub fn all_active_mastered(&self) -> bool {
        let active = self.active_states();
        !active.is_empty() && active.iter().all(|s| s.mastered)
    }

    pub fn next_locked_midi(&self) -> Option<u8> {
        self.order().get(self.unlocked_count).copied()
    }

    /// Average weakness across the active set (1.0 = all neutral). The auto
    /// hand rotation drills the weaker hand more.
    pub fn mean_active_weight(&self) -> f64 {
        let weights: Vec<f64> = self.active_states().iter().map(|s| s.weight).collect();
        if weights.is_empty() {
            1.0
        } else {
            weights.iter().sum::<f64>() / weights.len() as f64
        }
    }

    pub fn active_pitch_options(&self) -> Vec<PitchOption> {
        self.active_states()
            .iter()
            .map(|s| PitchOption::weighted(s.midi, s.weight))
            .collect()
    }

    /// Item names currently being drilled hardest (for the exercise record).
    pub fn targeted_item_names(&self) -> Vec<String> {
        let mut targeted: Vec<&ItemState> = self
            .active_states()
            .into_iter()
            .filter(|s| s.weight >= Thresholds::TARGETED_WEIGHT)
            .collect();
        targeted.sort_by(|a, b| b.weight.partial_cmp(&a.weight).expect("finite weights"));
        targeted.into_iter().take(3).map(|s| s.name()).collect()
    }

    // --- Key signatures (introduced as patterns, not theory) ---

    /// Diatonic pitch classes for sharp keys up to 2 sharps (C, G, D).
    pub fn diatonic_pitch_classes(fifths: i32) -> HashSet<i32> {
        let mut pcs: HashSet<i32> = [0, 2, 4, 5, 7, 9, 11].into_iter().collect(); // C major
        if fifths >= 1 {
            pcs.remove(&5);
            pcs.insert(6); // F → F#
        }
        if fifths >= 2 {
            pcs.remove(&0);
            pcs.insert(1); // C → C#
        }
        pcs
    }

    /// The signature sharps at this staff's drilled register
    /// (F#4/C#4 treble, F#3/C#3 bass).
    fn f_sharp_midi(&self) -> u8 {
        if self.staff == Staff::Bass {
            54
        } else {
            66
        }
    }

    fn c_sharp_midi(&self) -> u8 {
        if self.staff == Staff::Bass {
            49
        } else {
            61
        }
    }

    /// A key becomes available once its signature sharps are unlocked
    /// items — then exercises in that key drill the sharp at its staff
    /// position with the signature carrying the pattern (no accidental per
    /// note).
    pub fn available_keys(&self) -> Vec<KeyOption> {
        let unlocked_midis: HashSet<u8> = self.active_states().iter().map(|s| s.midi).collect();
        let f_sharp = self.f_sharp_midi();
        let c_sharp = self.c_sharp_midi();
        let mut keys = vec![KeyOption {
            fifths: 0,
            weight: 1.0,
        }];
        if unlocked_midis.contains(&f_sharp) {
            keys.push(KeyOption {
                fifths: 1,
                weight: self.key_weight(&[f_sharp]),
            });
        }
        if unlocked_midis.contains(&f_sharp) && unlocked_midis.contains(&c_sharp) {
            keys.push(KeyOption {
                fifths: 2,
                weight: self.key_weight(&[f_sharp, c_sharp]),
            });
        }
        keys
    }

    fn key_weight(&self, sharps: &[u8]) -> f64 {
        let weights: Vec<f64> = self
            .states
            .iter()
            .filter(|s| sharps.contains(&s.midi))
            .map(|s| s.weight)
            .collect();
        1.0 + weights.iter().sum::<f64>() / weights.len().max(1) as f64
    }

    /// Active pitches restricted to a key's diatonic set.
    pub fn active_pitch_options_in_key(&self, fifths: i32) -> Vec<PitchOption> {
        let pcs = Self::diatonic_pitch_classes(fifths);
        let options: Vec<PitchOption> = self
            .active_states()
            .iter()
            .filter(|s| pcs.contains(&((s.midi % 12) as i32)))
            .map(|s| PitchOption::weighted(s.midi, s.weight))
            .collect();
        // A key must leave a workable range; fall back to the full C-major set.
        if options.len() >= 3 {
            options
        } else {
            self.active_pitch_options()
        }
    }

    // --- Progression ---

    /// Unlocks the next item if every active item is mastered.
    /// Caller persists `unlocked_count` and re-refreshes.
    pub fn unlock_if_earned(&mut self) -> Option<u8> {
        if !self.all_active_mastered() || self.unlocked_count >= self.order().len() {
            return None;
        }
        self.unlocked_count += 1;
        Some(self.order()[self.unlocked_count - 1])
    }

    pub fn set_unlocked_count(&mut self, count: usize) {
        self.unlocked_count = count.clamp(SEED_COUNT, self.order().len());
    }
}
