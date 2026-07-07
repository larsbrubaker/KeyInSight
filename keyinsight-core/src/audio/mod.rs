//! Audio: pure DSP + timing (YIN pitch detection, the note gate, the SMF
//! encoder, the metronome clock) and the platform output seam.
//!
//! Ports `Sources/KeyInSight/Audio/`. The AVAudioEngine plumbing maps to
//! the [`AudioOut`] trait per `docs/platform-substitutions.md`: cpal on
//! native, WebAudio on WASM, silent in tests/headless (the Swift metronome
//! also kept its clock running when audio was unavailable).

#[cfg(test)]
mod tests;

pub mod goertzel;
mod metronome;
mod midi_file_encoder;
pub mod synth;
mod yin;

pub use goertzel::{goertzel_power, midi_frequency, GoertzelDetector};
pub use metronome::Metronome;
pub use midi_file_encoder::MidiFileEncoder;
pub use synth::{click_samples, parse_smf, render_smf, Clip, SmfNote};
pub use yin::{Detection, GateAction, NoteGate, YinPitchDetector};

/// Platform audio output. All methods are fire-and-forget; failures are the
/// implementation's to log (the app keeps training without sound).
pub trait AudioOut {
    /// Schedule a metronome click at `at_host_seconds` on the shared host
    /// clock (accented on measure starts).
    fn play_click(&self, at_host_seconds: f64, accent: bool);

    /// Start playing a Standard MIDI File; returns false when audio output
    /// is unavailable.
    fn play_smf(&self, smf: &[u8]) -> bool;

    /// Stop any in-progress SMF playback.
    fn stop_smf(&self);
}

/// Silent output — headless runs, tests, and shells before audio lands.
pub struct NullAudioOut;

impl AudioOut for NullAudioOut {
    fn play_click(&self, _at_host_seconds: f64, _accent: bool) {}

    fn play_smf(&self, _smf: &[u8]) -> bool {
        false
    }

    fn stop_smf(&self) {}
}
