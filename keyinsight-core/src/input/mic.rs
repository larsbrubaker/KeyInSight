//! The microphone input backend: platform-captured samples through the
//! Goertzel bank (`audio::goertzel`) into the normalized NoteEvent
//! stream.
//!
//! The platform shells own the device (cpal input stream on native,
//! getUserMedia on the web) behind [`MicSource`]; the backend drains
//! captured samples once per engine tick, analyzes a sliding window
//! against the exercise's current candidate notes, and emits:
//! - confident `On`/`Off` events for detected candidates (chords work —
//!   candidates are independent), and
//! - a low-confidence event for attacks the candidates can't explain,
//!   which the engine's confidence gate turns into the gentle
//!   "heard something" notice (never a wrong mark).

use std::rc::Rc;

use crate::audio::goertzel::{Detected, GoertzelDetector, WINDOW_SAMPLES};
use crate::core::{InputBackend, NoteEvent, NoteEventKind};

/// Platform microphone capture, pull model: the shell buffers samples
/// from its capture callback; the backend drains them per frame.
pub trait MicSource {
    /// Begin capture (this is where the permission prompt happens on the
    /// web). Returns false when no input device is available.
    fn start(&self) -> bool;
    fn stop(&self);
    /// Device sample rate; valid after `start`.
    fn sample_rate(&self) -> f64;
    /// Move samples captured since the last call into `out` (mono f32).
    fn drain(&self, out: &mut Vec<f32>);
}

/// Analyze only after this many fresh samples (~46 ms at 44.1 kHz) — the
/// window barely moves faster than that, and it caps the Goertzel cost.
const HOP_SAMPLES: usize = 2048;

pub struct MicBackend {
    source: Rc<dyn MicSource>,
    on_event: Option<Box<dyn FnMut(NoteEvent)>>,
    detector: GoertzelDetector,
    /// Sliding analysis window (last `WINDOW_SAMPLES`).
    window: Vec<f32>,
    /// Fresh samples since the last analysis.
    pending: usize,
    scratch: Vec<f32>,
    /// Smoothed input level for the side panel meter (0..1).
    level: f64,
    running: bool,
}

impl MicBackend {
    pub fn new(source: Rc<dyn MicSource>) -> Self {
        Self {
            source,
            on_event: None,
            detector: GoertzelDetector::new(),
            window: Vec::with_capacity(WINDOW_SAMPLES),
            pending: 0,
            scratch: Vec::new(),
            level: 0.0,
            running: false,
        }
    }

    /// Smoothed mic level for the UI meter.
    pub fn level(&self) -> f64 {
        self.level
    }

    /// Drain captured audio and run detection against `candidates`.
    /// Called once per engine tick.
    pub fn process(&mut self, now: f64, candidates: &[u8]) {
        if !self.running {
            return;
        }
        self.scratch.clear();
        self.source.drain(&mut self.scratch);
        if !self.scratch.is_empty() {
            // Meter: peak-ish RMS of the fresh chunk, smoothed.
            let rms = (self
                .scratch
                .iter()
                .map(|s| (*s as f64) * (*s as f64))
                .sum::<f64>()
                / self.scratch.len() as f64)
                .sqrt();
            self.level = 0.7 * self.level + 0.3 * (rms * 6.0).min(1.0);

            self.window.extend_from_slice(&self.scratch);
            let excess = self.window.len().saturating_sub(WINDOW_SAMPLES);
            if excess > 0 {
                self.window.drain(..excess);
            }
            self.pending += self.scratch.len();
        }
        if self.pending < HOP_SAMPLES {
            return;
        }
        self.pending = 0;

        let events =
            self.detector
                .process(&self.window, self.source.sample_rate(), candidates);
        let Some(on_event) = &mut self.on_event else {
            return;
        };
        for detected in events {
            let event = match detected {
                Detected::On(midi) => NoteEvent {
                    kind: NoteEventKind::On,
                    midi,
                    velocity: Some(80),
                    timestamp: now,
                    confidence: 1.0,
                },
                Detected::Off(midi) => NoteEvent {
                    kind: NoteEventKind::Off,
                    midi,
                    velocity: None,
                    timestamp: now,
                    confidence: 1.0,
                },
                Detected::Uncertain => NoteEvent {
                    kind: NoteEventKind::On,
                    midi: 0,
                    velocity: None,
                    timestamp: now,
                    confidence: 0.0,
                },
            };
            on_event(event);
        }
    }
}

impl InputBackend for MicBackend {
    fn display_name(&self) -> &str {
        "Microphone"
    }

    fn set_on_event(&mut self, on_event: Option<Box<dyn FnMut(NoteEvent)>>) {
        self.on_event = on_event;
    }

    fn start(&mut self) {
        self.detector.reset();
        self.window.clear();
        self.pending = 0;
        self.level = 0.0;
        self.running = self.source.start();
    }

    fn stop(&mut self) {
        if self.running {
            self.source.stop();
            self.running = false;
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
