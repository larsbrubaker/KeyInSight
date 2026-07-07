//! Browser [`MicSource`]: getUserMedia → ScriptProcessorNode → sample
//! ring, drained by the core mic backend once per engine tick.
//!
//! `start` kicks the (async) permission request and reports optimistic
//! success; samples begin flowing when the user grants access. The
//! permission prompt itself belongs here in the shim, never in visible
//! UI (`docs/architecture.md`).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use keyinsight_core::input::MicSource;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    AudioContext, AudioProcessingEvent, MediaStream, MediaStreamAudioSourceNode,
    MediaStreamConstraints, ScriptProcessorNode,
};

/// Cap the buffer at ~2 s so a stalled tab can't grow it unbounded.
const MIC_RING_CAP: usize = 96_000;
/// ScriptProcessor chunk size (samples per callback).
const CAPTURE_CHUNK: u32 = 2048;

#[derive(Default)]
struct WebMicState {
    ring: Option<Rc<RefCell<VecDeque<f32>>>>,
    ctx: Option<AudioContext>,
    processor: Option<ScriptProcessorNode>,
    source: Option<MediaStreamAudioSourceNode>,
    stream: Option<MediaStream>,
    // Kept alive for the processor's lifetime.
    on_audio: Option<Closure<dyn FnMut(AudioProcessingEvent)>>,
    requested: bool,
}

pub struct WebMicSource {
    state: Rc<RefCell<WebMicState>>,
}

impl WebMicSource {
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(WebMicState::default())),
        }
    }

    /// Ask for the microphone and wire the capture graph on grant.
    fn request(state: Rc<RefCell<WebMicState>>) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(devices) = window.navigator().media_devices() else {
            web_sys::console::warn_1(&"KeyInSight: mediaDevices unavailable".into());
            return;
        };
        let constraints = MediaStreamConstraints::new();
        constraints.set_audio(&JsValue::TRUE);
        let Ok(promise) = devices.get_user_media_with_constraints(&constraints) else {
            web_sys::console::warn_1(&"KeyInSight: getUserMedia rejected".into());
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(stream) => {
                    let stream: MediaStream = stream.unchecked_into();
                    if let Err(err) = Self::wire(&state, stream) {
                        web_sys::console::warn_2(
                            &"KeyInSight: mic capture failed".into(),
                            &err,
                        );
                    }
                }
                Err(err) => {
                    web_sys::console::warn_2(
                        &"KeyInSight: microphone permission denied".into(),
                        &err,
                    );
                }
            }
        });
    }

    fn wire(state: &Rc<RefCell<WebMicState>>, stream: MediaStream) -> Result<(), JsValue> {
        let ctx = AudioContext::new()?;
        let source = ctx.create_media_stream_source(&stream)?;
        let processor =
            ctx.create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
                CAPTURE_CHUNK, 1, 1,
            )?;

        let ring: Rc<RefCell<VecDeque<f32>>> = Rc::new(RefCell::new(VecDeque::new()));
        let sink = Rc::clone(&ring);
        let on_audio = Closure::<dyn FnMut(AudioProcessingEvent)>::new(
            move |event: AudioProcessingEvent| {
                let Ok(buffer) = event.input_buffer() else {
                    return;
                };
                let Ok(samples) = buffer.get_channel_data(0) else {
                    return;
                };
                let mut ring = sink.borrow_mut();
                ring.extend(samples.iter().copied());
                let excess = ring.len().saturating_sub(MIC_RING_CAP);
                ring.drain(..excess);
            },
        );
        processor.set_onaudioprocess(Some(on_audio.as_ref().unchecked_ref()));

        // The processor only runs while connected downstream; its output
        // buffer stays silent (never written), so nothing is audible.
        source.connect_with_audio_node(&processor)?;
        processor.connect_with_audio_node(&ctx.destination())?;

        let mut state = state.borrow_mut();
        state.ring = Some(ring);
        state.ctx = Some(ctx);
        state.processor = Some(processor);
        state.source = Some(source);
        state.stream = Some(stream);
        state.on_audio = Some(on_audio);
        Ok(())
    }
}

impl MicSource for WebMicSource {
    fn start(&self) -> bool {
        let mut state = self.state.borrow_mut();
        if state.processor.is_some() {
            return true;
        }
        if !state.requested {
            state.requested = true;
            drop(state);
            Self::request(Rc::clone(&self.state));
        }
        // Optimistic: samples flow once the user grants the prompt.
        true
    }

    fn stop(&self) {
        let mut state = self.state.borrow_mut();
        if let Some(processor) = state.processor.take() {
            processor.set_onaudioprocess(None);
            let _ = processor.disconnect();
        }
        if let Some(source) = state.source.take() {
            let _ = source.disconnect();
        }
        if let Some(stream) = state.stream.take() {
            for track in stream.get_tracks().iter() {
                track.unchecked_into::<web_sys::MediaStreamTrack>().stop();
            }
        }
        if let Some(ctx) = state.ctx.take() {
            let _ = ctx.close();
        }
        state.ring = None;
        state.on_audio = None;
        state.requested = false;
    }

    fn sample_rate(&self) -> f64 {
        self.state
            .borrow()
            .ctx
            .as_ref()
            .map(|ctx| ctx.sample_rate() as f64)
            .unwrap_or(48_000.0)
    }

    fn drain(&self, out: &mut Vec<f32>) {
        if let Some(ring) = self.state.borrow().ring.as_ref() {
            let mut ring = ring.borrow_mut();
            out.extend(ring.drain(..));
        }
    }
}
