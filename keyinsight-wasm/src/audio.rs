//! Browser [`AudioOut`] over WebAudio: the shared synth buffers
//! (metronome clicks + rendered SMF piano clips) scheduled on an
//! `AudioContext`, with host-clock seconds mapped onto the context
//! timeline at each call.
//!
//! The context is created lazily on the first playback request — that
//! request always originates from a user gesture (a Hear It click, a
//! Tempo-mode start), which is exactly when browsers allow audio to
//! start. If construction or resume fails, the app keeps training
//! silently, like the Swift engine-start failure path.

use std::cell::RefCell;

use keyinsight_core::audio::{self, AudioOut};
use keyinsight_core::host_now;
use web_sys::{AudioBuffer, AudioBufferSourceNode, AudioContext};

struct WebAudioState {
    ctx: AudioContext,
    click: AudioBuffer,
    accent: AudioBuffer,
    /// The in-flight Hear It source, for `stop_smf`.
    clip_source: Option<AudioBufferSourceNode>,
}

pub struct WebAudioOut {
    state: RefCell<Option<WebAudioState>>,
}

impl WebAudioOut {
    pub fn new() -> Self {
        Self {
            state: RefCell::new(None),
        }
    }

    /// Create the context + click buffers on first use; resume a
    /// suspended context (autoplay policy) on every use.
    fn with_state<R>(&self, f: impl FnOnce(&mut WebAudioState) -> Option<R>) -> Option<R> {
        let mut slot = self.state.borrow_mut();
        if slot.is_none() {
            *slot = Self::build_state();
        }
        let state = slot.as_mut()?;
        if state.ctx.state() == web_sys::AudioContextState::Suspended {
            let _ = state.ctx.resume();
        }
        f(state)
    }

    fn build_state() -> Option<WebAudioState> {
        let ctx = AudioContext::new()
            .map_err(|err| {
                web_sys::console::warn_2(
                    &"KeyInSight: WebAudio unavailable — continuing silently".into(),
                    &err,
                );
            })
            .ok()?;
        let rate = ctx.sample_rate() as f64;
        let click = to_buffer(&ctx, &audio::click_samples(rate, false))?;
        let accent = to_buffer(&ctx, &audio::click_samples(rate, true))?;
        Some(WebAudioState {
            ctx,
            click,
            accent,
            clip_source: None,
        })
    }
}

impl AudioOut for WebAudioOut {
    fn play_click(&self, at_host_seconds: f64, accent: bool) {
        self.with_state(|state| {
            // Host seconds → context seconds: both clocks tick in real
            // time, so the offset measured now holds for the schedule
            // horizon (1.5 s).
            let when = state.ctx.current_time() + (at_host_seconds - host_now()).max(0.0);
            let source = state.ctx.create_buffer_source().ok()?;
            source.set_buffer(Some(if accent { &state.accent } else { &state.click }));
            source
                .connect_with_audio_node(&state.ctx.destination())
                .ok()?;
            source.start_with_when(when).ok()
        });
    }

    fn play_smf(&self, smf: &[u8]) -> bool {
        self.with_state(|state| {
            let rate = state.ctx.sample_rate() as f64;
            let clip = audio::render_smf(smf, rate)?;
            let buffer = to_buffer(&state.ctx, &clip.samples)?;
            let source = state.ctx.create_buffer_source().ok()?;
            source.set_buffer(Some(&buffer));
            source
                .connect_with_audio_node(&state.ctx.destination())
                .ok()?;
            source.start().ok()?;
            if let Some(previous) = state.clip_source.replace(source) {
                let _ = previous.stop();
            }
            Some(())
        })
        .is_some()
    }

    fn stop_smf(&self) {
        self.with_state(|state| {
            if let Some(source) = state.clip_source.take() {
                let _ = source.stop();
            }
            Some(())
        });
    }

    fn play_note(&self, midi: u8, duration_seconds: f64) {
        self.with_state(|state| {
            let rate = state.ctx.sample_rate() as f64;
            let clip = audio::render_note(midi, duration_seconds, rate)?;
            let buffer = to_buffer(&state.ctx, &clip.samples)?;
            let source = state.ctx.create_buffer_source().ok()?;
            source.set_buffer(Some(&buffer));
            source
                .connect_with_audio_node(&state.ctx.destination())
                .ok()?;
            // Its own source (like the Swift sampler note beside the
            // sequencer): not tracked as the clip, so it neither cancels
            // nor is cancelled by SMF playback.
            source.start().ok()
        });
    }
}

/// Copy a mono sample buffer into a WebAudio `AudioBuffer`.
fn to_buffer(ctx: &AudioContext, samples: &[f32]) -> Option<AudioBuffer> {
    let buffer = ctx
        .create_buffer(1, samples.len().max(1) as u32, ctx.sample_rate())
        .ok()?;
    // web-sys takes &mut [f32] here despite only reading it.
    let mut copy: Vec<f32> = samples.to_vec();
    buffer.copy_to_channel(&mut copy, 0).ok()?;
    Some(buffer)
}
