// An in-memory stand-in for the notebook half of the engine, just enough for the Survey panel. It answers
// the way the real calls do (`ok` or `error: ...`, the same JSON shapes). It is not the engine: it checks
// less, and lays a splay out under its station as the real page does.

import { sketch, VIEW_NAME } from '../src/lib/sketch.svelte';

export const book = { n: 0, rev: 0, shots: [], walls: {}, edits: [] };
globalThis.__book = book;

const blankWalls = () => ({ left: null, right: null, up: null, down: null, left_open: false, right_open: false, up_open: false, down_open: false });

const row = (extra) => ({ kind: 'shot', label: '', station: null, shot: null, anchor: false, lrud: null, lrud_editable: false, note: '', note_source: null, distance: null, az_fs: null, az_bs: null, inc_fs: null, inc_bs: null, total: null, excluded: false, warn: '', ...extra });

function page(trip) {
  const rows = [];
  const seen = new Set();
  const station = (label, shot) => {
    if (seen.has(label)) return;
    seen.add(label);
    rows.push(row({ kind: 'station', label, station: label, shot, lrud: book.walls[label] ?? blankWalls(), lrud_editable: true }));
  };
  // legs in order, then each station's splays under it
  for (const s of book.shots.filter((x) => x.to)) {
    station(s.from, null);
    rows.push(row({ kind: 'shot', label: s.to, shot: s.id, distance: s.c.distance ?? null, az_fs: s.c.az_fs ?? null, az_bs: s.c.az_bs ?? null, inc_fs: s.c.inc_fs ?? null, inc_bs: s.c.inc_bs ?? null }));
    station(s.to, s.id);
  }
  for (const s of book.shots.filter((x) => !x.to)) {
    station(s.from, null);
    rows.push(row({ kind: 'splay', label: `${s.from}.1`, shot: s.id, distance: s.c.distance ?? null, az_fs: s.c.az_fs ?? null, inc_fs: s.c.inc_fs ?? null }));
  }
  return { trip, index: 0, name: 'Harness trip', date: '', team: '', instruments: '', unit: 'm', declination: null, rows, totals: { length: 0, shots: book.shots.length, splays: 0, stations: seen.size, warnings: 0 }, closures: [] };
}

const bump = (what) => {
  book.rev += 1;
  book.edits.push(what);
};

export const impl = {
  start: () => {},
  symbols: () => '[]',
  log: () => {},
  console_events: () => '[]',
  revision: () => book.rev,
  // Undo and redo are per panel, as in the app. The Plan, Profile and Cross section panes keep their own history
  // in the sketch store; the Survey panel's notebook scope has none in the mock.
  edit_state: (scope) => {
    const pane = sketch.paneOf(scope);
    return JSON.stringify(pane ? sketch.can(pane) : { undo: false, redo: false });
  },
  undo: (scope) => {
    const pane = sketch.paneOf(scope);
    return pane ? sketch.step(pane, 'undo') : false;
  },
  redo: (scope) => {
    const pane = sketch.paneOf(scope);
    return pane ? sketch.step(pane, 'redo') : false;
  },
  // The ribbon's Edit tab reads and changes the selected line through these; here the lines are the sketchpad's.
  line_info: (id) => {
    const k = sketch.find(id);
    return JSON.stringify(k ? { kind: k.kind, view: VIEW_NAME[k.view], closed: k.closed, flip: k.flip, text: k.text } : null);
  },
  set_line_kind: (id, kind) => (sketch.patch(id, { kind }), 'ok'),
  set_line_closed: (id, closed) => (sketch.patch(id, { closed }), 'ok'),
  set_line_flip: (id, flip) => (sketch.patch(id, { flip }), 'ok'),
  set_line_text: (id, text) => (sketch.patch(id, { text }), 'ok'),
  delete_line: (id) => (sketch.remove(id), 'ok'),
  // Shaped lines (ledge hachures, pit ticks) are laid out by cave-shape in Rust; the mock draws just the line.
  ornament: () => JSON.stringify({ spine: true, lines: [] }),
  notebook_trips: () => JSON.stringify([{ id: 't1', index: 0, name: 'Harness trip', date: '', shots: book.shots.length }]),
  notebook_page: (trip) => JSON.stringify(page(trip)),
  scan_view_stations: () => JSON.stringify([...new Set(book.shots.flatMap((s) => [s.from, s.to].filter(Boolean)))].map((label) => ({ id: label, label }))),
  notebook_add_shot: (trip, from, to, splay) => {
    if (!from.trim()) return 'error: a shot starts at a named station';
    if (!splay && (!to.trim() || from === to)) return 'error: a shot joins two different stations';
    book.shots.push({ id: `s${++book.n}`, from, to: splay ? null : to, c: {} });
    bump(`add ${from}${splay ? ' (splay)' : `-${to}`}`);
    return 'ok';
  },
  notebook_set_reading: (shot, which, value) => {
    const s = book.shots.find((x) => x.id === shot);
    if (!s) return 'error: no such shot';
    const v = value === '' ? null : Number(value);
    if (v !== null && ((which === 'distance' && v < 0) || (which.startsWith('inc') && Math.abs(v) > 90))) return 'error: that value is out of range';
    s.c[which] = v;
    bump(`${shot} ${which}`);
    return 'ok';
  },
  notebook_remove_shot: (shot) => {
    const i = book.shots.findIndex((x) => x.id === shot);
    if (i < 0) return 'error: no such shot';
    book.shots.splice(i, 1);
    bump(`remove ${shot}`);
    return 'ok';
  },
  notebook_set_walls: (target, w) => {
    const s = book.shots.find((x) => x.id === target);
    book.walls[s ? s.to : target] = JSON.parse(w);
    bump(`walls ${s ? s.to : target}`);
    return 'ok';
  },
};
