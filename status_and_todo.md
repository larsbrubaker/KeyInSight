# KeyInSight — working status & TODO

*Working doc, not a history: remaining work + how to resume. Prune as items
land; delete this file when it's empty.*

## Resume here

Three repos, all pushed and green at this hand-off:

| Repo | main | Notes |
|---|---|---|
| KeyInSight | this commit | 367 tests, clippy `-D warnings`, native + wasm build; submodule `verovio-rust/` pinned to `d85ccff` |
| verovio-rust | `d85ccff` | engraving parity work in flight (see below); `verovio-cpp-reference/` must be initialized to read the C++ (`git submodule update --init --depth 1`) |
| agg-gui (sibling `../agg-gui`, path-patched) | `8ffc74a` (0.4.1) | SegmentedControl, Spinner, default/cancel actions, `NativeShellConfig::new().with_min_size`, scrollbar helpers public. **No branches** — commit on main |

Fresh machine:
```bash
git clone --recurse-submodules https://github.com/larsbrubaker/rust-apps.git
cd rust-apps/KeyInSight && cargo test --workspace && cargo run -p keyinsight-native
```
Rules: `CLAUDE.md` (orchestration pattern: plan in the main session, delegate to
`.claude/agents/{implementer,reviewer,fix-test-failures}.md`), `docs/*.md`.
Engraving parity is measured by `tools/reference-harness/` (Verovio 6.2 goldens;
`node render_goldens.mjs && node extract_metrics.mjs --check`) and gated by
`verovio-rust/tests/golden_metrics_tests.rs` (set `VEROVIO_GOLDENS_REQUIRED=1` to
fail instead of skip when goldens are absent).

## TODO — engraving parity (verovio-rust, numeric, Windows-doable)

Scoreboard at `d85ccff` (golden units; 180 = 1 staff space): system structure
exact 111/111; staff tops ≤ 2; note y exact; note x / barline x ≤ 45 outside
`KNOWN_DEVIATIONS` (moonlight-opening chords, gen-s4 stems — see the test).

1. **Step 8 — chords**: `Note::CalcNoteHeadShift` (seconds flip across the stem),
   `AdjustAccidXFunctor` column stacking, chord dot collision pass
   (`calcdotsfunctor.cpp`). Goldens: moonlight-opening, gymnopedie-1.
2. **Step 9 — ties**: direction follows the stem, notehead-edge endpoints with
   Verovio insets, `tieMidpointThickness`, ties as positioners in the vertical
   overflow (`vertical.rs`). Goldens: tie x2/y2 + apex.
3. Shrink `KNOWN_DEVIATIONS` to empty; tighten gates; update `docs/porting.md`
   tolerances (it must match the test constants).
4. Review `dd0c7e4` (stems) + `d85ccff` (follow-ups) — committed unreviewed when
   the session stopped (tests green).
5. KeyInSight data fix: add proper `<accidental>` elements (measure-carry rules)
   to pieces with out-of-key alters and no accidentals (fur-elise ×2,
   minuet-in-g-full, eine-kleine-nachtmusik, moonlight-opening,
   sheep-may-safely-graze); Swift shows these without sharps (upstream data bug);
   regenerate those goldens; check the generator encoder emits measure-aware
   accidentals; test that every bundled piece's out-of-key alters carry one.
6. Optional: light-heavy final barline needs `<barline>` in the data (Verovio
   6.2.1 does not add one) — parity with Swift is *plain*, so leave unless wanted.
7. KeyInSight timemap: the harness shows Verovio turning tie-stop notes *on* at
   their own onset; `Toolkit::build_timemap` folds continuations — decide.
8. `notation/controller.rs:427` comment about em-square glyph boxes is stale;
   ghost-note radius/tick placement could use the real notehead box now.

## TODO — UI parity (agg-gui UI; see the Swift UI sources)

Done: accent `#007AFF`, segmented pickers, tooltips (all but two), full-width
buttons where Swift has `.frame(maxWidth:.infinity)`, default/cancel actions,
spinner, Profile toggle order, level-meter palette, bottom-bar surface.

1. **Return leaks past the Calibration sheet** (review finding): the sheet is
   `with_key_passthrough(true)`; while Start/Done are hidden, Enter reaches the
   side-panel default (Next Exercise / Replay / Run It Back) behind the modal.
   Fix in agg-gui (`on_key_down` must not run `dispatch_root_action` while a
   modal is active) or give the calibration sheet a swallowing
   `with_default_action`.
2. **InfoRows wrapping** (`ui/info_rows.rs`): wrap long status/summary rows with a
   hanging indent (they clip at 272 px today), per-branch row gap (5 generic /
   6 survival+drill), `ICON_SCALE` 0.85 → 1.0, count-in "Ready… N" + BPM on one
   row, beat-dots→BPM gap 8; then wire the two remaining tooltips
   (`help::FOLLOWING_OCTAVE`, `help::STATS_SUPPRESSED`) — needs per-row hover
   targets or an agg-gui hook to submit a tooltip from a custom widget.
   (A partial split of info_rows.rs into a directory was discarded at stop.)
3. **Notation page styling** (`NotationController.swift` CSS): page padding
   16 v / 24 h; ghost note as a −20° oval, 2.5 px `#8a8a8a` stroke,
   `rgba(138,138,138,0.25)` fill; ghost ledger lines (2 px `#9a9a9a`,
   width headWidth×1.8, every space beyond the staff); ticks as bold 15 px
   ◂/▸ `#b8860b` 24 px above the notehead, 6 px left; verify HIT_PAD 10 and the
   8 inspect kinds.
4. Sheet chrome (needs agg-gui): ModalSheet min/ideal/max sizing + present/dismiss
   animation (Progress 780–1100×640–900, Library 640–900×440–800, About
   560–760×480–820); List/row chrome (separators, section headers) for Progress
   and Library; sheet padding ~16; center Calibration buttons; About italics via
   rich_text; `.thinMaterial` callout + `.bar` material + opacity fade; intrinsic
   width ComboBox; Semibold face for `.headline`; icon cleanup (distinct
   `CHECK_SEAL`, real piano-keys and metronome glyphs; bottom-bar glyph 14→13).
5. Add `keyinsight-native --screenshot <path> [--library|--survival|--progress|
   --about|--profile|--calibration]` (agg-gui `ScreenshotHandle` + demo-wgpu
   readback) for deterministic captures; `.claude/launch.json` for `demo/`
   (`bun install`, `bun run wasm`, `bun run dev`).

## TODO — visual matching (needs the Mac)

1. Build/run the pinned Swift app (`keyinsight-swift-reference`, `swift run`);
   capture reference screenshots of every view/state (training per hand mode,
   tempo, drill, survival mid-run + summary, free play, repertoire + chip,
   Library filters, Progress, About, Profile, Calibration, dialogs) — the
   Swift `--demo` run walks most states. No Swift screenshots exist anywhere yet.
2. Commit them as a `reference/` corpus; diff against Rust captures (item 5 above
   or the wasm demo in a browser at a fixed viewport).
3. Capture one end-to-end pinned exercise per seed from the macOS build as a
   cross-platform determinism test (SplitMix64 draws are now bit-exact).

## TODO — features (small)

- PWA polish for `demo/`: manifest + icons + favicon + head tags (Antidote/demo
  is the pattern) and a `sw.js` with a `__BUILD_ID__` cache (18 MB wasm).
- Web MusicXML import in `WasmPlatform` (hidden `<input type=file>` inside the
  user gesture) — record in `docs/platform-substitutions.md`.
- `split_mix64.rs` `next_f64_below`: `debug_assert!(total > 0.0)`.
- Web MIDI + wake lock were compiled but not exercised in a browser; midir hot-plug
  untested against hardware (`keyinsight-native --midi-smoke`).
- Minor review nits: `notation/widget/scroll.rs` re-derives ScrollView's track
  geometry (expose `ScrollbarGeometry::vertical_for` in agg-gui); override held
  by system index can drift across a re-wrap; `offset_x` negative on very narrow
  widgets; `verovio-rust` tie tests skip silently without the KeyInSight sibling;
  agg-gui main has 3 pre-existing `multi_touch_routing` test failures and a
  clippy backlog.

## Known rough edges

- KeyInSight CI builds against the pushed sibling agg-gui and the verovio-rust
  submodule pin — push those first.
- `Toolkit::layout()`/`render()` panic if called before `load_music_xml`.
- The session RNG seeds from wall time at launch; pass a fixed seed to
  `SessionEngine::new` for reproducible runs (`--demo` uses 42).
