# Reference screenshots — pinned Swift KeyInSight

Captured on macOS (Retina, 2× — 2560×1144 px = the 1280×572 logical default
window incl. title bar) from the pinned `keyinsight-swift-reference` submodule
at `9fc4f78`, 2026-08-21. These are the visual-matching targets for the Rust
port (`docs/architecture.md`); compare against
`keyinsight-native --screenshot <path> [--library|--survival|--progress|--about|--profile|--calibration]`.

Regenerate with `tools/swift-reference-capture/` (needs the Swift build:
`cd keyinsight-swift-reference && swift build`, and System Events /
Screen Recording permission for the terminal):

| Dir | How | Contents |
|---|---|---|
| `swift/window/` | `capture.sh` (launch hooks) / `capture_click.sh` (AX clicks at window-relative logical points) | `training-default`, `hands-left`, `hands-both`, `tempo` (count-in), `tempo-calibration` (Latency Calibration dialog), `drill`, `freeplay`, `survival` (`--survival`), `repertoire-minuet` (`--piece minuet-in-g`), `library-one-hand` (`--library` + One hand filter), `progress`, `about`, `profile` (sliders icon), `profile-rename` (pencil) |
| `swift/demo/` | `capture_demo.sh` — window grabbed every ~2 s during `--demo`; `frames.log` maps frame → last `demo:` line | frames 3 (wrong-note ghost), 5 (exercise summary), 11/15 (tempo count-in), 21 (tempo wrong pitch), 23/26 (repertoire, practice-from-here), 28 (free play), 30 (drill / playback), 36/38 (survival two hands), 43 (survival summary) |
| `swift/notation/` | `KeyInSight --demo --snapshot-dir` (the Swift app's own WebView snapshot) | `exercise1-wrong-ghost`, `exercise2-wrong-ghost`, `tempo-complete`, `repertoire-complete`, `freeplay` |

Caveats: the generated exercise differs per launch (wall-clock seed), so
notation content is not pixel-comparable — compare chrome, layout, sheet
sizing, and the notation *style* (ghost note, ticks, cursor colours). The
Swift app reads/writes the real user database, so toggle state (hands mode,
"Resume Training") can leak between captures. At the 540-tall default the
Swift survival side panel overflows and clips the bottom bar (`survival.png`,
`demo/frame-0038.png`) — that is upstream behaviour, not a target.
