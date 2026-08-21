//! The MIDI input source end to end: scripted ports feed raw packets with
//! capture timestamps; the backend parses them per tick; the engine
//! scores them like any other note events (self-paced completion, tempo
//! timing classification off the packet timestamps).

use std::cell::RefCell;
use std::rc::Rc;

use super::{test_clock, NOW};
use crate::audio::NullAudioOut;
use crate::engine::session::{InputSource, PacingMode, Phase, SessionEngine};
use crate::engine::{default_backend_factory, BackendFactory};
use crate::input::{MidiBackend, MidiPorts};
use crate::persistence::AppDatabase;

/// Fake platform MIDI ports: hands out scripted packets.
struct FakeMidiPorts {
    packets: RefCell<Vec<(Vec<u8>, f64)>>,
    started: std::cell::Cell<bool>,
}

impl FakeMidiPorts {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            packets: RefCell::new(Vec::new()),
            started: std::cell::Cell::new(false),
        })
    }

    fn push(&self, bytes: &[u8], at: f64) {
        self.packets.borrow_mut().push((bytes.to_vec(), at));
    }
}

impl MidiPorts for FakeMidiPorts {
    fn start(&self) -> bool {
        self.started.set(true);
        true
    }
    fn stop(&self) {
        self.started.set(false);
    }
    fn port_names(&self) -> Vec<String> {
        vec!["Fake Piano".to_string()]
    }
    fn drain(&self, out: &mut Vec<(Vec<u8>, f64)>) {
        out.append(&mut self.packets.borrow_mut());
    }
    fn poll_devices(&self) -> bool {
        false
    }
}

/// An engine whose MIDI source is backed by the fake ports (the real
/// factory's shape: MIDI → MidiBackend, everything else the default).
fn midi_engine() -> (SessionEngine, Rc<RefCell<f64>>, Rc<FakeMidiPorts>) {
    let (time, clock) = test_clock();
    let ports = FakeMidiPorts::new();
    let ports_for_factory = Rc::clone(&ports);
    let fallback = default_backend_factory();
    let factory: BackendFactory = Box::new(move |source| match source {
        InputSource::Midi => Box::new(MidiBackend::new(
            Rc::clone(&ports_for_factory) as Rc<dyn MidiPorts>
        )),
        other => fallback(other),
    });
    let mut engine = SessionEngine::new(
        Some(AppDatabase::in_memory(NOW)),
        Rc::new(NullAudioOut),
        clock,
        factory,
        42,
    );
    engine.start();
    engine.set_input_source(InputSource::Midi);
    (engine, time, ports)
}

#[test]
fn midi_packets_complete_a_self_paced_exercise() {
    let (mut engine, time, ports) = midi_engine();
    assert!(ports.started.get(), "switching to MIDI opened the ports");
    assert_eq!(engine.input_source(), InputSource::Midi);
    assert_eq!(*engine.phase(), Phase::Playing);

    let mut guard = 0;
    while *engine.phase() == Phase::Playing {
        let mut midis: Vec<u8> = engine.current_expected_midis().iter().copied().collect();
        midis.sort_unstable();
        assert!(!midis.is_empty());
        *time.borrow_mut() += 0.4;
        let at = *time.borrow();
        // One packet per chord: note-ons under running status, a clock
        // byte interleaved, then the releases as velocity-0 note-ons.
        let mut bytes = vec![0x90];
        for (i, midi) in midis.iter().enumerate() {
            if i == 1 {
                bytes.push(0xF8);
            }
            bytes.extend_from_slice(&[*midi, 80]);
        }
        ports.push(&bytes, at - 0.01);
        let mut release = vec![0x90];
        for midi in &midis {
            release.extend_from_slice(&[*midi, 0]);
        }
        ports.push(&release, at);
        engine.tick();
        guard += 1;
        assert!(guard < 400, "exercise should complete");
    }
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected a summary, got {:?}", engine.phase());
    };
    assert_eq!(summary.error_count, 0);
    assert_eq!(summary.first_try_correct, summary.note_count);
    assert!(summary.mean_latency_ms.is_some());
    assert_eq!(engine.exercises_completed(), 1);
}

#[test]
fn midi_packet_timestamps_drive_tempo_classification() {
    let (mut engine, time, ports) = midi_engine();
    assert!(engine.content_supports_tempo());
    engine.set_mode(PacingMode::Tempo);
    assert_eq!(engine.active_pacing(), PacingMode::Tempo);

    // Every target in host seconds, from the metronome's grid.
    let now = *time.borrow();
    let start_host = now - engine.metronome.milliseconds_since_start(now) / 1000.0;
    let targets: Vec<(u8, f64)> = engine
        .tempo_matcher
        .as_ref()
        .expect("tempo run")
        .expected
        .iter()
        .map(|e| (e.midi, start_host + e.target_ms / 1000.0))
        .collect();
    let expected_count = targets.len();
    assert!(expected_count > 0);

    // Play every note 80 ms late (inside the ±120 ms window, outside the
    // ±45 ms on-time band): the packet timestamps, not the tick time,
    // decide the classification — the tick lands well after.
    for (midi, target) in &targets {
        let played_at = target + 0.080;
        ports.push(&[0x90, *midi, 90], played_at);
        ports.push(&[0x80, *midi, 0], played_at + 0.05);
        *time.borrow_mut() = played_at + 0.03;
        engine.tick();
    }
    // The deferred TempoFinish needs a tick past its deadline.
    *time.borrow_mut() += 1.0;
    engine.tick();
    let Phase::Summary(summary) = engine.phase() else {
        panic!("expected tempo summary, got {:?}", engine.phase());
    };
    let timing = summary.timing.as_ref().expect("tempo summary carries timing");
    assert_eq!(timing.expected_count, expected_count);
    assert_eq!(timing.late, expected_count, "{timing:?}");
    assert_eq!(timing.missed, 0);
    assert_eq!(timing.early, 0);
    assert_eq!(timing.on_time, 0);
    assert_eq!(summary.error_count, 0);
}

#[test]
fn midi_backend_reports_its_display_name_and_device() {
    let (mut engine, _time, _ports) = midi_engine();
    let backend = engine
        .backend
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<MidiBackend>())
        .expect("MIDI source runs the MIDI backend");
    assert_eq!(crate::core::InputBackend::display_name(backend), "MIDI keyboard");
    assert_eq!(backend.device_label(), "Fake Piano");
}
