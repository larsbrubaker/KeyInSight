# KeyInSight

[![KeyInSight — try it in the browser](hero_readme.jpg)](https://larsbrubaker.github.io/KeyInSight/)

**[▶ Try it live in your browser](https://larsbrubaker.github.io/KeyInSight/)** — the full trainer as WASM on GitHub Pages, with piano audio.

A cross-platform port of [KeyInSight](https://github.com/kevinepope/KeyInSight)
— a piano sight-reading trainer that builds the *see-note → press-key* reflex
the way touch-typing trainers work: adaptive generated exercises, tight
feedback loops, no music-reading prerequisite.

The original is a macOS Swift/SwiftUI app. This port is Rust +
[agg-gui](https://github.com/larsbrubaker/agg-gui), producing:

- a **native desktop app** (Windows / macOS / Linux via winit + wgpu), and
- a **browser app** (WASM, deployed to GitHub Pages).

## Status

**Phase 1 port complete and synced with upstream Swift `9fc4f78`.** All
engine modules are ported from Swift with their test suites (Core, Score,
Engine + SessionEngine, Skill, Persistence, Audio DSP, Input, Notation,
UI): the adaptive trainer runs end-to-end — generated exercises engraved
via [verovio-rust](https://github.com/larsbrubaker/verovio-rust),
self-paced and tempo modes, right/left/both/auto hand modes with a
per-staff skill model, interval and chord-shape ladders with readiness
probes, the endless streak-centered micro-drill, survival mode (endless
reading on a sliding three-line feed, three lives, a score to beat),
practice-from-here on any clicked note, free play with take recording and
replay, the 61-piece bundled repertoire with search/filter/sort, per-user
profiles and helper switches, and computer-keyboard input
(A S D F G H J K = C4–C5, W E T Y U = sharps, Z/X octave).

Phase 2 so far: real audio out on both platforms (metronome clicks +
Hear It playing an OxiSynth-rendered CC0 piano soundfont through cpal /
WebAudio), the microphone backend (Goertzel-bank note detection), the
calibration sheet, player dialogs, Verovio-faithful spacing and
justification with multi-system line breaking, and the Swift app's visual
layout. Still to come: native MIDI (midir), Web MIDI, and engraving
refinements.

## Layout

| Path | Contents |
|---|---|
| `keyinsight-swift-reference/` | The pinned Swift source being ported (git submodule) |
| `docs/` | Porting rules, platform substitutions, architecture, build/deploy |
| `keyinsight-core/` | The entire app: engine, score model, skill model, widgets. `wasm32`-clean. |
| `keyinsight-native/` | Desktop shim (platform impl over `demo_wgpu::native_shell`) |
| `keyinsight-wasm/` | Browser shim (platform impl over `demo_wgpu::web_shell`) |
| `demo/` | Vite site that hosts the WASM build (GitHub Pages) |

## Building

```powershell
# Clone with the Swift reference submodule
git clone --recurse-submodules https://github.com/larsbrubaker/KeyInSight.git

# agg-gui is path-patched to a sibling checkout
git clone https://github.com/larsbrubaker/agg-gui.git ../agg-gui

cargo test --workspace     # build + tests
cargo run -p keyinsight-native   # desktop app

# Browser build
wasm-pack build keyinsight-wasm --target web --out-dir ../demo/public/pkg --no-typescript
cd demo; bun install; bun run dev
```

## Porting plan

Phase 1 is the truest port we can make — module by module in dependency
order, with the Swift test suite ported alongside as the acceptance gate.
Phase 2 takes over development in the agg-gui environment. The rules live in
[CLAUDE.md](CLAUDE.md) and the `docs/` directory:
[porting](docs/porting.md), [platform substitutions](docs/platform-substitutions.md)
(CoreMIDI → midir / Web MIDI, AVAudioEngine → cpal / WebAudio, Verovio →
SMuFL staff renderer, GRDB → storage trait),
[architecture](docs/architecture.md), and
[build & deploy](docs/build-and-deploy.md).

## License

MIT for the Rust code in this repository. The Swift reference submodule
remains under its upstream repository's terms.
