//! Desktop [`MidiPorts`] over midir (CoreMIDI / WinMM / ALSA): one input
//! connection per port, all feeding a shared packet ring the core MIDI
//! backend drains once per engine tick. Hot-plug is a once-a-second
//! rescan (the backend paces it): when the enumerated port set changes,
//! every connection is rebuilt. The comparison is against the last
//! ENUMERATION, not the open connections — a port that refuses to open
//! must not make every rescan tear down and rebuild the working ones
//! (parser reset, notes lost, log spam once a second).
//!
//! The Swift app subscribed SwiftMIDI to `.allOutputs`; the behaviour
//! here is the same — every source, no picker (`MIDIBackend.swift`).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use keyinsight_core::input::MidiPorts;
use midir::{Ignore, MidiInput, MidiInputConnection};

/// Packets older than this are dropped from the ring (a stalled frame
/// loop can't grow it unbounded).
const RING_SECONDS: f64 = 2.0;
const RING_CAP: usize = 4096;
/// A midir timestamp whose mapped host time drifts this far from "now"
/// is re-anchored (driver clock hiccup, suspend/resume).
const MAX_DRIFT_SECONDS: f64 = 0.25;

type PacketRing = Arc<Mutex<VecDeque<(Vec<u8>, f64)>>>;

struct Connection {
    name: String,
    _conn: MidiInputConnection<()>,
}

pub struct MidirPorts {
    connections: RefCell<Vec<Connection>>,
    /// Port names as last enumerated (sorted) — the hot-plug baseline.
    enumerated: RefCell<Vec<String>>,
    ring: PacketRing,
    running: std::cell::Cell<bool>,
}

impl Default for MidirPorts {
    fn default() -> Self {
        Self::new()
    }
}

impl MidirPorts {
    pub fn new() -> Self {
        Self {
            connections: RefCell::new(Vec::new()),
            enumerated: RefCell::new(Vec::new()),
            ring: Arc::new(Mutex::new(VecDeque::new())),
            running: std::cell::Cell::new(false),
        }
    }

    fn client() -> Option<MidiInput> {
        match MidiInput::new("KeyInSight") {
            Ok(mut input) => {
                input.ignore(Ignore::SysexAndTime);
                Some(input)
            }
            Err(err) => {
                eprintln!("KeyInSight: MIDI unavailable ({err})");
                None
            }
        }
    }

    /// A port whose name lookup fails still gets subscribed, under this
    /// substitute (the same in enumeration and connection so the hot-plug
    /// comparison stays stable).
    fn port_label(input: &MidiInput, port: &midir::MidiInputPort) -> String {
        input
            .port_name(port)
            .unwrap_or_else(|_| "MIDI input".to_string())
    }

    /// Names of every input port the system offers right now (sorted, so
    /// hot-plug comparisons are order-independent).
    fn available_names() -> Vec<String> {
        let Some(input) = Self::client() else {
            return Vec::new();
        };
        let mut names: Vec<String> = input
            .ports()
            .iter()
            .map(|port| Self::port_label(&input, port))
            .collect();
        names.sort();
        names
    }

    /// Subscribe to every input port (dropping any existing connections)
    /// and remember the enumeration this subscription was built from.
    fn connect_all(&self) {
        let mut connections = self.connections.borrow_mut();
        connections.clear();
        self.enumerated.borrow_mut().clear();
        let Some(enumerator) = Self::client() else {
            return;
        };
        let mut enumerated: Vec<String> = Vec::new();
        for port in enumerator.ports() {
            let name = Self::port_label(&enumerator, &port);
            enumerated.push(name.clone());
            // `connect` consumes the client, so each port gets its own.
            let Some(input) = Self::client() else {
                break;
            };
            let Some(port) = input.find_port_by_id(port.id()) else {
                continue;
            };
            let ring = Arc::clone(&self.ring);
            // Per-connection mapping from midir's microsecond stamps
            // (unspecified epoch) onto the host clock: anchored on the
            // first packet, re-anchored on drift.
            let mut epoch_offset: Option<f64> = None;
            let callback = move |stamp_us: u64, bytes: &[u8], _: &mut ()| {
                let now = keyinsight_core::host_now();
                let at = if stamp_us == 0 {
                    now
                } else {
                    let stamp = stamp_us as f64 / 1e6;
                    let offset = *epoch_offset.get_or_insert(now - stamp);
                    let mapped = stamp + offset;
                    if (mapped - now).abs() > MAX_DRIFT_SECONDS {
                        epoch_offset = Some(now - stamp);
                        now
                    } else {
                        mapped
                    }
                };
                push_packet(&ring, bytes, at, now);
            };
            match input.connect(&port, "keyinsight-in", callback, ()) {
                Ok(conn) => connections.push(Connection { name, _conn: conn }),
                Err(err) => eprintln!("KeyInSight: couldn't open MIDI port {name} ({err})"),
            }
        }
        enumerated.sort();
        *self.enumerated.borrow_mut() = enumerated;
    }
}

fn push_packet(ring: &PacketRing, bytes: &[u8], at: f64, now: f64) {
    let mut ring = ring.lock().unwrap_or_else(PoisonError::into_inner);
    ring.push_back((bytes.to_vec(), at));
    while ring
        .front()
        .is_some_and(|(_, t)| *t < now - RING_SECONDS)
        || ring.len() > RING_CAP
    {
        ring.pop_front();
    }
}

impl MidiPorts for MidirPorts {
    fn start(&self) -> bool {
        if self.running.get() {
            return true;
        }
        // No client at all (no MIDI subsystem) means no MIDI; zero ports
        // is fine — hot-plug may bring one.
        if Self::client().is_none() {
            return false;
        }
        self.ring
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.connect_all();
        self.running.set(true);
        true
    }

    fn stop(&self) {
        self.connections.borrow_mut().clear();
        self.enumerated.borrow_mut().clear();
        self.ring
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.running.set(false);
    }

    fn port_names(&self) -> Vec<String> {
        self.connections
            .borrow()
            .iter()
            .map(|c| c.name.clone())
            .collect()
    }

    fn drain(&self, out: &mut Vec<(Vec<u8>, f64)>) {
        let mut ring = self.ring.lock().unwrap_or_else(PoisonError::into_inner);
        out.extend(ring.drain(..));
    }

    fn poll_devices(&self) -> bool {
        if !self.running.get() {
            return false;
        }
        // Only a change in what the system ENUMERATES rebuilds the
        // subscriptions; a port that failed to open stays failed until
        // the enumeration itself changes.
        if Self::available_names() == *self.enumerated.borrow() {
            return false;
        }
        self.connect_all();
        true
    }
}

/// Headless MIDI diagnostic (`keyinsight-native --midi-smoke`): list the
/// input ports, then print parsed note events for ~10 s.
pub fn midi_smoke() {
    use keyinsight_core::core::InputBackend;
    use keyinsight_core::input::MidiBackend;
    use std::rc::Rc;

    let ports = Rc::new(MidirPorts::new());
    let mut backend = MidiBackend::new(Rc::clone(&ports) as Rc<dyn MidiPorts>);
    backend.set_on_event(Some(Box::new(|event| {
        println!(
            "midi-smoke: {:?} midi={} velocity={:?} at={:.3}s",
            event.kind, event.midi, event.velocity, event.timestamp
        );
    })));
    backend.start();
    let names = ports.port_names();
    if names.is_empty() {
        println!("midi-smoke: no MIDI ports — connect a keyboard and retry");
        backend.stop();
        return;
    }
    println!("midi-smoke: listening on {names:?} for 10 s — play something");
    let start = std::time::Instant::now();
    while start.elapsed().as_secs_f64() < 10.0 {
        backend.process(keyinsight_core::host_now());
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    backend.stop();
    println!("midi-smoke: done");
}
