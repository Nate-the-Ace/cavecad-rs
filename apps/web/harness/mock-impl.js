// An in-memory stand-in for the notebook half of the engine, just enough for the Survey panel. It answers
// the way the real calls do (`ok` or `error: ...`, the same JSON shapes). It is not the engine: it checks
// less, and lays a splay out under its station as the real page does.

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
  palette: () => '[]',
  symbols: () => '[]',
  log: () => {},
  console_events: () => '[]',
  revision: () => book.rev,
  edit_state: () => JSON.stringify({ undo: false, redo: false }),
  undo: () => false,
  redo: () => false,
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
