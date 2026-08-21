# Platform substitutions (Swift/Apple → cross-platform Rust)

The Swift app leans on Apple frameworks. Each has a designated replacement;
the substitution is always made **behind a trait in `keyinsight-core`**,
never inline. These are the only sanctioned divergences from the Swift
source (see `docs/porting.md`).

| Swift / Apple | Rust port |
|---|---|
| SwiftUI views | agg-gui widgets (see `docs/architecture.md`) |
| `AttributedString(markdown:)` inline emphasis in `AboutSheet.swift` (`*italics*` in the `term` bodies) | agg-gui labels are plain text: the About sheet (`ui/sheets/about.rs`) ships the same copy with the emphasis asterisks stripped — the words stand unmarked |
| Verovio SVG notation via WKWebView | [verovio-rust](https://github.com/larsbrubaker/verovio-rust) — our Rust port of Verovio engraving, rendering through `DrawCtx` with the Leipzig SMuFL font. It lives in its own repository because Verovio is LGPL-3.0 and this app is MIT; consume it only as the `verovio-rust` library dependency (pinned git submodule at `verovio-rust/`). Per-note ids, bounds lookup, color overrides, and the timemap replace Verovio's SVG-id APIs. The score always renders on a light page — music is always light. |
| SwiftMIDI / CoreMIDI (`Input/MIDIBackend.swift`) | `midir` on native (`keyinsight-native/src/midi.rs`, one connection per input port, every port — the Swift `.allOutputs` subscription — hot-plug by a once-a-second rescan); Web MIDI via `web-sys` on WASM (`keyinsight-wasm/src/midi.rs`, `requestMIDIAccess` without sysex, `onstatechange` hot-plug). Both sit behind the core's `input::MidiPorts` trait (pull model: raw packets + capture timestamps in host seconds); the core's `input::MidiBackend` + `MidiParser` do what SwiftMIDI's receiver options did — running status, note-on velocity 0 → note-off, clock/active-sensing filtered, confidence 1.0, packet timestamps with a host-clock fallback when the stamp is 0. Display name stays the Swift literal "MIDI keyboard". Timestamps: midir's microsecond stamps are anchored to `host_now()` per connection on the first packet (re-anchored on >250 ms drift); `MIDIMessageEvent.timeStamp` is mapped through `performance.now()`. |
| AVAudioEngine sampler + metronome | `cpal` output on native; WebAudio on WASM — behind the core's `AudioOut` trait. SMF playback renders through OxiSynth + the bundled CC0 Upright Piano KW SF2 (`audio::synth`); a synthesized piano voice is the no-soundfont fallback. |
| SoundpipeAudioKit PitchTap | Goertzel bank over the exercise's candidate notes (`audio::goertzel`, chord-capable, noise-robust); mic capture behind `KeyInSightPlatform::mic` (cpal / getUserMedia). The ported `YinPitchDetector` remains for monophonic pitch tracking. Display name divergence: Swift's `MicBackend.displayName` is "Microphone (single notes)"; the Rust backend reports "Microphone" because the Goertzel bank hears chords — the "(single notes)" caveat no longer applies. The name is persisted in the session row (`sessions.input_backend`), so rows written by the two apps differ there. |
| GRDB / SQLite | Storage trait in core (load/save serialized state); native = file-backed, WASM = localStorage/IndexedDB. Port the `AppDatabase` schema semantics (skill stats, session history, settings, library) even though the storage engine differs. |
| MusicXML via Verovio | Port `MusicXMLImporter`/`MusicXMLEncoder` directly (plain XML processing); use `quick-xml` |
| IOKit power assertions (`IOPMAssertionCreateWithName` / `IOPMAssertionDeclareUserActivity`, `DisplaySleepGuard.swift`) | `KeyInSightPlatform::set_display_awake` — `keepawake` on native (Windows/macOS/Linux), Screen Wake Lock on WASM (`keyinsight-wasm/src/wake_lock.rs`: `navigator.wakeLock.request("screen")` on true, `release()` on false, re-acquired on `visibilitychange` because the browser drops the lock whenever the tab hides; called through `js_sys::Reflect` since web-sys gates the API behind `web_sys_unstable_apis`; browsers without it warn once), no-op headless. Known gap: the Swift guard also calls `IOPMAssertionDeclareUserActivity` on release so the idle clock restarts when an exercise ends; the native shim only drops the `keepawake` handle, so the display may dim sooner after an exercise on macOS than it did in the Swift app. |

Notes:

- Music glyphs (noteheads, clefs, accidentals) come from a bundled SMuFL
  font (Bravura, OFL-licensed) rendered through agg-gui's text stack.
- The shims (`keyinsight-native`, `keyinsight-wasm`) implement these traits;
  the core never `cfg`-gates on platform.
