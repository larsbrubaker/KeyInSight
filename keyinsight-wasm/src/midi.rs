//! Browser [`MidiPorts`]: Web MIDI (`navigator.requestMIDIAccess`) →
//! one `onmidimessage` listener per input → packet ring, drained by the
//! core MIDI backend once per engine tick.
//!
//! `start` kicks the (async) permission request and reports optimistic
//! success; packets begin flowing when the user grants access. Hot-plug
//! is real here (`onstatechange`), surfaced through `poll_devices` as a
//! cached change flag. The permission prompt belongs here in the shim,
//! never in visible UI (`docs/architecture.md`).

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use keyinsight_core::input::MidiPorts;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{
    MidiAccess, MidiConnectionEvent, MidiInput, MidiMessageEvent, MidiOptions,
    MidiPortDeviceState,
};

/// Packets older than this are dropped from the ring (a stalled tab can't
/// grow it unbounded).
const RING_SECONDS: f64 = 2.0;
const RING_CAP: usize = 4096;

type PacketRing = Rc<RefCell<VecDeque<(Vec<u8>, f64)>>>;

#[derive(Default)]
struct WebMidiState {
    access: Option<MidiAccess>,
    ring: PacketRing,
    /// Inputs we listen to, by port id, with their listener kept alive.
    inputs: HashMap<String, (MidiInput, Closure<dyn FnMut(MidiMessageEvent)>)>,
    on_state_change: Option<Closure<dyn FnMut(MidiConnectionEvent)>>,
    /// Set by `onstatechange`, cleared by `poll_devices`.
    changed: bool,
    requested: bool,
    listening: bool,
}

pub struct WebMidiPorts {
    state: Rc<RefCell<WebMidiState>>,
}

impl WebMidiPorts {
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(WebMidiState::default())),
        }
    }

    /// Ask for MIDI access and wire the inputs on grant.
    fn request(state: Rc<RefCell<WebMidiState>>) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let options = MidiOptions::new();
        options.set_sysex(false);
        let Ok(promise) = window
            .navigator()
            .request_midi_access_with_options(&options)
        else {
            web_sys::console::warn_1(&"KeyInSight: Web MIDI unavailable".into());
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(access) => {
                    let access: MidiAccess = access.unchecked_into();
                    Self::wire(&state, access);
                }
                Err(err) => {
                    web_sys::console::warn_2(
                        &"KeyInSight: MIDI permission denied".into(),
                        &err,
                    );
                }
            }
        });
    }

    fn wire(state: &Rc<RefCell<WebMidiState>>, access: MidiAccess) {
        // Hot-plug: re-attach to whatever is connected now and flag the
        // change for the backend's next poll.
        let state_for_change = Rc::clone(state);
        let on_state_change = Closure::<dyn FnMut(MidiConnectionEvent)>::new(
            move |_event: MidiConnectionEvent| {
                Self::attach_inputs(&state_for_change);
                state_for_change.borrow_mut().changed = true;
            },
        );
        access.set_onstatechange(Some(on_state_change.as_ref().unchecked_ref()));
        {
            let mut state = state.borrow_mut();
            state.access = Some(access);
            state.on_state_change = Some(on_state_change);
            state.changed = true;
        }
        Self::attach_inputs(state);
    }

    /// Listen on every input we aren't already listening to (while the
    /// backend is running); forget inputs the browser no longer lists.
    fn attach_inputs(state: &Rc<RefCell<WebMidiState>>) {
        let (access, listening) = {
            let state = state.borrow();
            (state.access.clone(), state.listening)
        };
        let Some(access) = access else {
            return;
        };
        let current: Vec<MidiInput> = Self::inputs_of(&access);
        let mut state_mut = state.borrow_mut();
        let live_ids: Vec<String> = current.iter().map(|i| i.id()).collect();
        state_mut.inputs.retain(|id, (input, _)| {
            let keep = live_ids.contains(id);
            if !keep {
                input.set_onmidimessage(None);
            }
            keep
        });
        if !listening {
            return;
        }
        for input in current {
            let id = input.id();
            if state_mut.inputs.contains_key(&id) {
                continue;
            }
            let ring = Rc::clone(&state_mut.ring);
            let on_midi = Closure::<dyn FnMut(MidiMessageEvent)>::new(
                move |event: MidiMessageEvent| {
                    let Ok(bytes) = event.data() else {
                        return;
                    };
                    let now = keyinsight_core::host_now();
                    let at = event_host_time(event.time_stamp(), now);
                    let mut ring = ring.borrow_mut();
                    ring.push_back((bytes, at));
                    while ring
                        .front()
                        .is_some_and(|(_, t)| *t < now - RING_SECONDS)
                        || ring.len() > RING_CAP
                    {
                        ring.pop_front();
                    }
                },
            );
            input.set_onmidimessage(Some(on_midi.as_ref().unchecked_ref()));
            state_mut.inputs.insert(id, (input, on_midi));
        }
    }

    fn inputs_of(access: &MidiAccess) -> Vec<MidiInput> {
        access
            .inputs()
            .values()
            .into_iter()
            .filter_map(|value| value.ok())
            .map(|value| value.unchecked_into::<MidiInput>())
            .collect()
    }

    fn detach_all(state: &mut WebMidiState) {
        for (_, (input, _)) in state.inputs.drain() {
            input.set_onmidimessage(None);
        }
        state.ring.borrow_mut().clear();
    }
}

/// Map a `MIDIMessageEvent.timeStamp` (DOMHighResTimeStamp ms on the
/// `performance.now()` timeline) onto the host clock. `host_now` runs on
/// the same monotonic source with its own zero, so the offset between the
/// two is a constant measured right now; a zero stamp means "now".
fn event_host_time(stamp_ms: f64, host_now: f64) -> f64 {
    if stamp_ms <= 0.0 {
        return host_now;
    }
    let Some(perf_now_ms) = web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
    else {
        return host_now;
    };
    host_now - (perf_now_ms - stamp_ms) / 1000.0
}

impl MidiPorts for WebMidiPorts {
    fn start(&self) -> bool {
        {
            let mut state = self.state.borrow_mut();
            state.listening = true;
            if state.access.is_some() {
                drop(state);
                Self::attach_inputs(&self.state);
                self.state.borrow_mut().changed = true;
                return true;
            }
            if state.requested {
                return true;
            }
            state.requested = true;
        }
        Self::request(Rc::clone(&self.state));
        // Optimistic: packets flow once the user grants the prompt.
        true
    }

    fn stop(&self) {
        let mut state = self.state.borrow_mut();
        state.listening = false;
        Self::detach_all(&mut state);
    }

    fn port_names(&self) -> Vec<String> {
        let state = self.state.borrow();
        let Some(access) = &state.access else {
            return Vec::new();
        };
        Self::inputs_of(access)
            .iter()
            .filter(|input| input.state() == MidiPortDeviceState::Connected)
            .map(|input| input.name().unwrap_or_else(|| "MIDI input".to_string()))
            .collect()
    }

    fn drain(&self, out: &mut Vec<(Vec<u8>, f64)>) {
        let state = self.state.borrow();
        let mut ring = state.ring.borrow_mut();
        out.extend(ring.drain(..));
    }

    fn poll_devices(&self) -> bool {
        let mut state = self.state.borrow_mut();
        std::mem::replace(&mut state.changed, false)
    }
}
