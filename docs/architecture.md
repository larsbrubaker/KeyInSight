# Architecture — the agg-gui environment

## Crate layout

- 3-crate workspace: `keyinsight-core` (all logic + all widgets),
`keyinsight-native` (desktop shim), `keyinsight-wasm` (browser shim).
- **Single application, two minimal host shims.** `keyinsight-core` is the  
entire visible app. The window/canvas, wgpu surface, event loop, and ALL  
input forwarding (pointer, wheel, keyboard, clipboard, DPR,  
client-platform detection) live in the shared shells  
`demo_wgpu::native_shell` / `demo_wgpu::web_shell` (agg-gui workspace).  
The shims contain ONLY app-specific glue: the `KeyInSightPlatform` impl  
(MIDI, audio out, mic, storage) and per-frame clock ticks. Do not add  
generic plumbing to a shim — extend the shared shells instead so every  
agg-gui app inherits it.



## UI rules

- **All UI through agg-gui.** The staff/notation view, piano keyboard,
progress panel, side panel, and every sheet/dialog are agg-gui widgets
painting through `DrawCtx`. **No separate canvas/WebGL/SVG pipeline.**
If a needed primitive is missing, add it to `../agg-gui/agg-gui/src/…`
first.
- **No HTML/CSS UI in the WASM shell.** The browser side owns a single
canvas, the Web MIDI / WebAudio / getUserMedia permission requests, and
forwards results into Rust via `wasm-bindgen` exports. It must not draw
buttons/labels/status text. Input and the frame loop are Rust-owned
(`web_shell`); `main.ts` must not wire pointer/keyboard listeners or
drive `requestAnimationFrame`.
- **Typography / icons.** Text renders through agg-gui text widgets; icons
are Font Awesome glyphs via agg-gui's icon-font path. Music glyphs come
from a bundled SMuFL font (Bravura) through the same text stack.
- **No accounts, no network backend.** All state is local (storage trait);
all assets ship statically (bundled MusicXML pieces, fonts).



## Local development uses agg-gui as a path dep — improve it as you go

The workspace `Cargo.toml` redirects `agg-gui` to `../agg-gui/agg-gui` via
`[patch.crates-io]`. This is the default state — every commit assumes
contributors run with the path override active.

When KeyInSight needs an agg-gui feature that doesn't exist yet, **add
it to agg-gui** (not a one-off here). Workflow:

1. Make the change in `../agg-gui/agg-gui/src/…`.
2. Run KeyInSight against the patched local crate
  (`cargo check --workspace`).
3. When stable, publish a new agg-gui version (Lars handles this).
4. CI builds against the published crates.io version. CI clones
  `larsbrubaker/agg-gui` as a sibling so the patch resolves there too.

Standalone clone, if you don't already have it as a sibling:

```powershell
git clone https://github.com/larsbrubaker/agg-gui.git ../agg-gui
```

