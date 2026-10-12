// Extracts the old app's cave symbols (the SYM_* blocks of its NSS template) into
// crates/cave-symbols/data/symbols.json: one entry per symbol with its names, category and
// vector drawing. Run:  node tools/extract_symbols.mjs <old repo root>
// Geometry is normalised to a unit radius (the farthest drawn point from the symbol's origin is at
// distance 1, y up), curves are flattened, so the web app only ever draws lines, circles and polylines.
import { readFileSync, writeFileSync } from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';

const root = process.argv[2];
if (!root) throw new Error('usage: node tools/extract_symbols.mjs <old repo root>');
const out = path.resolve(import.meta.dirname, '../crates/cave-symbols/data/symbols.json');

// ---- the catalogue (names, aliases, categories) from the old CsSymbols.js ----
const ctx = { CsSymbols: {}, isNull: (x) => x == null };
vm.createContext(ctx);
vm.runInContext(readFileSync(path.join(root, 'scripts/CaveSurvey/Core/CsSymbols.js'), 'utf8').replace(/^var CsSymbols = \{\};/m, ''), ctx);
const catalogue = new Map(ctx.CsSymbols.CATALOG.map((c) => [c.block, c]));

// ---- the drawings, from the template's BLOCKS section ----
const lines = readFileSync(path.join(root, 'templates/NSS_Cave_Template_PLAN.dxf'), 'utf8').split(/\r?\n/);
const pairs = [];
for (let i = 0; i + 1 < lines.length; i += 2) pairs.push([lines[i].trim(), lines[i + 1].trim()]);

const blocks = new Map();
let inBlocks = false, cur = null, ent = null, base = [0, 0], afterBlock = false;
const flush = () => { if (cur && ent) cur.ents.push(ent); ent = null; };
for (const [c, v] of pairs) {
  if (c === '2' && v === 'BLOCKS') inBlocks = true;
  if (!inBlocks) continue;
  if (c === '0' && v === 'ENDSEC') { flush(); break; }
  if (c === '0' && v === 'BLOCK') { flush(); cur = { name: '', base: [0, 0], ents: [] }; afterBlock = true; continue; }
  if (c === '0' && v === 'ENDBLK') { flush(); if (cur?.name) blocks.set(cur.name, cur); cur = null; afterBlock = false; continue; }
  if (!cur) continue;
  if (afterBlock) {
    if (c === '2') cur.name = v;
    if (c === '10') cur.base[0] = +v;
    if (c === '20') cur.base[1] = +v;
    if (c === '0') afterBlock = false; else continue;
  }
  if (c === '0') { flush(); ent = { type: v, g: [] }; continue; }
  if (ent) ent.g.push([+c, v]);
}

const num = (e, code, k = 0) => { const f = e.g.filter((x) => x[0] === code); return f[k] ? +f[k][1] : undefined; };
const rad = (d) => (d * Math.PI) / 180;
const arcPoints = (cx, cy, r, a0, a1, steps) => {
  while (a1 <= a0) a1 += 360;
  const n = Math.max(3, Math.ceil(((a1 - a0) / 360) * steps));
  return Array.from({ length: n + 1 }, (_, i) => [cx + r * Math.cos(rad(a0 + ((a1 - a0) * i) / n)), cy + r * Math.sin(rad(a0 + ((a1 - a0) * i) / n))]);
};
/** A polyline segment with a bulge (tan of a quarter of the arc angle) as arc points, excluding the start. */
const bulgeArc = (p, q, bulge) => {
  const theta = 4 * Math.atan(bulge), chord = Math.hypot(q[0] - p[0], q[1] - p[1]);
  const r = chord / (2 * Math.sin(Math.abs(theta) / 2));
  const mx = (p[0] + q[0]) / 2, my = (p[1] + q[1]) / 2;
  const h = r * Math.cos(theta / 2) * Math.sign(theta);
  const nx = -(q[1] - p[1]) / chord, ny = (q[0] - p[0]) / chord;
  const cx = mx + nx * h, cy = my + ny * h;
  const a0 = Math.atan2(p[1] - cy, p[0] - cx);
  const n = Math.max(2, Math.ceil(Math.abs(theta) / (Math.PI / 12)));
  return Array.from({ length: n }, (_, i) => { const a = a0 + (theta * (i + 1)) / n; return [cx + Math.abs(r) * Math.cos(a), cy + Math.abs(r) * Math.sin(a)]; });
};

const symbols = [];
for (const [name, b] of blocks) {
  if (!name.startsWith('SYM_')) continue;
  const prims = [];
  for (const e of b.ents) {
    const at = (x, y) => [x - b.base[0], y - b.base[1]];
    if (e.type === 'LINE') prims.push({ t: 'poly', pts: [at(num(e, 10), num(e, 20)), at(num(e, 11), num(e, 21))] });
    else if (e.type === 'CIRCLE') prims.push({ t: 'circle', c: at(num(e, 10), num(e, 20)), r: num(e, 40) });
    else if (e.type === 'ARC') prims.push({ t: 'poly', pts: arcPoints(num(e, 10) - b.base[0], num(e, 20) - b.base[1], num(e, 40), num(e, 50), num(e, 51), 48) });
    else if (e.type === 'LWPOLYLINE') {
      // bulges ride with their vertex: walk the group list in order
      const verts = [];
      for (const [c, v] of e.g) { if (c === 10) verts.push({ x: +v, y: 0, bulge: 0 }); else if (c === 20) verts[verts.length - 1].y = +v; else if (c === 42) verts[verts.length - 1].bulge = +v; }
      const closed = ((num(e, 70) ?? 0) & 1) === 1;
      const pts = [];
      verts.forEach((v, i) => {
        pts.push(at(v.x, v.y));
        const next = verts[(i + 1) % verts.length];
        if (v.bulge && (closed || i + 1 < verts.length)) pts.push(...bulgeArc([v.x, v.y], [next.x, next.y], v.bulge).slice(0, -1).map((p) => at(p[0], p[1])));
      });
      prims.push({ t: 'poly', pts, closed });
    }
  }
  const all = prims.flatMap((p) => (p.t === 'circle' ? [[Math.hypot(p.c[0], p.c[1]) + p.r, 0]] : p.pts));
  const extent = Math.max(1e-9, ...all.map((p) => Math.hypot(p[0], p[1])));
  const k = 1 / extent;
  const round = (n) => Math.round(n * k * 1e4) / 1e4;
  const cat = catalogue.get(name);
  const title = name.slice(4).split('_').map((w) => w[0] + w.slice(1).toLowerCase()).join(' ');
  symbols.push({
    key: 'sym-' + name.slice(4).toLowerCase().replace(/_/g, '-'),
    block: name,
    set: 'nss',
    name: cat?.nss ?? title,
    uis: cat?.uis ?? cat?.nss ?? title,
    category: cat?.category ?? 'Other',
    layer: cat?.layer ?? 'NOTES-ANNOTATION',
    prims: prims.map((p) => (p.t === 'circle' ? { t: 'circle', c: [round(p.c[0]), round(p.c[1])], r: round(p.r) } : { t: 'poly', pts: p.pts.map((q) => [round(q[0]), round(q[1])]), ...(p.closed ? { closed: true } : {}) })),
  });
}
symbols.sort((a, b) => (a.category + a.name).localeCompare(b.category + b.name));
writeFileSync(out, JSON.stringify(symbols) + '\n');
console.log(`${symbols.length} symbols -> ${out}`);
const missing = [...catalogue.keys()].filter((k) => !blocks.has(k));
if (missing.length) console.log('in the old catalogue but with no drawing:', missing.join(', '));
