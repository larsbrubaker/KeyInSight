//! The MIDI input backend: raw bytes from the platform's MIDI ports
//! through a running-status-aware parser into the normalized NoteEvent
//! stream.
//!
//! Ports `Input/MIDIBackend.swift`. The Swift app leaned on SwiftMIDI's
//! event receiver (`translateMIDI1NoteOnZeroVelocityToNoteOff`,
//! `filterActiveSensingAndClock`, packet timestamps); here the platform
//! shells own the ports behind [`MidiPorts`] (midir on native, Web MIDI in
//! the browser — `docs/platform-substitutions.md`) and hand over raw
//! packets with their capture timestamps, and [`MidiParser`] does the
//! translation the Swift receiver options did. Rhythm scoring uses the
//! packet timestamp, never the tick's wall-clock.

use std::rc::Rc;

use crate::core::{InputBackend, NoteEvent, NoteEventKind};

/// Platform MIDI ports, pull model: the shell subscribes to every input
/// port and buffers packets from its driver callbacks; the backend drains
/// them once per engine tick.
pub trait MidiPorts {
    /// Open the MIDI client and subscribe to all input ports (on the web
    /// this is where the permission prompt happens). Returns false when
    /// MIDI is unavailable on this platform.
    fn start(&self) -> bool;
    fn stop(&self);
    /// Names of the currently connected input ports.
    fn port_names(&self) -> Vec<String>;
    /// Move packets captured since the last call into `out`: raw bytes +
    /// the capture timestamp in host seconds.
    fn drain(&self, out: &mut Vec<(Vec<u8>, f64)>);
    /// Rescan for hot-plugged / unplugged ports; true when the set of
    /// ports changed since the last poll.
    fn poll_devices(&self) -> bool;
}

/// MIDI 1.0 byte-stream parser: channel-voice messages with running
/// status, System Common / Real-Time handling, note-on-velocity-0 =
/// note-off. Channel is ignored (a piano is a piano).
#[derive(Debug, Default)]
pub struct MidiParser {
    /// The last channel-voice status byte (running status).
    running_status: Option<u8>,
    /// Data bytes collected for the message in progress.
    pending: Vec<u8>,
}

impl MidiParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget running status and any half-received message (port
    /// reconnect, backend restart).
    pub fn reset(&mut self) {
        self.running_status = None;
        self.pending.clear();
    }

    /// Parse `bytes` (one packet or any slice of the stream), appending
    /// the note events it completes to `out`, all stamped `at`.
    pub fn feed(&mut self, bytes: &[u8], at: f64, out: &mut Vec<NoteEvent>) {
        for &byte in bytes {
            match byte {
                // System Real-Time: may interleave anywhere and never
                // disturbs running status. Clock (0xF8) and active sensing
                // (0xFE) are the chatter the Swift receiver filtered; the
                // others (start/continue/stop/reset) carry no notes.
                0xF8..=0xFF => {}
                // System Common (and SysEx start/end): no running status
                // afterwards, and any data bytes that follow are theirs,
                // not ours (they're ignored until the next status byte).
                0xF0..=0xF7 => {
                    self.running_status = None;
                    self.pending.clear();
                }
                // Channel voice status: a new message starts (a status
                // arriving mid-message resyncs on it).
                0x80..=0xEF => {
                    self.running_status = Some(byte);
                    self.pending.clear();
                }
                // Data byte.
                _ => {
                    let Some(status) = self.running_status else {
                        // Stray data with no status to belong to.
                        continue;
                    };
                    self.pending.push(byte);
                    if self.pending.len() == Self::data_len(status) {
                        self.dispatch(status, at, out);
                        self.pending.clear();
                    }
                }
            }
        }
    }

    /// Data bytes per channel-voice message type.
    fn data_len(status: u8) -> usize {
        match status & 0xF0 {
            0xC0 | 0xD0 => 1,
            _ => 2,
        }
    }

    fn dispatch(&self, status: u8, at: f64, out: &mut Vec<NoteEvent>) {
        let midi = self.pending[0];
        match status & 0xF0 {
            0x90 => {
                let velocity = self.pending[1];
                if velocity > 0 {
                    out.push(Self::event(NoteEventKind::On, midi, Some(velocity), at));
                } else {
                    // Note-on with velocity 0 is a note-off.
                    out.push(Self::event(NoteEventKind::Off, midi, None, at));
                }
            }
            0x80 => out.push(Self::event(NoteEventKind::Off, midi, None, at)),
            // Aftertouch, control change, program, pressure, pitch bend:
            // consumed, nothing to emit.
            _ => {}
        }
    }

    fn event(kind: NoteEventKind, midi: u8, velocity: Option<u8>, at: f64) -> NoteEvent {
        NoteEvent {
            kind,
            midi,
            velocity,
            timestamp: at,
            confidence: 1.0,
        }
    }
}

/// How often the backend asks the ports to rescan for hot-plug changes.
const RESCAN_INTERVAL: f64 = 1.0;

pub struct MidiBackend {
    ports: Rc<dyn MidiPorts>,
    parser: MidiParser,
    on_event: Option<Box<dyn FnMut(NoteEvent)>>,
    scratch: Vec<(Vec<u8>, f64)>,
    events: Vec<NoteEvent>,
    running: bool,
    device_label: String,
    /// Host time of the next device rescan.
    rescan_due: f64,
}

impl MidiBackend {
    pub fn new(ports: Rc<dyn MidiPorts>) -> Self {
        Self {
            ports,
            parser: MidiParser::new(),
            on_event: None,
            scratch: Vec::new(),
            events: Vec::new(),
            running: false,
            device_label: Self::IDLE_LABEL.to_string(),
            rescan_due: 0.0,
        }
    }

    const IDLE_LABEL: &'static str = "MIDI keyboard";

    /// What's plugged in, for a status line: the idle name before start,
    /// then the device name, "N MIDI devices", or "No MIDI device".
    pub fn device_label(&self) -> &str {
        &self.device_label
    }

    fn refresh_label(&mut self) {
        let names = self.ports.port_names();
        self.device_label = match names.len() {
            0 => "No MIDI device".to_string(),
            1 => names.into_iter().next().expect("one name"),
            n => format!("{n} MIDI devices"),
        };
    }

    /// Drain captured packets into note events. Called once per engine
    /// tick; `now` only paces the hot-plug rescan (events carry their
    /// packet timestamps).
    pub fn process(&mut self, now: f64) {
        if !self.running {
            return;
        }
        if now >= self.rescan_due {
            self.rescan_due = now + RESCAN_INTERVAL;
            if self.ports.poll_devices() {
                // A reconnect starts a fresh byte stream.
                self.parser.reset();
                self.refresh_label();
            }
        }
        self.scratch.clear();
        self.ports.drain(&mut self.scratch);
        if self.scratch.is_empty() {
            return;
        }
        self.events.clear();
        for (bytes, at) in &self.scratch {
            self.parser.feed(bytes, *at, &mut self.events);
        }
        let Some(on_event) = &mut self.on_event else {
            return;
        };
        for event in self.events.drain(..) {
            on_event(event);
        }
    }
}

impl InputBackend for MidiBackend {
    fn display_name(&self) -> &str {
        Self::IDLE_LABEL
    }

    fn set_on_event(&mut self, on_event: Option<Box<dyn FnMut(NoteEvent)>>) {
        self.on_event = on_event;
    }

    fn start(&mut self) {
        self.parser.reset();
        self.running = self.ports.start();
        self.rescan_due = 0.0;
        if self.running {
            self.refresh_label();
        } else {
            self.device_label = "No MIDI device".to_string();
        }
    }

    fn stop(&mut self) {
        if self.running {
            self.ports.stop();
            self.running = false;
        }
        self.device_label = Self::IDLE_LABEL.to_string();
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8]) -> Vec<NoteEvent> {
        let mut parser = MidiParser::new();
        let mut out = Vec::new();
        parser.feed(bytes, 1.5, &mut out);
        out
    }

    fn on(midi: u8, velocity: u8) -> NoteEvent {
        NoteEvent {
            kind: NoteEventKind::On,
            midi,
            velocity: Some(velocity),
            timestamp: 1.5,
            confidence: 1.0,
        }
    }

    fn off(midi: u8) -> NoteEvent {
        NoteEvent {
            kind: NoteEventKind::Off,
            midi,
            velocity: None,
            timestamp: 1.5,
            confidence: 1.0,
        }
    }

    #[test]
    fn note_on_with_velocity_is_on_at_full_confidence() {
        assert_eq!(parse(&[0x90, 60, 100]), vec![on(60, 100)]);
    }

    #[test]
    fn note_on_with_zero_velocity_is_off() {
        assert_eq!(parse(&[0x90, 60, 0]), vec![off(60)]);
    }

    #[test]
    fn note_off_drops_release_velocity() {
        assert_eq!(parse(&[0x80, 60, 64]), vec![off(60)]);
    }

    #[test]
    fn running_status_reuses_the_last_status() {
        assert_eq!(
            parse(&[0x90, 60, 100, 64, 90, 60, 0]),
            vec![on(60, 100), on(64, 90), off(60)]
        );
    }

    #[test]
    fn running_status_survives_across_packets() {
        let mut parser = MidiParser::new();
        let mut out = Vec::new();
        parser.feed(&[0x90, 60, 100], 1.0, &mut out);
        parser.feed(&[64, 80], 2.0, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].midi, 64);
        assert_eq!(out[1].timestamp, 2.0);
    }

    #[test]
    fn system_common_clears_running_status() {
        // Song select (0xF3 + 1 data byte), then data bytes with no status
        // to belong to: nothing emitted.
        assert!(parse(&[0x90, 60, 100, 0xF3, 5, 64, 100]).len() == 1);
        // SysEx: the payload is ignored until EOX; EOX leaves no running
        // status either.
        assert_eq!(parse(&[0x90, 60, 100, 0xF0, 0x7E, 0x7F, 0xF7, 64, 100]), vec![on(60, 100)]);
        // Tune request (no data) also breaks running status.
        assert_eq!(parse(&[0x90, 0xF6, 60, 100]), vec![]);
    }

    #[test]
    fn real_time_does_not_disturb_running_status() {
        // Clock and active sensing interleaved mid-message and between
        // running-status messages.
        assert_eq!(
            parse(&[0x90, 60, 0xF8, 100, 0xFE, 0xF8, 64, 0xFE, 100]),
            vec![on(60, 100), on(64, 100)]
        );
        // Start/stop/reset are dropped too (nothing to emit).
        assert_eq!(parse(&[0xFA, 0x90, 60, 100, 0xFC, 0xFF]), vec![on(60, 100)]);
    }

    #[test]
    fn clock_and_active_sensing_alone_emit_nothing() {
        assert_eq!(parse(&[0xF8, 0xFE, 0xF8]), vec![]);
    }

    #[test]
    fn other_channel_voice_messages_consume_their_data_and_emit_nothing() {
        // Control change (2), program change (1), channel pressure (1),
        // pitch bend (2), poly aftertouch (2) — then a note-on that must
        // still parse correctly.
        let bytes = [
            0xB0, 64, 127, // sustain on
            0xC0, 5, // program
            0xD0, 90, // channel pressure
            0xE0, 0x00, 0x40, // pitch bend center
            0xA0, 60, 50, // poly aftertouch
            0x90, 62, 70,
        ];
        assert_eq!(parse(&bytes), vec![on(62, 70)]);
        // Running status applies to them too: two program changes, then a
        // note-on needs its own status.
        assert_eq!(parse(&[0xC0, 1, 2, 0x90, 60, 10]), vec![on(60, 10)]);
    }

    #[test]
    fn channel_is_ignored() {
        assert_eq!(parse(&[0x9F, 60, 100, 0x83, 60, 0]), vec![on(60, 100), off(60)]);
    }

    #[test]
    fn stray_data_bytes_before_any_status_are_ignored() {
        assert_eq!(parse(&[60, 100, 0x90, 61, 100]), vec![on(61, 100)]);
    }

    #[test]
    fn stray_status_mid_message_resyncs() {
        // The note-on lost its velocity byte; the new status restarts.
        assert_eq!(parse(&[0x90, 60, 0x80, 62, 0]), vec![off(62)]);
    }

    #[test]
    fn reset_forgets_running_status_and_partial_message() {
        let mut parser = MidiParser::new();
        let mut out = Vec::new();
        parser.feed(&[0x90, 60], 1.0, &mut out);
        parser.reset();
        parser.feed(&[100, 64, 100], 1.0, &mut out);
        assert!(out.is_empty(), "no status after reset: {out:?}");
        parser.feed(&[0x90, 64, 100], 1.0, &mut out);
        assert_eq!(out.len(), 1);
    }

    /// Scripted ports for the backend tests.
    struct ScriptedPorts {
        packets: std::cell::RefCell<Vec<(Vec<u8>, f64)>>,
        names: std::cell::RefCell<Vec<String>>,
        changed: std::cell::Cell<bool>,
        started: std::cell::Cell<bool>,
        available: bool,
    }
    impl MidiPorts for ScriptedPorts {
        fn start(&self) -> bool {
            self.started.set(self.available);
            self.available
        }
        fn stop(&self) {
            self.started.set(false);
        }
        fn port_names(&self) -> Vec<String> {
            self.names.borrow().clone()
        }
        fn drain(&self, out: &mut Vec<(Vec<u8>, f64)>) {
            out.append(&mut self.packets.borrow_mut());
        }
        fn poll_devices(&self) -> bool {
            self.changed.replace(false)
        }
    }
    fn ports(names: &[&str]) -> Rc<ScriptedPorts> {
        Rc::new(ScriptedPorts {
            packets: std::cell::RefCell::new(Vec::new()),
            names: std::cell::RefCell::new(names.iter().map(|s| s.to_string()).collect()),
            changed: std::cell::Cell::new(false),
            started: std::cell::Cell::new(false),
            available: true,
        })
    }
    fn backend_with_sink(
        ports: &Rc<ScriptedPorts>,
    ) -> (MidiBackend, Rc<std::cell::RefCell<Vec<NoteEvent>>>) {
        let mut backend = MidiBackend::new(Rc::clone(ports) as Rc<dyn MidiPorts>);
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        backend.set_on_event(Some(Box::new(move |e| sink.borrow_mut().push(e))));
        (backend, events)
    }

    #[test]
    fn backend_emits_packet_events_with_packet_timestamps() {
        let ports = ports(&["Piano"]);
        let (mut backend, events) = backend_with_sink(&ports);
        assert_eq!(backend.display_name(), "MIDI keyboard");
        assert_eq!(backend.device_label(), "MIDI keyboard");
        backend.start();
        assert_eq!(backend.device_label(), "Piano");
        ports
            .packets
            .borrow_mut()
            .push((vec![0x90, 60, 100], 10.25));
        ports.packets.borrow_mut().push((vec![60, 0], 10.75));
        backend.process(11.0);
        let events = events.borrow();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].timestamp, 10.25);
        assert_eq!(events[1].kind, NoteEventKind::Off);
        assert_eq!(events[1].timestamp, 10.75, "running status across packets");
    }

    #[test]
    fn device_label_tracks_hot_plug_and_rescans_once_a_second() {
        let ports = ports(&[]);
        let (mut backend, _events) = backend_with_sink(&ports);
        backend.start();
        assert_eq!(backend.device_label(), "No MIDI device");
        backend.process(0.0);
        ports.names.borrow_mut().push("A".into());
        ports.names.borrow_mut().push("B".into());
        ports.changed.set(true);
        backend.process(0.5);
        assert_eq!(backend.device_label(), "No MIDI device", "not due yet");
        backend.process(1.0);
        assert_eq!(backend.device_label(), "2 MIDI devices");
        backend.stop();
        assert!(!ports.started.get());
        assert_eq!(backend.device_label(), "MIDI keyboard");
    }

    #[test]
    fn reconnect_resets_the_parser() {
        let ports = ports(&["Piano"]);
        let (mut backend, events) = backend_with_sink(&ports);
        backend.start();
        // Half a message, then a hot-plug change: the dangling byte must
        // not pair with the next packet's data.
        ports.packets.borrow_mut().push((vec![0x90, 60], 1.0));
        backend.process(0.0);
        ports.changed.set(true);
        ports.packets.borrow_mut().push((vec![100], 2.0));
        backend.process(1.0);
        assert!(events.borrow().is_empty(), "{:?}", events.borrow());
    }

    #[test]
    fn unavailable_ports_leave_the_backend_idle() {
        let ports = Rc::new(ScriptedPorts {
            packets: std::cell::RefCell::new(vec![(vec![0x90, 60, 100], 1.0)]),
            names: std::cell::RefCell::new(Vec::new()),
            changed: std::cell::Cell::new(false),
            started: std::cell::Cell::new(false),
            available: false,
        });
        let (mut backend, events) = backend_with_sink(&ports);
        backend.start();
        assert_eq!(backend.device_label(), "No MIDI device");
        backend.process(1.0);
        assert!(events.borrow().is_empty());
    }
}
