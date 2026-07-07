//! Goertzel-bank note detection for microphone input.
//!
//! Piano targets are fixed equal-temperament frequencies known in
//! advance, so instead of a full spectrum or a generic pitch estimator,
//! each candidate note gets a handful of Goertzel evaluations
//! (fundamental + harmonics) over a short sliding window — cheap,
//! chord-friendly (candidates are independent), and pure Rust on both
//! targets.
//!
//! Detection per analysis frame:
//! - spectral contrast: Σ E(k·f0) against off-frequency probes ±1.5
//!   semitones beside each harmonic — broadband room noise raises both
//!   sides equally, so a true tone stays detectable at poor SNR,
//! - a small absolute share of the window energy as a sanity floor,
//! - sub-octave rejection: a strong half-frequency bin means the *lower*
//!   octave is sounding and this candidate is just its 2nd harmonic,
//! - onset debounce: a hit needs `DEBOUNCE_FRAMES` frames of evidence
//!   (decaying on misses), and releases when the evidence falls away.
//!
//! Sounds the candidates can't explain surface as *uncertain* onsets —
//! the session engine shows its gentle "heard something" notice and
//! never marks them wrong.

/// Window length in samples (~93 ms at 44.1 kHz — resolves low piano
/// notes; a couple of frames of debounce keeps effective latency near
/// the 50–100 ms most tutors run).
pub const WINDOW_SAMPLES: usize = 4096;
/// Consecutive frames of evidence before a note-on fires.
const DEBOUNCE_FRAMES: u32 = 3;
/// Candidate harmonics must carry this many times the energy of the
/// off-frequency probes beside them to count as evidence.
const HIT_CONTRAST: f64 = 4.0;
/// A sounding note releases when its contrast falls below this.
const RELEASE_CONTRAST: f64 = 1.6;
/// Sanity floor: the candidate must still hold this share of the window
/// (rejects contrast flukes in near-silence).
const MIN_SHARE: f64 = 0.01;
/// Off-frequency probe offset (±1.5 semitones — outside the Goertzel
/// main lobe across the trainer's range).
const PROBE_OFFSET: f64 = 1.090508; // 2^(1.5/12)
/// RMS below this is silence (mic noise floor).
const ENERGY_FLOOR: f64 = 1e-4;
/// Sub-octave rejection: half-frequency energy this much above the
/// fundamental means the lower octave is playing.
const SUB_OCTAVE_FACTOR: f64 = 1.2;
/// Harmonics summed per candidate (fundamental + 2f + 3f).
const HARMONICS: usize = 3;
/// Total-energy jump (vs the previous frame) that reads as an attack.
const ONSET_JUMP: f64 = 2.5;
/// Frames of quiet before another uncertain onset may fire.
const UNCERTAIN_COOLDOWN_FRAMES: u32 = 30;

/// Equal-temperament frequency of a MIDI note (A4 = 440 Hz).
pub fn midi_frequency(midi: u8) -> f64 {
    440.0 * ((midi as f64 - 69.0) / 12.0).exp2()
}

/// Normalized signal power at one frequency (Goertzel): mean-square
/// amplitude contribution of the bin, comparable against the window's
/// mean-square total energy.
pub fn goertzel_power(samples: &[f32], sample_rate: f64, frequency: f64) -> f64 {
    let n = samples.len();
    if n == 0 || frequency <= 0.0 || frequency >= sample_rate / 2.0 {
        return 0.0;
    }
    let omega = 2.0 * std::f64::consts::PI * frequency / sample_rate;
    let coefficient = 2.0 * omega.cos();
    let mut s_prev = 0.0f64;
    let mut s_prev2 = 0.0f64;
    for &sample in samples {
        let s = sample as f64 + coefficient * s_prev - s_prev2;
        s_prev2 = s_prev;
        s_prev = s;
    }
    let magnitude_sq =
        s_prev * s_prev + s_prev2 * s_prev2 - coefficient * s_prev * s_prev2;
    // Bin power → mean-square units: |X|² · 2 / N².
    magnitude_sq * 2.0 / (n as f64 * n as f64)
}

/// Diagnostic: a candidate's share of the window energy (the quantity
/// the detector thresholds) — used by the native `--mic-smoke` loopback.
pub fn candidate_ratio(window: &[f32], sample_rate: f64, midi: u8) -> f64 {
    let total = window
        .iter()
        .map(|s| (*s as f64) * (*s as f64))
        .sum::<f64>()
        / window.len().max(1) as f64;
    if total <= 0.0 {
        return 0.0;
    }
    let f0 = midi_frequency(midi);
    let mut energy = 0.0;
    for harmonic in 1..=HARMONICS {
        energy += goertzel_power(window, sample_rate, f0 * harmonic as f64);
    }
    energy / total
}

/// Diagnostic: E(f0/2) / E(f0) — the octave-confusion contrast the
/// detector guards on.
pub fn sub_octave_contrast(window: &[f32], sample_rate: f64, midi: u8) -> f64 {
    let f0 = midi_frequency(midi);
    let fundamental = goertzel_power(window, sample_rate, f0);
    if fundamental <= 0.0 {
        return f64::INFINITY;
    }
    goertzel_power(window, sample_rate, f0 / 2.0) / fundamental
}

/// One detection event out of an analysis frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    /// A candidate note started sounding.
    On(u8),
    /// A sounding candidate stopped.
    Off(u8),
    /// An attack the candidates can't explain (confidence-gated upstream).
    Uncertain,
}

/// Per-candidate debounce/hold state across frames.
#[derive(Debug, Clone, Copy, Default)]
struct CandidateState {
    consecutive: u32,
    sounding: bool,
}

/// The Goertzel bank + onset state machine. Feed it the sliding window
/// each frame with the exercise's current candidate notes.
pub struct GoertzelDetector {
    states: [CandidateState; 128],
    previous_energy: f64,
    uncertain_cooldown: u32,
}

impl Default for GoertzelDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl GoertzelDetector {
    pub fn new() -> Self {
        Self {
            states: [CandidateState::default(); 128],
            previous_energy: 0.0,
            uncertain_cooldown: 0,
        }
    }

    /// Drop all held state (exercise change, backend restart).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Analyze one window; `candidates` are the MIDI notes worth
    /// listening for right now. Returns the events this frame produced.
    pub fn process(
        &mut self,
        window: &[f32],
        sample_rate: f64,
        candidates: &[u8],
    ) -> Vec<Detected> {
        let mut events = Vec::new();
        if window.len() < WINDOW_SAMPLES / 2 {
            return events;
        }
        let total_energy = window
            .iter()
            .map(|s| (*s as f64) * (*s as f64))
            .sum::<f64>()
            / window.len() as f64;
        self.uncertain_cooldown = self.uncertain_cooldown.saturating_sub(1);

        let silent = total_energy < ENERGY_FLOOR;
        let mut any_evidence = false;

        for &midi in candidates {
            let f0 = midi_frequency(midi);
            let mut candidate_energy = 0.0;
            let mut probe_energy = 0.0;
            for harmonic in 1..=HARMONICS {
                let hf = f0 * harmonic as f64;
                candidate_energy += goertzel_power(window, sample_rate, hf);
                probe_energy += (goertzel_power(window, sample_rate, hf / PROBE_OFFSET)
                    + goertzel_power(window, sample_rate, hf * PROBE_OFFSET))
                    / 2.0;
            }
            let contrast = candidate_energy / probe_energy.max(1e-12);
            let share = if total_energy > 0.0 {
                candidate_energy / total_energy
            } else {
                0.0
            };
            // Octave confusion: our f0 may just be the 2nd harmonic of
            // the note an octave below.
            let sub_octave = goertzel_power(window, sample_rate, f0 / 2.0);
            let confused = sub_octave
                > goertzel_power(window, sample_rate, f0) * SUB_OCTAVE_FACTOR;

            let evidence =
                !silent && contrast >= HIT_CONTRAST && share >= MIN_SHARE && !confused;
            let holds = !silent && contrast >= RELEASE_CONTRAST && !confused;
            #[cfg(feature = "detector-trace")]
            if contrast >= RELEASE_CONTRAST {
                eprintln!(
                    "GDBG midi={midi} contrast={contrast:.2} share={share:.4} confused={confused} evidence={evidence}"
                );
            }
            if evidence || (self.states[midi as usize].sounding && holds) {
                any_evidence = true;
            }

            let state = &mut self.states[midi as usize];
            if evidence {
                state.consecutive += 1;
                if state.consecutive >= DEBOUNCE_FRAMES && !state.sounding {
                    state.sounding = true;
                    events.push(Detected::On(midi));
                }
            } else if state.sounding && !holds {
                state.sounding = false;
                state.consecutive = 0;
                events.push(Detected::Off(midi));
            } else if !state.sounding {
                // Decay rather than reset: real-room evidence flickers
                // around the threshold frame to frame.
                state.consecutive = state.consecutive.saturating_sub(1);
            }
        }

        // An attack the candidates don't explain: energy jumped, nothing
        // above matched, and we're not inside the cooldown.
        let onset = total_energy > ENERGY_FLOOR
            && total_energy > self.previous_energy * ONSET_JUMP
            && self.previous_energy > 0.0;
        if onset && !any_evidence && self.uncertain_cooldown == 0 {
            self.uncertain_cooldown = UNCERTAIN_COOLDOWN_FRAMES;
            events.push(Detected::Uncertain);
        }
        self.previous_energy = total_energy.max(1e-12);
        events
    }
}
