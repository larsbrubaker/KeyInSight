// Extracts compact numeric goldens from the SVG pages + timemaps that
// render_goldens.mjs wrote into `out/`, and writes them to `goldens/`:
//
//   goldens/<name>.<mode>.elements.csv   one row per note / rest / tie /
//                                        clef / keysig / metersig
//   goldens/<name>.<mode>.layout.json    page + system + barline metrics
//
//   node extract_metrics.mjs            # write goldens
//   node extract_metrics.mjs --check    # compare against committed goldens,
//                                       # exit 1 on any difference
//
// Coordinates are Verovio's SVG viewBox units, y-down, page-local, as
// printed in the SVG (integers). With the app's options the staff-line
// spacing is 180 units, so 1 staff space = 180 units and the MEI "unit"
// (half space) is 90 units; divide x / y by `staff_space` from layout.json
// to get staff spaces. The value is measured from the staff lines of every
// page rather than assumed. See README.md for the full schema.
//
// The extractor fails loudly when the set of note ids found in the SVG
// pages differs from the set of ids the timemap turns on — that is the
// join key between Verovio's ids and verovio-rust's `note-N` ids (timemap
// order), so it must be complete.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { OUT_DIR, EXPECTED_VEROVIO, BASE_OPTIONS, MODE_OPTIONS } from './render_goldens.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const GOLDENS_DIR = path.resolve(here, 'goldens');

const ELEMENT_COLUMNS = [
  'kind', // note | rest | tie | tie-span | clef | keysig | metersig
  'id', // Verovio xml:id (notes: the timemap id)
  'page', // 1-based page index
  'system', // 0-based system index across ALL pages
  'measure', // 0-based measure index across the piece
  'staff', // 1-based staff index within the measure (1 = treble on a grand staff)
  'onset', // notes: index of the timemap entry that turns the note on (-1 otherwise)
  'slot', // notes: position inside that entry's `on` list (document order)
  'qstamp', // notes: onset in quarter notes from the timemap
  'x', // notehead / rest / sign anchor x; ties: start x
  'y', // same anchor y; ties: start y
  'glyph', // SMuFL codepoint of the notehead / rest / clef / first sign glyph
  'staff_top', // y of the staff's top line (the element's staff)
  'stem_x', // notes: stem path x ('' when stemless)
  'stem_y1', // notes: stem path start y (notehead end)
  'stem_y2', // notes: stem path end y (tip)
  'accid', // notes: accidental codepoint ('' when none drawn)
  'accid_x', // notes: accidental x
  'dot_x', // notes / rests: first augmentation dot cx ('' when none)
  'dot_y', // notes / rests: first augmentation dot cy
  'x2', // ties: end x
  'y2', // ties: end y
  'flag', // notes: flag codepoint ('' when none)
];

// --- tiny SVG tag scanner -------------------------------------------------

const TAG_RE = /<(\/?)([a-zA-Z][\w:-]*)([^>]*?)(\/?)>/g;
const ATTR_RE = /([\w:-]+)="([^"]*)"/g;

function attrs(raw) {
  const out = {};
  for (const m of raw.matchAll(ATTR_RE)) out[m[1]] = m[2];
  return out;
}

function translateOf(transform) {
  const m = /translate\(\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*\)/.exec(transform ?? '');
  return m ? [Number(m[1]), Number(m[2])] : null;
}

function glyphOf(href) {
  const m = /#([0-9A-Fa-f]{4,5})-/.exec(href ?? '');
  return m ? m[1].toUpperCase() : '';
}

function lineOf(d) {
  const m = /M\s*(-?[\d.]+)[ ,]+(-?[\d.]+)\s*L\s*(-?[\d.]+)[ ,]+(-?[\d.]+)/.exec(d ?? '');
  return m ? m.slice(1, 5).map(Number) : null;
}

function tieEndsOf(d) {
  // "M x0,y0 C c1x,c1y c2x,c2y x1,y1 C ..." — start and first-curve end.
  const m = /M\s*(-?[\d.]+),(-?[\d.]+)\s*C\s*-?[\d.]+,-?[\d.]+\s+-?[\d.]+,-?[\d.]+\s+(-?[\d.]+),(-?[\d.]+)/.exec(
    d ?? ''
  );
  return m ? m.slice(1, 5).map(Number) : null;
}

/// Walks one SVG page; appends element rows and returns the page record.
function scanPage(svg, page, systemBase, measureBase, rows, counts) {
  const header = /<svg width="([\d.]+)px" height="([\d.]+)px"/.exec(svg);
  const viewBox = /viewBox="0 0 (\d+) (\d+)"/.exec(svg);
  const pageRecord = {
    page,
    width_px: header ? Number(header[1]) : null,
    height_px: header ? Number(header[2]) : null,
    viewbox_w: viewBox ? Number(viewBox[1]) : null,
    viewbox_h: viewBox ? Number(viewBox[2]) : null,
    systems: [],
  };
  // Only the drawn body matters; <defs> also holds <g id=...> glyphs.
  const bodyStart = svg.indexOf('class="page-margin"');
  const body = bodyStart >= 0 ? svg.slice(bodyStart) : svg;

  const stack = []; // [{cls, id, el}]
  let system = null;
  let measure = null; // {index, staves: []}
  let staff = null; // {n, top, bottom, lines: []}
  let current = null; // open note/rest/tie/clef/keysig/metersig row
  let chordStem = null; // stem of the enclosing chord, shared by its notes
  const staffLinesBySystem = [];
  const closeCurrent = () => {
    if (current) rows.push(current);
    current = null;
  };
  const blank = (kind, id) => ({
    kind,
    id,
    page,
    system: system ? system.index : -1,
    measure: measure ? measure.index : -1,
    staff: staff ? staff.n : '',
    onset: -1,
    slot: -1,
    qstamp: '',
    x: '',
    y: '',
    glyph: '',
    staff_top: staff ? staff.top : '',
    stem_x: '',
    stem_y1: '',
    stem_y2: '',
    accid: '',
    accid_x: '',
    dot_x: '',
    dot_y: '',
    x2: '',
    y2: '',
    flag: '',
  });
  const inside = (cls) => stack.some((s) => s.cls === cls);

  for (const m of body.matchAll(TAG_RE)) {
    const [, closing, tag, rawAttrs, selfClosing] = m;
    if (closing) {
      const popped = stack.pop();
      if (!popped) continue;
      if (popped.cls === 'note' || popped.cls === 'rest' || popped.cls === 'tie') closeCurrent();
      // (a spanning tie half is class "tie id-… spanning": cls is still 'tie')
      if (popped.cls === 'clef' || popped.cls === 'keySig' || popped.cls === 'meterSig') closeCurrent();
      if (popped.cls === 'chord') chordStem = null;
      if (popped.cls === 'staff') staff = null;
      if (popped.cls === 'measure') measure = null;
      if (popped.cls === 'system') system = null;
      continue;
    }
    const a = attrs(rawAttrs);
    const cls = (a.class ?? '').split(' ')[0];
    // Every open tag (g, text, tspan, svg ...) is pushed so the closing
    // tags balance; only <g class=...> carries structure.
    if (!selfClosing) stack.push({ cls: tag === 'g' ? cls : `<${tag}>`, id: a.id });

    if (tag === 'g') {
      switch (cls) {
        case 'system':
          system = { index: systemBase + pageRecord.systems.length, measures: [], barlines: [] };
          pageRecord.systems.push(system);
          staffLinesBySystem.push([]);
          break;
        case 'measure':
          measure = { index: measureBase + countMeasures(pageRecord), staves: [] };
          system.measures.push(measure);
          break;
        case 'staff':
          staff = { n: measure.staves.length + 1, top: null, bottom: null, lines: [] };
          measure.staves.push(staff);
          break;
        case 'chord':
          chordStem = { pending: true };
          break;
        case 'note':
          closeCurrent();
          current = blank('note', a.id);
          if (chordStem && !chordStem.pending) {
            current.stem_x = chordStem.x;
            current.stem_y1 = chordStem.y1;
            current.stem_y2 = chordStem.y2;
          }
          counts.notes += 1;
          break;
        case 'rest':
          closeCurrent();
          current = blank('rest', a.id);
          counts.rests += 1;
          break;
        case 'tie': {
          // A tie across a system break is drawn as two halves: the first
          // keeps the id, the second is `<g class="tie id-<id> spanning">`
          // at system level (no measure). Row kind `tie-span`, same id.
          closeCurrent();
          const classes = (a.class ?? '').split(' ');
          const spanning = classes.includes('spanning');
          const id = a.id ?? classes.find((c) => c.startsWith('id-'))?.slice(3) ?? '';
          current = blank(spanning ? 'tie-span' : 'tie', id);
          counts.ties += 1;
          break;
        }
        case 'clef':
        case 'keySig':
        case 'meterSig':
          closeCurrent();
          current = blank(cls.toLowerCase(), a.id);
          break;
        case 'beam':
          counts.beams += 1;
          break;
        case 'flag':
          counts.flags += 1;
          break;
        default:
          break;
      }
      if (selfClosing) {
        // `<g id class="keySig" />` — an empty key signature is a row too.
        if (cls === 'keySig') {
          closeCurrent();
        }
        if (cls === 'note' || cls === 'rest') {
          // Not produced by Verovio; guard the stack either way.
        }
      }
      continue;
    }

    if (tag === 'use') {
      const xy = translateOf(a.transform);
      const glyph = glyphOf(a['xlink:href'] ?? a.href);
      if (!xy || !current) continue;
      const parent = stack[stack.length - 1]?.cls;
      if (current.kind === 'note' && parent === 'notehead') {
        [current.x, current.y] = xy;
        current.glyph = glyph;
      } else if (current.kind === 'note' && parent === 'accid') {
        current.accid = glyph;
        current.accid_x = xy[0];
      } else if (current.kind === 'note' && parent === 'flag') {
        current.flag = glyph;
      } else if (current.kind === 'rest' && parent === 'rest') {
        [current.x, current.y] = xy;
        current.glyph = glyph;
      } else if (
        (current.kind === 'clef' && parent === 'clef') ||
        (current.kind === 'keysig' && parent === 'keyAccid') ||
        (current.kind === 'metersig' && parent === 'meterSig')
      ) {
        if (current.x === '') {
          [current.x, current.y] = xy;
          current.glyph = glyph;
        }
      }
      continue;
    }

    if (tag === 'path') {
      const parent = stack[stack.length - 1];
      const line = lineOf(a.d);
      if (parent?.cls === 'staff' && line && staff) {
        staff.lines.push(line[1]);
        staff.top = Math.min(...staff.lines);
        staff.bottom = Math.max(...staff.lines);
        if (staff.lines.length === 5) staffLinesBySystem.at(-1).push(staff.lines.slice());
      } else if (parent?.cls === 'barLine' && line) {
        // One <g class="barLine"> per measure; on a grand staff it holds
        // several segments (per staff + the joining span). Record x once.
        if (!system.barlines.length || system.barlines.at(-1).measure !== measure.index) {
          system.barlines.push({ measure: measure.index, x: line[0], segments: 0 });
        }
        system.barlines.at(-1).segments += 1;
      } else if (parent?.cls === 'stem' && line) {
        if (chordStem && chordStem.pending) {
          chordStem = { pending: false, x: line[0], y1: line[1], y2: line[3] };
        } else if (current && current.kind === 'note') {
          current.stem_x = line[0];
          current.stem_y1 = line[1];
          current.stem_y2 = line[3];
        }
      } else if (parent?.cls === 'tie' && current && current.kind.startsWith('tie')) {
        const ends = tieEndsOf(a.d);
        if (ends) [current.x, current.y, current.x2, current.y2] = ends;
      }
      continue;
    }

    if (tag === 'ellipse' && current && stack[stack.length - 1]?.cls === 'dots') {
      if (current.dot_x === '' && inside(current.kind === 'note' ? 'note' : 'rest')) {
        current.dot_x = Number(a.cx);
        current.dot_y = Number(a.cy);
      }
      continue;
    }
  }
  closeCurrent();
  pageRecord.staff_lines = staffLinesBySystem;
  return pageRecord;
}

function countMeasures(pageRecord) {
  return pageRecord.systems.reduce((n, s) => n + s.measures.length, 0);
}

// --- per piece ---------------------------------------------------------------

function extractOne(name, mode) {
  const stem = path.join(OUT_DIR, `${name}.${mode}`);
  const meta = JSON.parse(fs.readFileSync(`${stem}.timemap.json`, 'utf8'));
  const rows = [];
  const counts = { notes: 0, rests: 0, ties: 0, beams: 0, flags: 0 };
  const pages = [];
  let systemBase = 0;
  let measureBase = 0;
  for (let page = 1; page <= meta.pageCount; page += 1) {
    const svg = fs.readFileSync(`${stem}.p${page}.svg`, 'utf8');
    const record = scanPage(svg, page, systemBase, measureBase, rows, counts);
    systemBase += record.systems.length;
    measureBase += countMeasures(record);
    pages.push(record);
  }

  // Staff space measured from the rendered staves, never assumed.
  const spacings = new Set();
  for (const p of pages)
    for (const sys of p.staff_lines)
      for (const lines of sys) {
        const sorted = lines.slice().sort((a, b) => a - b);
        for (let i = 1; i < sorted.length; i += 1) spacings.add(sorted[i] - sorted[i - 1]);
      }
  if (spacings.size !== 1) {
    throw new Error(`${name}.${mode}: staff line spacing is not uniform: ${[...spacings].join(',')}`);
  }
  const staffSpace = [...spacings][0];

  // Timemap join: onset index / slot / qstamp per note id, and the id-set
  // equality check the goldens stand on.
  const byId = new Map();
  let onsetIndex = 0;
  for (const entry of meta.timemap) {
    if (!entry.on || !entry.on.length) continue;
    entry.on.forEach((id, slot) => {
      if (!byId.has(id)) byId.set(id, { onset: onsetIndex, slot, qstamp: entry.qstamp });
    });
    onsetIndex += 1;
  }
  const svgNoteIds = new Set(rows.filter((r) => r.kind === 'note').map((r) => r.id));
  const missingInSvg = [...byId.keys()].filter((id) => !svgNoteIds.has(id));
  const missingInTimemap = [...svgNoteIds].filter((id) => !byId.has(id));
  if (missingInSvg.length || missingInTimemap.length) {
    throw new Error(
      `${name}.${mode}: note-id set mismatch — ${missingInSvg.length} timemap ids not drawn ` +
        `[${missingInSvg.slice(0, 5)}], ${missingInTimemap.length} drawn notes not in timemap ` +
        `[${missingInTimemap.slice(0, 5)}]`
    );
  }
  for (const row of rows) {
    if (row.kind !== 'note') continue;
    const t = byId.get(row.id);
    row.onset = t.onset;
    row.slot = t.slot;
    row.qstamp = t.qstamp;
  }

  const csv = [ELEMENT_COLUMNS.join(',')]
    .concat(rows.map((r) => ELEMENT_COLUMNS.map((c) => String(r[c])).join(',')))
    .join('\n');

  const layout = {
    verovio: EXPECTED_VEROVIO,
    source: meta.source,
    mode,
    options: { ...BASE_OPTIONS, ...MODE_OPTIONS[mode] },
    units: {
      description:
        'SVG viewBox units, y-down, page-local; divide by staff_space for staff spaces ' +
        '(MEI unit = staff_space / 2).',
      staff_space: staffSpace,
      mei_unit: staffSpace / 2,
    },
    page_count: meta.pageCount,
    pages: pages.map((p) => ({
      page: p.page,
      width_px: p.width_px,
      height_px: p.height_px,
      viewbox_w: p.viewbox_w,
      viewbox_h: p.viewbox_h,
    })),
    system_count: pages.reduce((n, p) => n + p.systems.length, 0),
    measure_count: measureBase,
    systems: pages.flatMap((p) =>
      p.systems.map((s) => ({
        index: s.index,
        page: p.page,
        measure_count: s.measures.length,
        first_measure: s.measures.length ? s.measures[0].index : -1,
        // Staff line extents from the system's first measure.
        staves: (s.measures[0]?.staves ?? []).map((st) => ({ n: st.n, top: st.top, bottom: st.bottom })),
        barline_x: s.barlines.map((b) => b.x),
      }))
    ),
    counts: { ...counts, timemap_onsets: onsetIndex },
  };
  return { csv, layout };
}

function listRendered() {
  const names = [];
  for (const file of fs.readdirSync(OUT_DIR).sort()) {
    const m = /^(.*)\.(auto|feed)\.timemap\.json$/.exec(file);
    if (m) names.push({ name: m[1], mode: m[2] });
  }
  return names;
}

function main() {
  const check = process.argv.includes('--check');
  if (!fs.existsSync(OUT_DIR)) {
    console.error('reference-harness: out/ is empty — run `node render_goldens.mjs` first');
    process.exit(2);
  }
  fs.mkdirSync(GOLDENS_DIR, { recursive: true });
  const rendered = listRendered();
  const diffs = [];
  const failures = [];
  let written = 0;
  for (const { name, mode } of rendered) {
    let result;
    try {
      result = extractOne(name, mode);
    } catch (err) {
      failures.push(`${name}.${mode}: ${err.stack ?? err}`);
      continue;
    }
    const files = {
      [`${name}.${mode}.elements.csv`]: result.csv + '\n',
      [`${name}.${mode}.layout.json`]: JSON.stringify(result.layout, null, 1) + '\n',
    };
    for (const [file, content] of Object.entries(files)) {
      const target = path.join(GOLDENS_DIR, file);
      if (check) {
        const existing = fs.existsSync(target) ? fs.readFileSync(target, 'utf8') : null;
        if (existing !== content) diffs.push(file);
      } else {
        fs.writeFileSync(target, content);
        written += 1;
      }
    }
  }
  if (check) {
    const expected = new Set(rendered.flatMap(({ name, mode }) => [
      `${name}.${mode}.elements.csv`,
      `${name}.${mode}.layout.json`,
    ]));
    for (const file of fs.readdirSync(GOLDENS_DIR)) {
      if (!expected.has(file) && /\.(csv|json)$/.test(file)) diffs.push(`${file} (stale: no render)`);
    }
  }
  console.log(
    check
      ? `checked ${rendered.length} goldens: ${diffs.length} differ, ${failures.length} failed`
      : `wrote ${written} golden files for ${rendered.length} renders into ${GOLDENS_DIR}`
  );
  if (failures.length) console.error('failures:\n  ' + failures.join('\n  '));
  if (diffs.length) console.error('differences:\n  ' + diffs.join('\n  '));
  if (failures.length || diffs.length) process.exit(1);
}

main();
