# KeyInSight — working status & TODO

*Working doc, not a history: remaining work + how to resume. Prune as items
land; delete this file when it's empty.*

## Resume here

State at 2026-08-21 late evening (Mac session). Commits since the hand-off —
KeyInSight: `851e359` (`--screenshot`, sheet launch hooks), `eaeb339` (Swift
reference corpus + capture tools), `a14132b` (verovio-rust pin → `fc6666b`),
`f9330a2` (Library sheet layout fix), `904d55e` (`--piece` regression test),
`2c56e0d` (headless layout tests for the other sheets), `58497ac` (labeled
pickers, tabular digits), `cf7a68d` (Return no longer leaks past the
Calibration sheet; UI item 1 done), `e18022a` (notation page styling; UI item 3
done except the follow-ups below), `30fc514` (review follow-ups: content-sized
picker tracks, `--screenshot` arg errors, shared harness, bin tests on).
agg-gui: `149db0d` (`with_screenshot`), `aac93b8` (`Font::with_tabular_digits`),
`8d665dc` (modal owns Enter/Escape), `3e989d5` (15 s capture budget, shared
font bytes). **Visually unverified** (screen was locked when they landed —
run `keyinsight-native --screenshot` for `--survival`, `--piece minuet-in-g`,
plain, and compare to `reference/swift/window/`): `e18022a` page padding /
ghost / ticks and `30fc514` picker track widths (Pacing/Hands are 27/8 px
narrower than Swift — agg-gui SegmentedControl padding/min width if wanted). verovio-rust: `fc6666b` (golden gate from the
submodule layout, per-category `KNOWN_DEVIATIONS`, CI parity job).
**Nothing pushed yet** — push agg-gui first (KeyInSight CI builds against it),
then verovio-rust, then KeyInSight. The reviewer pass over 851e359..58497ac is
done and its findings landed in 30fc514 / cf7a68d; e18022a and 30fc514
themselves are unreviewed.

Findings parked for later (not in the numbered lists):
- `ui/app.rs` ~288: the session seed is `host_now()*1000` at build time, ≈0 on
  every launch — runs are NOT wall-clock seeded (the "Known rough edges" entry
  below is wrong); decide whether to seed from the OS clock or keep determinism.
- agg-gui `SHAPE_CACHE` is keyed on the font's `Arc<Vec<u8>>` pointer — a font
  dropped and reallocated at the same address serves stale shaping.
- agg-gui `FlexColumn::layout` rounds child origin and height separately, so a
  half-pixel row spills 1 px past its slot (sheet tests allow 1 px slack).
- Side-panel picker tracks stretch to the row (flex 1); Swift's Pacing/Hands
  tracks stop at natural width — one-line anchor change if wanted.
- `sheets/player_dialogs.rs` has no layout test yet (cheap, same harness).
- Notation hover: Rust emits only `note`/`rest` inspect kinds; Swift's
  `preciseKindAt` fallback (NotationController.swift ~L379–391) also names
  clef/keySig/meterSig/accid/dots/barLine — hit-test `layout.elements` by
  `ElementKind` in `widget/mod.rs::route_hover`. `HIT_PAD` is applied in
  layout px in Rust, screen px in Swift (equal only at scale 1). No launch hook
  reaches a wrong-note ghost state for screenshots (the offscreen paint test in
  `widget/tests.rs` is the evidence today).
- agg-gui `dispatch_unconsumed_key` still walks the tree behind a non-passthrough
  modal (not observable today; ordering is subtle).

agg-gui `stash@{0}` holds someone's earlier WIP from detached `266ed7a`
(pin_platform_for_testing guard, mac CI matrix, `cargo dev-mac`) — not from
this session; rebase onto main or drop.

Three repos:

| Repo | main | Notes |
|---|---|---|
| KeyInSight | `f9330a2`+ | 372 tests, clippy `-D warnings`, native + wasm build; submodule `verovio-rust/` pinned to `fc6666b` |
| verovio-rust | `fc6666b` | golden gate: set `VEROVIO_GOLDENS_REQUIRED=1`; from inside the submodule run cargo with `--config 'patch.crates-io.agg-gui.path="/Users/larsbrubaker/Development/rust-apps/agg-gui/agg-gui"'` |
| agg-gui (sibling `../agg-gui`, path-patched) | `149db0d` | 0.4.1 + `with_screenshot`. agg-gui itself is not clippy-clean under `-D warnings` (131 pre-existing lints in `agg-gui/src`). **No branches** — commit on main |

Fresh machine:
```bash
git clone --recurse-submodules https://github.com/larsbrubaker/rust-apps.git
cd rust-apps/KeyInSight && cargo test --workspace && cargo run -p keyinsight-native
```
Rules: `CLAUDE.md` (orchestration pattern: plan in the main session, delegate to
`.claude/agents/{implementer,reviewer,fix-test-failures}.md`), `docs/*.md`.
Engraving parity is measured by `tools/reference-harness/` (Verovio 6.2 goldens;
`node render_goldens.mjs && node extract_metrics.mjs --check`) and gated by
`verovio-rust/tests/golden_metrics_tests.rs`.
Visual parity: Swift references in `reference/swift/` (README there; regenerate
with `tools/swift-reference-capture/` — macOS has no `timeout`; the Swift app
writes the real user DB, so restore hands mode to Right after captures); Rust
captures via `keyinsight-native --screenshot <png> [--library|--survival|--progress|--about|--profile|--calibration|--piece <slug>]`
(physical 2× pixels; one unexplained hang at 100 % CPU was seen once, re-run
completed in 2 s).

## TODO — engraving parity (verovio-rust, numeric, Windows-doable)

Scoreboard at `fc6666b` (golden units; 180 = 1 staff space): structure 111/111;
staff tops ≤ 2; note y / stem y / dot y exact; note x / stem x / barline x /
dot x ≤ 45; accidental x ≤ 45; ties scoreboard-only. `KNOWN_DEVIATIONS` is now
per (render, category): moonlight-opening/StaffTop (ties as positioners, step
9) and gymnopedie-1/AccidX (stacked accidentals, step 8).

1. **Step 8 — chords**: `Note::CalcNoteHeadShift` (seconds flip across the stem),
   `AdjustAccidXFunctor` column stacking, chord dot collision pass
   (`calcdotsfunctor.cpp`). Goldens: moonlight-opening, gymnopedie-1.
2. **Step 9 — ties**: direction follows the stem, notehead-edge endpoints with
   Verovio insets, `tieMidpointThickness`, ties as positioners in the vertical
   overflow (`vertical.rs`). Goldens: tie x2/y2 + apex.
3. Shrink `KNOWN_DEVIATIONS` to empty; tighten gates; update `docs/porting.md`
   tolerances (it must match the test constants).
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

1. ~~Return leaks past the Calibration sheet~~ — done (`cf7a68d`, agg-gui `8d665dc`).
2. **InfoRows wrapping** (`ui/info_rows.rs`): wrap long status/summary rows with a
   hanging indent (they clip at 272 px today), per-branch row gap (5 generic /
   6 survival+drill), `ICON_SCALE` 0.85 → 1.0, count-in "Ready… N" + BPM on one
   row, beat-dots→BPM gap 8; then wire the two remaining tooltips
   (`help::FOLLOWING_OCTAVE`, `help::STATS_SUPPRESSED`) — needs per-row hover
   targets or an agg-gui hook to submit a tooltip from a custom widget.
   (A partial split of info_rows.rs into a directory was discarded at stop.)
3. ~~Notation page styling~~ — done (`e18022a`); remaining: the six
   un-emitted inspect kinds, `HIT_PAD` units (see parked findings above).
4. Sheet chrome (needs agg-gui): ModalSheet min/ideal/max sizing + present/dismiss
   animation (Progress 780–1100×640–900, Library 640–900×440–800, About
   560–760×480–820); List/row chrome (separators, section headers) for Progress
   and Library; sheet padding ~16; center Calibration buttons; About italics via
   rich_text; `.thinMaterial` callout + `.bar` material + opacity fade; intrinsic
   width ComboBox; Semibold face for `.headline`; icon cleanup (distinct
   `CHECK_SEAL`, real piano-keys and metronome glyphs; bottom-bar glyph 14→13).
5. **Diffs from the Rust-vs-Swift captures** (besides 1–4 and the in-flight
   items above): Progress sheet's staff draws stems and misplaced ledger lines
   where Swift shows stemless noteheads with one ledger per note; Library rows
   lack hairline separators and the filter/sort controls are top-aligned
   instead of centred on the search field; survival/notation page margins far
   tighter than Swift (item 3); About lacks italics (item 4); the Rust About
   sheet is wider than Swift's.

## TODO — visual matching (needs the Mac)

Done: reference corpus (`reference/swift/`, items 1–2). Remaining:

1. Capture one end-to-end pinned exercise per seed from the macOS build as a
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
