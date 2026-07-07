//! Simulated input backend: maps a computer-keyboard row to piano keys so
//! development and UI iteration never block on MIDI hardware.
//!
//!   A S D F G H J K  →  C4 D4 E4 F4 G4 A4 B4 C5   (white keys)
//!   W E   T Y U      →  C#4 D#4  F#4 G#4 A#4      (black keys)
//!   Z / X            →  shift the whole mapping an octave down / up
//!
//! Ports `Input/SimulatedKeyboardBackend.swift`. The NSEvent local monitor
//! maps to agg-gui key events: the app widget forwards key downs/ups into
//! [`SimulatedKeyboardBackend::handle_key`]; text-field focus suppression
//! is the caller's job (agg-gui reports focus, the shim skips forwarding).

use std::collections::HashMap;

use crate::core::{InputBackend, NoteEvent, NoteEventKind};

pub struct SimulatedKeyboardBackend {
    on_event: Option<Box<dyn FnMut(NoteEvent)>>,
    pub on_octave_change: Option<Box<dyn FnMut(i32)>>,
    /// Octaves relative to the base mapping (C4–C5).
    octave_offset: i32,
    /// Held key → the midi note it sounded (so a note-off matches its
    /// note-on even if the octave shifts while held).
    held_keys: HashMap<char, u8>,
    running: bool,
}

fn key_map(ch: char) -> Option<u8> {
    match ch {
        'a' => Some(60),
        'w' => Some(61),
        's' => Some(62),
        'e' => Some(63),
        'd' => Some(64),
        'f' => Some(65),
        't' => Some(66),
        'g' => Some(67),
        'y' => Some(68),
        'h' => Some(69),
        'u' => Some(70),
        'j' => Some(71),
        'k' => Some(72),
        _ => None,
    }
}

impl Default for SimulatedKeyboardBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulatedKeyboardBackend {
    pub fn new() -> Self {
        Self {
            on_event: None,
            on_octave_change: None,
            octave_offset: 0,
            held_keys: HashMap::new(),
            running: false,
        }
    }

    pub fn octave_offset(&self) -> i32 {
        self.octave_offset
    }

    /// Feed a keyboard event. `timestamp` is host-uptime seconds (the same
    /// clock every NoteEvent carries). Returns true when the event was
    /// consumed (a mapped piano key or the octave shifters).
    pub fn handle_key(&mut self, ch: char, is_down: bool, is_repeat: bool, timestamp: f64) -> bool {
        if !self.running {
            return false;
        }
        let ch = ch.to_ascii_lowercase();

        if ch == 'z' || ch == 'x' {
            if is_down && !is_repeat {
                self.octave_offset =
                    (self.octave_offset + if ch == 'x' { 1 } else { -1 }).clamp(-2, 2);
                if let Some(on_octave_change) = &mut self.on_octave_change {
                    on_octave_change(self.octave_offset);
                }
            }
            return true;
        }
        let Some(base) = key_map(ch) else { return false };
        let midi = (base as i32 + self.octave_offset * 12).clamp(0, 127) as u8;

        if is_down {
            if is_repeat || self.held_keys.contains_key(&ch) {
                return true;
            }
            self.held_keys.insert(ch, midi);
            self.emit(NoteEventKind::On, midi, timestamp);
        } else if let Some(sounded) = self.held_keys.remove(&ch) {
            self.emit(NoteEventKind::Off, sounded, timestamp);
        }
        true
    }

    fn emit(&mut self, kind: NoteEventKind, midi: u8, timestamp: f64) {
        if let Some(on_event) = &mut self.on_event {
            on_event(NoteEvent {
                kind,
                midi,
                velocity: if kind == NoteEventKind::On {
                    Some(80)
                } else {
                    None
                },
                timestamp,
                confidence: 1.0,
            });
        }
    }
}

impl InputBackend for SimulatedKeyboardBackend {
    fn display_name(&self) -> &str {
        "Computer keyboard (simulated)"
    }

    fn set_on_event(&mut self, on_event: Option<Box<dyn FnMut(NoteEvent)>>) {
        self.on_event = on_event;
    }

    fn start(&mut self) {
        self.running = true;
    }

    fn stop(&mut self) {
        self.running = false;
        self.held_keys.clear();
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn backend_with_sink() -> (SimulatedKeyboardBackend, Rc<RefCell<Vec<NoteEvent>>>) {
        let events: Rc<RefCell<Vec<NoteEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        let mut backend = SimulatedKeyboardBackend::new();
        backend.set_on_event(Some(Box::new(move |e| sink.borrow_mut().push(e))));
        backend.start();
        (backend, events)
    }

    #[test]
    fn maps_home_row_to_c_major() {
        let (mut backend, events) = backend_with_sink();
        assert!(backend.handle_key('a', true, false, 0.0));
        assert!(backend.handle_key('a', false, false, 0.1));
        let events = events.borrow();
        assert_eq!(events.len(), 2);
        assert_eq!((events[0].kind, events[0].midi), (NoteEventKind::On, 60));
        assert_eq!(events[0].velocity, Some(80));
        assert_eq!((events[1].kind, events[1].midi), (NoteEventKind::Off, 60));
    }

    #[test]
    fn octave_shift_applies_and_clamps() {
        let (mut backend, events) = backend_with_sink();
        backend.handle_key('x', true, false, 0.0);
        assert_eq!(backend.octave_offset(), 1);
        backend.handle_key('a', true, false, 0.1);
        assert_eq!(events.borrow()[0].midi, 72);
        for _ in 0..5 {
            backend.handle_key('z', true, false, 0.2);
        }
        assert_eq!(backend.octave_offset(), -2);
    }

    #[test]
    fn note_off_matches_its_note_on_across_octave_shift() {
        let (mut backend, events) = backend_with_sink();
        backend.handle_key('a', true, false, 0.0); // C4 on
        backend.handle_key('x', true, false, 0.1); // shift up while held
        backend.handle_key('a', false, false, 0.2); // off must still be C4
        let events = events.borrow();
        assert_eq!(events[1].midi, 60);
    }

    #[test]
    fn repeats_and_unmapped_keys_ignored() {
        let (mut backend, events) = backend_with_sink();
        backend.handle_key('a', true, false, 0.0);
        backend.handle_key('a', true, true, 0.1); // auto-repeat
        assert!(!backend.handle_key('q', true, false, 0.2)); // unmapped
        assert_eq!(events.borrow().len(), 1);
    }

    #[test]
    fn stop_releases_held_keys_and_mutes() {
        let (mut backend, events) = backend_with_sink();
        backend.handle_key('a', true, false, 0.0);
        backend.stop();
        assert!(!backend.handle_key('s', true, false, 0.1));
        assert_eq!(events.borrow().len(), 1);
    }
}
