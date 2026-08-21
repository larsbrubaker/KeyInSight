# Verovio reference harness

Numeric layout goldens from the **real Verovio** (the engraver the Swift
app embeds), so `verovio-rust` can be diffed against it measure by measure
instead of by eye. This directory is MIT-side tooling: it only *runs*
Verovio (LGPL, from npm) and stores small text extracts — no Verovio
source or SVG blobs are committed.

```
tools/reference-harness/
  package.json, package-lock.json   verovio pin (npm)
  render_goldens.mjs                MusicXML -> out/*.svg + *.timemap.json
  extract_metrics.mjs               out/ -> goldens/*.elements.csv + *.layout.json
  generated/                        MusicXML dumped from the Rust generator (committed)
  goldens/                          the extracted goldens (committed)
  out/                              raw SVG + timemaps (gitignored, regenerable)
```

## Verovio pin

`package.json` pins **`verovio@6.2.0`**. The C++ reference submodule in
`verovio-rust` is tag `version-6.2.1` (commit `8d42439`); 6.2.1 was never
published to npm and its only change over 6.2.0 is "Fix missing file in
the resources (Python and cmd-line installation)" (CHANGELOG), so the npm
6.2.0 WASM build is engraving-identical to the 6.2.1 reference.
`render_goldens.mjs` refuses to run against any other version
(`EXPECTED_VEROVIO`).

**Goldens are regenerated only when this pin moves** (or when a committed
input changes — a piece in `keyinsight-core/assets/pieces/`, or the
generator / `MusicXmlEncoder` behind `generated/`). They are not touched
by verovio-rust work: verovio-rust is what gets compared *against* them.

## Regenerating

```powershell
# 0. once per checkout
cd tools/reference-harness
npm ci

# 1. generated exercises (deterministic; only changes with the generator/encoder)
cargo run -p keyinsight-core --bin dump_exercises

# 2. render with Verovio, then extract
node render_goldens.mjs        # -> out/ (110 inputs; 111 input x mode renders)
node extract_metrics.mjs       # -> goldens/

# smoke checks
node extract_metrics.mjs --check   # re-extract and compare with the committed goldens
```

`node render_goldens.mjs <substring>` renders only matching inputs;
`--all-modes` forces both option sets on every input (see Modes).

Smoke checks the scripts enforce:

- `render_goldens.mjs` reports `rendered N/M` and exits non-zero if any
  `loadData` fails (currently 111/111).
- `extract_metrics.mjs` fails loudly for any render whose set of drawn
  note ids is not exactly the set of ids the timemap turns `on` — that
  id set is the join key to verovio-rust (see Joining), so it must be
  complete. It also fails if staff-line spacing is not uniform.
- `extract_metrics.mjs --check` is the determinism gate: rendering +
  extracting twice must be byte-identical to what is committed (Verovio's
  random xml:ids are seeded with `xmlIdSeed: 1`, re-applied before every
  load, so ids do not depend on render order).

## Option sets (exactly the Swift app's)

From `keyinsight-swift-reference/Sources/KeyInSight/Notation/NotationRenderer.swift`:

| when                       | options                                                                              |
| -------------------------- | ------------------------------------------------------------------------------------ |
| toolkit init (once)        | `{"adjustPageHeight":true,"scale":60,"pageWidth":1400,"header":"none","footer":"none"}` |
| every render, `auto` mode  | `{"breaks":"auto","spacingLinear":0.25,"spacingNonLinear":0.6}`                       |
| every render, `feed` mode  | `{"breaks":"encoded","spacingLinear":0.3,"spacingNonLinear":1.0}`                     |

Everything else is Verovio's default (`unit` 9, `spacingStaff` 12,
`spacingSystem` 4 at the 6.2 defaults, Leipzig font, ...). The Swift app
then renders **page 1 only** (`renderToSVG(1, false)`); the harness
renders *every* page so long pieces keep their full note-id set
(`adjustPageHeight` shrinks a page but never grows it past `pageHeight`,
so e.g. `gymnopedie-1` spills onto page 2).

### Modes

`feed` is what the Swift app uses for the survival feed
(`feedLayout: true`) — always together with MusicXML that carries
`<print new-system="yes"/>` from `MusicXmlEncoder::encode_with_breaks`.
So the harness renders every input in `auto`, and additionally in `feed`
only for inputs that contain encoded breaks (today: `gen-feed-8m`).
`--all-modes` forces both for everything; on break-less input Verovio then
lays out one measure per system, which is real behaviour but not
something the app ever shows.

## Inputs

- `keyinsight-core/assets/pieces/*.musicxml` — the 61 repertoire pieces.
- `generated/*.musicxml` — 49 exercises written by
  `keyinsight-core/src/bin/dump_exercises.rs`: seeds 1-8 x hands
  {right, left, both} x {2, 4} measures (`gen-s<seed>-<hands>-<N>m`), plus
  `gen-feed-8m` = four stitched 2-measure `both` chunks (seeds 11-14)
  encoded with a system break every 2 measures. Seeds 1-4 walk the rhythm
  ladder (levels 0-3), seeds 5-8 use the full vocabulary; `seed % 3`
  picks the key (C/G/D); right-hand even seeds unlock chord shapes;
  `both` with `seed % 4 == 3` puts the melody in the bass. The seed list
  is part of the golden contract.

## Golden schema

### Units and coordinate conversion

All coordinates are **Verovio's SVG viewBox units, y-down, page-local,
exactly as printed in the SVG** (integers; the `<svg class="definition-scale">`
viewBox is `0 0 <pageWidth*10> <pageHeight*10>` and the body is offset by
the 500-unit page margin via `transform="translate(500, 500)"` — recorded
coordinates are *inside* that translate, i.e. relative to the page
margin). Verovio's internal MEI unit (half a staff space) is `unit` x
`DEFINITION_FACTOR` = 9 x 10 = **90** viewBox units, so

- **1 staff space = 180 units** (`layout.json` `units.staff_space`,
  *measured* from the drawn staff lines of every page, never assumed),
- staff spaces = `value / staff_space`; MEI units = `value / mei_unit`
  (`mei_unit = staff_space / 2 = 90`),
- rendered pixels = `value * scale / 100 / 10` = `value * 0.06`.

verovio-rust's `Layout` is in staff spaces, y-down, so the diff is
`rust_value ≈ golden_value / 180` after aligning the page origin (compare
positions relative to each system's first staff top line:
`y - staff_top`).

### `<name>.<mode>.elements.csv`

One row per drawn element, document order, columns:

| column       | meaning                                                                                   |
| ------------ | ----------------------------------------------------------------------------------------- |
| `kind`       | `note`, `rest`, `tie`, `tie-span` (second half of a tie across a system break), `clef`, `keysig`, `metersig` |
| `id`         | Verovio xml:id (notes: the id the timemap uses; `tie-span` repeats its tie's id)           |
| `page`       | 1-based page                                                                              |
| `system`     | 0-based system index across all pages                                                     |
| `measure`    | 0-based measure index across the piece (`-1` for system-level elements)                   |
| `staff`      | 1-based staff within the measure (1 = treble on a grand staff); blank for ties            |
| `onset`      | notes: index of the timemap entry (among entries with `on`) that turns the note on; `-1` otherwise |
| `slot`       | notes: position inside that entry's `on` list                                             |
| `qstamp`     | notes: onset in quarter notes                                                             |
| `x`, `y`     | notehead / rest / sign anchor (the `<use>` translate); ties: start point                  |
| `glyph`      | SMuFL codepoint of the notehead / rest / clef / first key accidental / meter digit         |
| `staff_top`  | y of the top line of the element's staff                                                  |
| `stem_x`, `stem_y1`, `stem_y2` | notes: stem line (y1 at the notehead, y2 the tip); chord notes share the chord's stem |
| `accid`, `accid_x` | notes: drawn accidental codepoint and x (blank when Verovio drew none — `<alter>` without `<accidental>` is gestural only) |
| `dot_x`, `dot_y` | first augmentation dot centre                                                          |
| `x2`, `y2`   | ties: end point                                                                           |
| `flag`       | notes: flag codepoint (Verovio flags every `<beam>`-less eighth)                           |

### `<name>.<mode>.layout.json`

`verovio`, `source` (`pieces` / `generated`), `mode`, the full `options`
used, `units` (`staff_space`, `mei_unit`), `page_count`, per-page
`width_px`/`height_px`/`viewbox_w`/`viewbox_h`, `system_count`,
`measure_count`, `systems[]` (`index`, `page`, `measure_count`,
`first_measure`, `staves[]` `{n, top, bottom}` from the system's first
measure, `barline_x[]` — one x per measure, the measure's closing
barline), and `counts` (`notes`, `rests`, `ties` incl. spanning halves,
`beams`, `flags`, `timemap_onsets`).

## Joining to verovio-rust

Verovio's ids are random strings; verovio-rust's are `note-N`. Both
expose a timemap with the same semantics (`on` lists per onset, tie
continuations included), so the join key is **(onset, slot)** — the
`onset`/`slot` columns — or equivalently the flat timemap order the Swift
app already relies on (`Rendered.noteIDs`).
