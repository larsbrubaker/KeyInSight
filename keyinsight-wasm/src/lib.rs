//! # WebAssembly Shell for KeyInSight
//!
//! Thinnest possible browser shim: everything platform-generic (canvas
//! sizing, wgpu/WebGL2 surface, the rAF loop, DOM pointer / wheel /
//! keyboard / clipboard listeners) lives in `demo_wgpu::web_shell`. This
//! crate contributes only what is genuinely specific to KeyInSight in a
//! browser: the [`KeyInSightPlatform`] implementation (localStorage-backed
//! persistence; Web MIDI, WebAudio, and getUserMedia land here next — see
//! `docs/platform-substitutions.md`) and the per-frame engine tick.

#![cfg(target_arch = "wasm32")]

mod audio;
mod mic;

use std::rc::Rc;

use demo_wgpu::web_shell;
use keyinsight_core::audio::AudioOut;
use keyinsight_core::persistence::Storage;
use keyinsight_core::{build_keyinsight_app, KeyInSightPlatform, UiFonts};
use wasm_bindgen::prelude::*;

const STORAGE_KEY: &str = "keyinsight-db";

/// localStorage-backed persistence (the browser's Application Support).
struct LocalStorage;

impl Storage for LocalStorage {
    fn load(&self) -> Option<String> {
        web_sys::window()?
            .local_storage()
            .ok()??
            .get_item(STORAGE_KEY)
            .ok()?
    }

    fn save(&self, contents: &str) {
        let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
        if let Some(storage) = storage {
            // Quota errors never take down the training loop.
            let _ = storage.set_item(STORAGE_KEY, contents);
        }
    }
}

/// Browser implementation of the platform capability surface.
struct WasmPlatform;

impl KeyInSightPlatform for WasmPlatform {
    fn storage(&self) -> Option<Box<dyn Storage>> {
        Some(Box::new(LocalStorage))
    }

    /// Metronome clicks + Hear It playback through WebAudio (silent
    /// fallback when the browser refuses a context).
    fn audio(&self) -> Rc<dyn AudioOut> {
        Rc::new(audio::WebAudioOut::new())
    }

    /// getUserMedia microphone capture for the mic input source.
    fn mic(&self) -> Option<Rc<dyn keyinsight_core::input::MicSource>> {
        Some(Rc::new(mic::WebMicSource::new()))
    }
}

thread_local! {
    static HANDLES: std::cell::RefCell<Option<keyinsight_core::KeyInSightHandles>> =
        const { std::cell::RefCell::new(None) };
}

#[wasm_bindgen(start)]
pub fn start() {
    web_shell::start(
        "keyinsight-canvas",
        || {
            let (app, handles) = build_keyinsight_app(UiFonts::bundled(), WasmPlatform);
            HANDLES.with(|h| *h.borrow_mut() = Some(handles));
            app
        },
        // Advance the engine every painted frame (input queue, deferred
        // actions, metronome sweep).
        || {
            HANDLES.with(|h| {
                if let Some(handles) = h.borrow().as_ref() {
                    handles.tick();
                }
            });
        },
    );
}

/// Report the port version to the hosting page (used by the status site).
#[wasm_bindgen]
pub fn port_version() -> String {
    keyinsight_core::PORT_VERSION.to_string()
}
