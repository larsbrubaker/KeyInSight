//! The normalized input event every backend emits (see the Swift reference
//! `docs/03-architecture.md`). Everything above this seam — matcher,
//! scoring, UI — is input-agnostic.
//!
//! Ports `Core/NoteEvent.swift`.

/// Note-on / note-off discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteEventKind {
    On,
    Off,
}

/// The normalized input event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteEvent {
    pub kind: NoteEventKind,
    /// MIDI note number.
    pub midi: u8,
    pub velocity: Option<u8>,
    /// Host-uptime seconds at capture (the Swift `CACurrentMediaTime`
    /// domain; shells feed a monotonic clock).
    pub timestamp: f64,
    /// 1.0 for MIDI and simulated input; real values arrive with the mic
    /// backend.
    pub confidence: f64,
}

/// A source of NoteEvents: MIDI hardware, mic pitch detection, or the
/// simulated backend (computer keyboard / scripted playback).
///
/// The Swift protocol exposes a settable `onEvent` closure; in Rust the
/// sink is installed through `set_on_event` (shells and the session engine
/// wire it at startup).
pub trait InputBackend {
    fn display_name(&self) -> &str;
    fn set_on_event(&mut self, on_event: Option<Box<dyn FnMut(NoteEvent)>>);
    fn start(&mut self);
    fn stop(&mut self);

    /// Concrete-type escape hatch (the session engine forwards computer
    /// keyboard input to the simulated backend through this).
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}
