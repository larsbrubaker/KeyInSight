//! RubricNet-style interpretable difficulty descriptors, computed per
//! generated exercise. Stored with the exercise so the difficulty scale can
//! be calibrated later without regenerating anything.
//!
//! Ports `Score/DifficultyDescriptors.swift`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use crate::core::PitchSpelling;
use crate::score::Exercise;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DifficultyDescriptors {
    #[serde(rename = "rangeSemitones")]
    pub range_semitones: i32,
    /// Shannon entropy of the pitch distribution, bits.
    #[serde(rename = "pitchEntropyBits")]
    pub pitch_entropy_bits: f64,
    #[serde(rename = "notesPerMeasure")]
    pub notes_per_measure: f64,
    /// Share of melodic intervals larger than a step (≥ a 3rd).
    #[serde(rename = "leapRatio")]
    pub leap_ratio: f64,
    /// 1 − unique pitch bigrams / total bigrams (0 = never repeats a move).
    pub repetitiveness: f64,
}

impl DifficultyDescriptors {
    pub fn compute(exercise: &Exercise) -> DifficultyDescriptors {
        let midis: Vec<i32> = exercise
            .sounded_notes()
            .iter()
            .map(|n| n.midi.expect("sounded notes always carry a pitch") as i32)
            .collect();
        let range =
            midis.iter().max().copied().unwrap_or(0) - midis.iter().min().copied().unwrap_or(0);

        let mut histogram: BTreeMap<i32, i32> = BTreeMap::new();
        for &midi in &midis {
            *histogram.entry(midi).or_insert(0) += 1;
        }
        let n = midis.len() as f64;
        let entropy = -histogram.values().fold(0.0, |acc, &count| {
            let p = count as f64 / n;
            acc + p * p.log2()
        });

        let measure_count = exercise.measures().len().max(1);

        let mut leaps = 0;
        let mut intervals = 0;
        let mut bigrams: HashSet<i32> = HashSet::new();
        for pair in midis.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            intervals += 1;
            let diatonic =
                (PitchSpelling::diatonic_index(b as u8) - PitchSpelling::diatonic_index(a as u8)).abs();
            if diatonic >= 2 {
                leaps += 1;
            }
            bigrams.insert(a * 128 + b);
        }

        DifficultyDescriptors {
            range_semitones: range,
            pitch_entropy_bits: entropy,
            notes_per_measure: midis.len() as f64 / measure_count as f64,
            leap_ratio: if intervals == 0 {
                0.0
            } else {
                leaps as f64 / intervals as f64
            },
            repetitiveness: if intervals == 0 {
                0.0
            } else {
                1.0 - bigrams.len() as f64 / intervals as f64
            },
        }
    }

    /// Sorted-keys JSON, mirroring the Swift `JSONEncoder` with
    /// `.sortedKeys` (used by persistence).
    pub fn json(&self) -> String {
        // serde_json with a BTreeMap intermediary guarantees sorted keys.
        let value = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        match value {
            serde_json::Value::Object(map) => {
                let sorted: BTreeMap<String, serde_json::Value> = map.into_iter().collect();
                serde_json::to_string(&sorted).unwrap_or_else(|_| "{}".to_string())
            }
            _ => "{}".to_string(),
        }
    }
}
