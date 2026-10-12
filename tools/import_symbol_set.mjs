#!/usr/bin/env node
// Adds the symbols of another set (UIS, USGS, ...) from a folder of SVG files to the catalogue,
// and says which of them the catalogue already covers, so only the missing ones are added.
//
//   node tools/import_symbol_set.mjs --set uis --dir ~/Downloads/uis-svg [--names names.json]
//        [--category Formations] [--dry-run] [--replace]
//
// names.json (optional) maps a file name (without .svg) to { "name": "...", "uis": "...",
// "category": "...", "layer": "..." }. Without it a file's name becomes the symbol's name.
// A symbol counts as already covered when its name matches (ignoring case and punctuation) a name
// or UIS name already in the catalogue; covered symbols are skipped unless --replace is given.
//
// Each SVG is flattened to polylines and circles (curves, arcs, rectangles, ellipses and
// transforms are handled), centred on its bounding box, y flipped to point up, and scaled so its
// farthest point is at distance 1. Closed shapes with a solid fill keep it. Only artwork the
// licence of the set allows should be added.
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const DATA = path.resolve(import.meta.dirname, '../crates/cave-symbols/data/symbols.json');

// ---------- geometry helpers ----------
const mul = (a, b) => [a[0] * b[0] + a[2] * b[1], a[1] * b[0] + a[3] * b[1], a[0] * b[2] + a[2] * b[3], a[1] * b[2] + a[3] * b[3], a[0] * b[4] + a[2] * b[5] + a[4], a[1] * b[4] + a[3] * b[5] + a[5]];
const apply = (m, [x, y]) => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
const IDENT = [1, 0, 0, 1, 0, 0];

export function parseTransform(t = '') {
  let m = IDENT;
  for (const [, fn, args] of t.matchAll(/(\w+)\s*\(([^)]*)\)/g)) {
    const a = args.split(/[\s,]+/).filter(Boolean).map(Number);
    let n = IDENT;
    if (fn === 'matrix') n = a;
    else if (fn === 'translate') n = [1, 0, 0, 1, a[0], a[1] ?? 0];
    else if (fn === 'scale') n = [a[0], 0, 0, a[1] ?? a[0], 0, 0];
    else if (fn === 'rotate') {
      const r = (a[0] * Math.PI) / 180, c = Math.cos(r), s = Math.sin(r);
      n = [c, s, -s, c, 0, 0];
      if (a.length === 3) n = mul(mul([1, 0, 0, 1, a[1], a[2]], n), [1, 0, 0, 1, -a[1], -a[2]]);
    }
    m = mul(m, n);
  }
  return m;
}

const cubic = (p0, p1, p2, p3, n = 12) => Array.from({ length: n }, (_, i) => { const t = (i + 1) / n, u = 1 - t; return [u ** 3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t ** 3 * p3[0], u ** 3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t ** 3 * p3[1]]; });
const quad = (p0, p1, p2, n = 10) => Array.from({ length: n }, (_, i) => { const t = (i + 1) / n, u = 1 - t; return [u * u * p0[0] + 2 * u * t * p1[0] + t * t * p2[0], u * u * p0[1] + 2 * u * t * p1[1] + t * t * p2[1]]; });

/** SVG endpoint-parameterised arc to points (after the start point). */
function arc(p0, rx, ry, rot, large, sweep, p1) {
  if (!rx || !ry) return [p1];
  const phi = (rot * Math.PI) / 180, c = Math.cos(phi), s = Math.sin(phi);
  const dx = (p0[0] - p1[0]) / 2, dy = (p0[1] - p1[1]) / 2;
  const x1 = c * dx + s * dy, y1 = -s * dx + c * dy;
  rx = Math.abs(rx); ry = Math.abs(ry);
  const lam = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
  if (lam > 1) { rx *= Math.sqrt(lam); ry *= Math.sqrt(lam); }
  const num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
  const den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
  const k = (large === sweep ? -1 : 1) * Math.sqrt(Math.max(0, num / den));
  const cxp = (k * rx * y1) / ry, cyp = (-k * ry * x1) / rx;
  const cx = c * cxp - s * cyp + (p0[0] + p1[0]) / 2, cy = s * cxp + c * cyp + (p0[1] + p1[1]) / 2;
  const ang = (ux, uy, vx, vy) => Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy);
  const th1 = ang(1, 0, (x1 - cxp) / rx, (y1 - cyp) / ry);
  let dth = ang((x1 - cxp) / rx, (y1 - cyp) / ry, (-x1 - cxp) / rx, (-y1 - cyp) / ry);
  if (!sweep && dth > 0) dth -= 2 * Math.PI;
  if (sweep && dth < 0) dth += 2 * Math.PI;
  const n = Math.max(4, Math.ceil(Math.abs(dth) / (Math.PI / 12)));
  return Array.from({ length: n }, (_, i) => { const t = th1 + (dth * (i + 1)) / n; return [cx + rx * Math.cos(t) * c - ry * Math.sin(t) * s, cy + rx * Math.cos(t) * s + ry * Math.sin(t) * c]; });
}

/** A path's `d` as subpaths: [{ pts, closed }]. */
export function parsePath(d) {
  const toks = [...d.matchAll(/([MmLlHhVvCcSsQqTtAaZz])|(-?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)/g)].map((m) => m[1] ?? Number(m[2]));
  const subs = [];
  let cur = null, pos = [0, 0], start = [0, 0], cmd = '', lastC = null, lastQ = null, i = 0;
  const num = () => toks[i++];
  const flag = () => { const t = toks[i++]; return t; };
  const begin = (p) => { cur = { pts: [p], closed: false }; subs.push(cur); start = p; pos = p; };
  while (i < toks.length) {
    if (typeof toks[i] === 'string') cmd = toks[i++];
    else if (cmd === 'M') cmd = 'L'; else if (cmd === 'm') cmd = 'l';
    const rel = cmd === cmd.toLowerCase(), up = cmd.toUpperCase();
    const P = (x, y) => (rel ? [pos[0] + x, pos[1] + y] : [x, y]);
    const push = (...ps) => { if (!cur) begin(pos); cur.pts.push(...ps); pos = ps[ps.length - 1]; };
    if (up === 'M') begin(P(num(), num()));
    else if (up === 'L') push(P(num(), num()));
    else if (up === 'H') { const x = num(); push([rel ? pos[0] + x : x, pos[1]]); }
    else if (up === 'V') { const y = num(); push([pos[0], rel ? pos[1] + y : y]); }
    else if (up === 'C') { const a = P(num(), num()), b = P(num(), num()), e = P(num(), num()); push(...cubic(pos, a, b, e)); lastC = b; }
    else if (up === 'S') { const a = lastC && 'CcSs'.includes(prev) ? [2 * pos[0] - lastC[0], 2 * pos[1] - lastC[1]] : pos, b = P(num(), num()), e = P(num(), num()); push(...cubic(pos, a, b, e)); lastC = b; }
    else if (up === 'Q') { const a = P(num(), num()), e = P(num(), num()); push(...quad(pos, a, e)); lastQ = a; }
    else if (up === 'T') { const a = lastQ ? [2 * pos[0] - lastQ[0], 2 * pos[1] - lastQ[1]] : pos, e = P(num(), num()); push(...quad(pos, a, e)); lastQ = a; }
    else if (up === 'A') { const rx = num(), ry = num(), rot = num(), lg = flag(), sw = flag(), e = P(num(), num()); push(...arc(pos, rx, ry, rot, lg, sw, e)); }
    else if (up === 'Z') { if (cur) { cur.closed = true; pos = start; } cur = null; }
    var prev = cmd; // eslint-disable-line no-var
  }
  return subs;
}

// ---------- a small SVG reader ----------
const attr = (tag, name) => (tag.match(new RegExp(`\\s${name}\\s*=\\s*("([^"]*)"|'([^']*)')`)) ?? [])[2] ?? (tag.match(new RegExp(`\\s${name}\\s*=\\s*'([^']*)'`)) ?? [])[1];
const styleOf = (tag) => Object.fromEntries((attr(tag, 'style') ?? '').split(';').map((s) => s.split(':').map((x) => x?.trim())).filter((x) => x[0]));
const prop = (tag, name, inherited) => attr(tag, name) ?? styleOf(tag)[name] ?? inherited?.[name];

/** The drawable shapes of an SVG as { pts, closed, fill } (circles as { circle }), in SVG coordinates. */
export function readSvg(text) {
  const out = [];
  const stack = [{ m: IDENT, props: {} }];
  for (const [full, close, name, rest] of text.matchAll(/<(\/?)([a-zA-Z][\w:-]*)([^>]*?)\/?>/g)) {
    const selfClosing = full.endsWith('/>');
    if (close) { stack.pop(); continue; }
    const top = stack[stack.length - 1];
    const tag = ' ' + rest;
    const m = mul(top.m, parseTransform(attr(tag, 'transform')));
    const props = { fill: prop(tag, 'fill', top.props), stroke: prop(tag, 'stroke', top.props) };
    const filled = props.fill !== undefined && props.fill !== 'none';
    const draw = (pts, closed) => out.push({ pts: pts.map((p) => apply(m, p)), closed, fill: closed && filled && props.stroke === undefined ? true : closed && filled && props.stroke === 'none' });
    const n = (k, d = 0) => Number(attr(tag, k) ?? d);
    if (name === 'path') for (const s of parsePath(attr(tag, 'd') ?? '')) draw(s.pts, s.closed);
    else if (name === 'line') draw([[n('x1'), n('y1')], [n('x2'), n('y2')]], false);
    else if (name === 'polyline' || name === 'polygon') {
      const v = (attr(tag, 'points') ?? '').split(/[\s,]+/).filter(Boolean).map(Number);
      draw(Array.from({ length: Math.floor(v.length / 2) }, (_, k) => [v[2 * k], v[2 * k + 1]]), name === 'polygon');
    } else if (name === 'rect') { const [x, y, w, h] = [n('x'), n('y'), n('width'), n('height')]; draw([[x, y], [x + w, y], [x + w, y + h], [x, y + h]], true); }
    else if (name === 'circle' || name === 'ellipse') {
      const rx = name === 'circle' ? n('r') : n('rx'), ry = name === 'circle' ? n('r') : n('ry');
      draw(Array.from({ length: 32 }, (_, k) => [n('cx') + rx * Math.cos((k / 32) * 2 * Math.PI), n('cy') + ry * Math.sin((k / 32) * 2 * Math.PI)]), true);
    }
    if (!selfClosing && !/^(path|line|polyline|polygon|rect|circle|ellipse)$/.test(name)) stack.push({ m, props });
    else if (!selfClosing) stack.push({ m, props });
  }
  return out.filter((s) => s.pts.length >= 2);
}

/** Shapes → catalogue drawing: centred on the bounding box, y up, farthest point at distance 1. */
export function normalise(shapes) {
  const all = shapes.flatMap((s) => s.pts);
  if (!all.length) return [];
  const xs = all.map((p) => p[0]), ys = all.map((p) => p[1]);
  const cx = (Math.min(...xs) + Math.max(...xs)) / 2, cy = (Math.min(...ys) + Math.max(...ys)) / 2;
  const far = Math.max(1e-9, ...all.map((p) => Math.hypot(p[0] - cx, p[1] - cy)));
  const k = (v) => Math.round(v * 1e4) / 1e4;
  return shapes.map((s) => ({ t: 'poly', pts: s.pts.map((p) => [k((p[0] - cx) / far), k(-(p[1] - cy) / far)]), ...(s.closed ? { closed: true } : {}), ...(s.fill ? { fill: true } : {}) }));
}

const norm = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, '');

// ---------- the command ----------
if (import.meta.url === `file://${process.argv[1]}`) {
  const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1]?.startsWith('--') || all[i + 1] === undefined ? true : all[i + 1]] : [])).filter((x) => x.length));
  if (!args.set || !args.dir) { console.log('usage: node tools/import_symbol_set.mjs --set <uis|usgs|...> --dir <folder of svg> [--names names.json] [--category X] [--dry-run] [--replace]'); process.exit(1); }
  const names = args.names ? JSON.parse(readFileSync(args.names, 'utf8')) : {};
  const catalogue = JSON.parse(readFileSync(DATA, 'utf8'));
  const known = new Map();
  for (const s of catalogue) for (const n of [s.name, s.uis]) known.set(norm(n), s);
  const added = [], covered = [], empty = [];
  for (const file of readdirSync(args.dir).filter((f) => f.toLowerCase().endsWith('.svg')).sort()) {
    const id = file.replace(/\.svg$/i, '');
    const meta = names[id] ?? {};
    const name = meta.name ?? id.replace(/[-_]+/g, ' ').replace(/^\w/, (c) => c.toUpperCase());
    const prims = normalise(readSvg(readFileSync(path.join(args.dir, file), 'utf8')));
    if (!prims.length) { empty.push(file); continue; }
    const hit = known.get(norm(name)) ?? known.get(norm(meta.uis ?? ''));
    if (hit && !args.replace) { covered.push(`${name}  (already: ${hit.name}, ${hit.set})`); continue; }
    const key = `sym-${args.set}-${norm(name).slice(0, 40)}`.replace(/[^a-z0-9-]/g, '');
    added.push({ key, block: '', set: args.set, name, uis: meta.uis ?? name, category: meta.category ?? args.category ?? 'Other', layer: meta.layer ?? 'NOTES-ANNOTATION', prims });
  }
  console.log(`${added.length} new, ${covered.length} already covered, ${empty.length} unreadable`);
  for (const c of covered) console.log('  covered:', c);
  for (const e of empty) console.log('  no drawable shapes:', e);
  if (!args['dry-run'] && added.length) {
    const have = new Set(catalogue.map((s) => s.key));
    for (const a of added) { if (have.has(a.key)) console.log('  key already used, skipped:', a.key); else catalogue.push(a); }
    catalogue.sort((a, b) => (a.category + a.name).localeCompare(b.category + b.name));
    writeFileSync(DATA, JSON.stringify(catalogue) + '\n');
    console.log('catalogue updated:', DATA);
  }
}
