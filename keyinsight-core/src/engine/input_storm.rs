//! Mash guard: a burst of wrong strikes is noise — a child, a cat, an
//! elbow — not practice, and it shouldn't poison the skill model's EWMAs.
//! A storm is ≥8 wrong strikes inside 3 seconds; while one is active the
//! engine suspends attempt recording until a clean event signals
//! intentional play again. Raw events still hit the event log.
//!
//! Ports `Engine/InputStormDetector.swift`.

#[derive(Debug, Default, Clone)]
pub struct InputStormDetector {
    wrong_times: Vec<f64>,
}

impl InputStormDetector {
    pub const WINDOW_SECONDS: f64 = 3.0;
    pub const STRIKE_THRESHOLD: usize = 8;

    /// Record a wrong strike; returns true when the storm threshold is met.
    pub fn record_wrong(&mut self, time: f64) -> bool {
        self.wrong_times.push(time);
        self.wrong_times
            .retain(|&t| time - t <= Self::WINDOW_SECONDS);
        self.wrong_times.len() >= Self::STRIKE_THRESHOLD
    }

    pub fn reset(&mut self) {
        self.wrong_times.clear();
    }
}
