// Renders every MusicXML input through the pinned Verovio toolkit with the
// Swift app's exact option sets and writes `<name>.<mode>.p<page>.svg` (one
// file per page) plus `<name>.<mode>.timemap.json` into `out/` (gitignored).
// See README.md.
//
//   node render_goldens.mjs                 # all inputs
//   node render_goldens.mjs fur-elise       # only inputs whose name contains it
//   node render_goldens.mjs --all-modes     # render every input in both modes
//
// Modes follow the app: every input renders in `auto`; inputs that carry
// encoded system breaks (`<print new-system="yes"/>`, the survival feed
// window) ALSO render in `feed`, because that is the only time the Swift
// app asks for `breaks: encoded`. `--all-modes` forces both for everything
// (Verovio then puts one measure per system on break-less input).
//
// Inputs: ../../keyinsight-core/assets/pieces/*.musicxml (repertoire) and
// ./generated/*.musicxml (dump_exercises output).

import createVerovioModule from 'verovio/wasm';
import { VerovioToolkit, enableLog, LOG_ERROR } from 'verovio/esm';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const PIECES_DIR = path.resolve(here, '../../keyinsight-core/assets/pieces');
const GENERATED_DIR = path.resolve(here, 'generated');
export const OUT_DIR = path.resolve(here, 'out');

// Verovio pin the goldens were produced with; the harness refuses to run
// against anything else so a silently drifted `node_modules` cannot
// produce goldens that disagree with the committed ones.
export const EXPECTED_VEROVIO = '6.2.0';

// NotationRenderer.swift `init`: set once for the toolkit's lifetime.
export const BASE_OPTIONS = {
  adjustPageHeight: true,
  scale: 60,
  pageWidth: 1400,
  header: 'none',
  footer: 'none',
};

// NotationRenderer.swift `render(musicXML:feedLayout:)`: set on every
// call (options are sticky) — `auto` for normal pages, `feed` for the
// survival feed (`feedLayout: true`).
export const MODE_OPTIONS = {
  auto: { breaks: 'auto', spacingLinear: 0.25, spacingNonLinear: 0.6 },
  feed: { breaks: 'encoded', spacingLinear: 0.3, spacingNonLinear: 1.0 },
};

// Verovio assigns random xml:ids unless seeded. The Swift app does not
// seed (it never compares ids across runs); the harness does so that
// re-rendering is byte-identical. The seed is re-applied before every
// load so each piece's ids do not depend on what was rendered before it.
const XML_ID_SEED = 1;

export function hasEncodedBreaks(xml) {
  return /<print[^>]*new-(system|page)="yes"/.test(xml);
}

export function modesFor(xml, allModes) {
  return allModes || hasEncodedBreaks(xml) ? ['auto', 'feed'] : ['auto'];
}

export function listInputs(filter) {
  const inputs = [];
  for (const [dir, source] of [
    [PIECES_DIR, 'pieces'],
    [GENERATED_DIR, 'generated'],
  ]) {
    if (!fs.existsSync(dir)) continue;
    for (const file of fs.readdirSync(dir).sort()) {
      if (!file.endsWith('.musicxml')) continue;
      const name = file.slice(0, -'.musicxml'.length);
      if (filter && !name.includes(filter)) continue;
      inputs.push({ name, source, path: path.join(dir, file) });
    }
  }
  return inputs;
}

async function main() {
  const args = process.argv.slice(2);
  const allModes = args.includes('--all-modes');
  const filter = args.find((arg) => !arg.startsWith('--'));
  const module = await createVerovioModule();
  // Verovio's justification warnings are noise here; keep real errors.
  enableLog(LOG_ERROR, module);
  const toolkit = new VerovioToolkit(module);
  const version = toolkit.getVersion();
  if (!version.startsWith(EXPECTED_VEROVIO)) {
    console.error(
      `reference-harness: expected verovio ${EXPECTED_VEROVIO}, got ${version}. ` +
        'Run `npm ci` in tools/reference-harness (and update README/goldens if the pin moved).'
    );
    process.exit(2);
  }
  toolkit.setOptions(BASE_OPTIONS);

  fs.mkdirSync(OUT_DIR, { recursive: true });
  const inputs = listInputs(filter);
  // Stale pages from a previous run with a different page count would
  // otherwise survive; start clean.
  for (const file of fs.readdirSync(OUT_DIR)) {
    if (!filter || file.includes(filter)) fs.unlinkSync(path.join(OUT_DIR, file));
  }
  let ok = 0;
  let attempted = 0;
  const failures = [];
  for (const input of inputs) {
    const xml = fs.readFileSync(input.path, 'utf8');
    for (const mode of modesFor(xml, allModes)) {
      attempted += 1;
      toolkit.setOptions({ ...MODE_OPTIONS[mode], xmlIdSeed: XML_ID_SEED });
      let loaded = false;
      try {
        loaded = toolkit.loadData(xml);
      } catch (err) {
        failures.push(`${input.name} (${mode}): loadData threw ${err}`);
        continue;
      }
      if (!loaded) {
        failures.push(`${input.name} (${mode}): loadData returned false`);
        continue;
      }
      // The Swift app renders page 1 only (adjustPageHeight shrinks a
      // page but never grows it past pageHeight, so long pieces spill onto
      // further pages the app never shows). The goldens keep every page so
      // the note-id set stays complete; page index is recorded per system.
      const pageCount = toolkit.getPageCount();
      const stem = path.join(OUT_DIR, `${input.name}.${mode}`);
      for (let page = 1; page <= pageCount; page += 1) {
        fs.writeFileSync(`${stem}.p${page}.svg`, toolkit.renderToSVG(page));
      }
      const timemap = toolkit.renderToTimemap({});
      fs.writeFileSync(
        `${stem}.timemap.json`,
        JSON.stringify({ source: input.source, mode, pageCount, timemap }, null, 1)
      );
      ok += 1;
    }
  }
  console.log(
    `verovio ${version}: rendered ${ok}/${attempted} input x mode combinations ` +
      `(${inputs.length} inputs) into ${OUT_DIR}`
  );
  if (failures.length) {
    console.error('failures:\n  ' + failures.join('\n  '));
    process.exit(1);
  }
}

// extract_metrics.mjs imports the option tables from here; only run as a script.
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
