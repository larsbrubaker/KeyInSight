# Claude Code Guidelines — KeyInSight

Port of **KeyInSight** (macOS Swift/SwiftUI piano sight-reading trainer) to
**Rust + agg-gui**: one core crate, native desktop + WASM/GitHub Pages.
Phase 1 is the truest possible port of the pinned submodule at
`keyinsight-swift-reference/`; Phase 2 takes over development here.

## Read before working (progressive discovery)


| When you are…                                                 | Read                                                                                              |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Porting any module from Swift                                 | `docs/porting.md` — reference layout, behavioral-matching rules, module order, forbidden patterns |
| Choosing a platform API (MIDI, audio, mic, storage, notation) | `docs/platform-substitutions.md` — the sanctioned Swift→Rust replacements and the trait seams     |
| Touching UI, the shims, or agg-gui                            | `docs/architecture.md` — crate layout, agg-gui-only UI rules, sibling path-dep workflow           |
| Building, testing, or deploying                               | `docs/build-and-deploy.md` — commands, demo site, CI/Pages pipeline                               |


## Non-negotiable rules

- **No stubs, no shortcuts.** No `todo!()`, `unimplemented!()`, or partial
implementations. If a dependency isn't ready, implement it first.
- **Test-first bug fixing.** Reproduce with a failing test, fix, verify.
Never commit a bug fix without the test that would have caught it.
- **Port the Swift tests with each module.** They are the acceptance gate;
never weaken a test to make it pass.
- **800-line file limit**, enforced by
`keyinsight-core/tests/file_line_count.rs`. Fix by real refactoring into
sibling modules — never by compressing code or bumping the limit.
- **All UI through agg-gui** — no HTML/CSS UI, no separate canvas pipeline.
  Missing primitives get added to `../agg-gui` first.
- **Light theme.** The app runs agg-gui's light visuals, and notation always
  renders as black ink on a light page (music is always light).
- **Notation goes through `verovio-rust`** (git submodule at `verovio-rust/`, its own LGPL repo) — never
  inline engraving code here (license separation).
- One green module per commit: `cargo build` clean, all tests pass.

## Quick commands

```powershell
cargo test --workspace          # build + all tests
cargo run -p keyinsight-native  # desktop app (or `cargo dev` for hot reload)
```

## Shell

Windows / **PowerShell**. Heredocs (`<<'EOF'`) don't work — use PowerShell
string variables with backtick-n (``n`) for newlines.

## Orchestration pattern

The main session acts as planner and orchestrator only — it should not write
or edit code directly. All implementation is delegated to the `implementer`
subagent (`.claude/agents/implementer.md`), one scoped step at a time. All
post-change review is delegated to the `reviewer` subagent
(`.claude/agents/reviewer.md`). When tests fail, delegate to the
`fix-test-failures` subagent (`.claude/agents/fix-test-failures.md`) — it
treats every failure as a real bug and resolves it through instrumentation
and root-cause analysis, never by weakening tests. The main session handles
only planning, architecture decisions, and synthesizing subagent results.
