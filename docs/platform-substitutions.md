# Platform substitutions (Swift/Apple → cross-platform Rust)

The Swift app leans on Apple frameworks. Each has a designated replacement;
the substitution is always made **behind a trait in `keyinsight-core`**,
never inline. These are the only sanctioned divergences from the Swift
source (see `docs/porting.md`).

| Swift / Apple | Rust port |
|---|---|
| SwiftUI views | agg-gui widgets (see `docs/architecture.md`) |
| Verovio SVG notation via WKWebView | [verovio-rust](https://github.com/larsbrubaker/verovio-rust) — our Rust port of Verovio engraving, rendering through `DrawCtx` with the Leipzig SMuFL font. It lives in its own repository because Verovio is LGPL-3.0 and this app is MIT; consume it only as the `verovio-rust` library dependency (sibling checkout, like agg-gui). Per-note ids, bounds lookup, color overrides, and the timemap replace Verovio's SVG-id APIs. The score always renders on a light page — music is always light. |
| SwiftMIDI / CoreMIDI | `midir` on native; Web MIDI API via `wasm-bindgen` on WASM — both behind the core's `MidiPort` trait |
| AVAudioEngine sampler + metronome | `cpal` output on native; WebAudio on WASM — behind the core's `AudioOut` trait. SMF playback renders through OxiSynth + the bundled CC0 Upright Piano KW SF2 (`audio::synth`); a synthesized piano voice is the no-soundfont fallback. |
| SoundpipeAudioKit PitchTap | Goertzel bank over the exercise's candidate notes (`audio::goertzel`, chord-capable, noise-robust); mic capture behind `KeyInSightPlatform::mic` (cpal / getUserMedia). The ported `YinPitchDetector` remains for monophonic pitch tracking. |
| GRDB / SQLite | Storage trait in core (load/save serialized state); native = file-backed, WASM = localStorage/IndexedDB. Port the `AppDatabase` schema semantics (skill stats, session history, settings, library) even though the storage engine differs. |
| MusicXML via Verovio | Port `MusicXMLImporter`/`MusicXMLEncoder` directly (plain XML processing); use `quick-xml` |

Notes:

- Music glyphs (noteheads, clefs, accidentals) come from a bundled SMuFL
  font (Bravura, OFL-licensed) rendered through agg-gui's text stack.
- The shims (`keyinsight-native`, `keyinsight-wasm`) implement these traits;
  the core never `cfg`-gates on platform.
