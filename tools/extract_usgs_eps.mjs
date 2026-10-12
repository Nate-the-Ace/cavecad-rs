#!/usr/bin/env node
// Reads the USGS Open-File Report 95-526 symbol sheets (Adobe Illustrator 3 EPS, public domain) and
// writes every symbol as a row: its reference number (as in the printed standard), the words beside
// it, and its drawing as polylines. Used to pick the symbols a cave map wants out of ~900 geologic
// map symbols; the picks go in tools/usgs_picks.json and are added to the catalogue by
// tools/add_usgs_symbols.mjs.
//
//   node tools/extract_usgs_eps.mjs <folder with GSTSYM*.EPS> <out.json> [preview-dir]
import { readFileSync, readdirSync, writeFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';

const cubic = (p0, p1, p2, p3, n = 10) => Array.from({ length: n }, (_, i) => { const t = (i + 1) / n, u = 1 - t; return [u ** 3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t ** 3 * p3[0], u ** 3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t ** 3 * p3[1]]; });

/** One sheet's paths ({ pts, closed, fill, stroke }) and text ({ x, y, s }), in sheet coordinates (y up). */
export function readSheet(text) {
  const body = text.replace(/\r/g, '\n');
  const toks = [...body.matchAll(/\((?:\\.|[^()\\])*\)|[^\s()]+/g)].map((m) => m[0]);
  const paths = [], texts = [];
  let stack = [], cur = null, pos = [0, 0], tp = [0, 0], td = [0, 0], line = 0, wide = 1, dashed = false, fillOn = false;
  const dash = [];
  const start = (p) => { cur = { pts: [p], closed: false }; pos = p; };
  const finish = (fill, stroke) => { if (cur && cur.pts.length >= 2) paths.push({ ...cur, fill, stroke, w: wide, dashed }); cur = null; };
  for (const t of toks) {
    if (t.startsWith('(')) { stack.push(t); continue; }
    const num = Number(t);
    if (t !== '' && !Number.isNaN(num) && /^[-+.\d]/.test(t)) { stack.push(num); continue; }
    const a = stack;
    switch (t) {
      case 'm': start([a[a.length - 2], a[a.length - 1]]); break;
      case 'l': case 'L': if (cur) { pos = [a[a.length - 2], a[a.length - 1]]; cur.pts.push(pos); } break;
      case 'c': case 'C': if (cur) { const [x1, y1, x2, y2, x3, y3] = a.slice(-6); cur.pts.push(...cubic(pos, [x1, y1], [x2, y2], [x3, y3])); pos = [x3, y3]; } break;
      case 'v': case 'V': if (cur) { const [x2, y2, x3, y3] = a.slice(-4); cur.pts.push(...cubic(pos, pos, [x2, y2], [x3, y3])); pos = [x3, y3]; } break;
      case 'y': case 'Y': if (cur) { const [x1, y1, x3, y3] = a.slice(-4); cur.pts.push(...cubic(pos, [x1, y1], [x3, y3], [x3, y3])); pos = [x3, y3]; } break;
      case 'h': case 'H': if (cur) cur.closed = true; break;
      case 'f': case 'F': finish(true, false); break;
      case 's': case 'S': finish(false, true); break;
      case 'b': case 'B': finish(true, true); break;
      case 'n': case 'N': if (cur) cur.closed = cur.closed; cur = null; break;
      case 'w': wide = a[a.length - 1]; break;
      case 'd': dashed = a.some((x) => typeof x === 'number' && x > 0) && String(a).includes('['); break;
      case 'Tp': tp = [a[a.length - 3], a[a.length - 2]]; td = [0, 0]; break;
      case 'Td': td = [td[0] + a[a.length - 2], td[1] + a[a.length - 1]]; break;
      case 'Tx': case 'TX': { const s = a[a.length - 1]; if (typeof s === 'string') { const v = s.slice(1, -1).replace(/\\r/g, '').trim(); if (v) texts.push({ x: tp[0] + td[0], y: tp[1] + td[1], s: v }); } break; }
      case 'To': td = [0, 0]; break;
      default: break;
    }
    if (!/^(Tx|TX)$/.test(t) || true) stack = [];
    void fillOn; void line; void dash;
  }
  return { paths, texts };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [dir, out, preview] = process.argv.slice(2);
  if (!dir || !out) { console.log('usage: node tools/extract_usgs_eps.mjs <EPS folder> <out.json> [preview-dir]'); process.exit(1); }
  const rows = [];
  for (const f of readdirSync(dir).filter((x) => /^GSTSYM\d+\.EPS$/i.test(x)).sort()) {
    const { paths, texts } = readSheet(readFileSync(path.join(dir, f), 'latin1'));
    // reference numbers (2.x.y) mark the rows; the symbol, its name and notes lie on the same band
    const refs = texts.filter((t) => /^2\.\d+\.\d+$/.test(t.s)).sort((a, b) => b.y - a.y);
    const left = refs.filter((r) => r.x < 0);
    const cols = [...new Set(refs.map((r) => Math.round(r.x / 5) * 5))].sort((a, b) => a - b);
    for (const ref of refs) {
      const col = cols.findIndex((c) => Math.abs(c - ref.x) <= 5);
      const next = cols[col + 1] ?? Infinity;
      const same = refs.filter((r) => Math.abs(r.x - ref.x) <= 5).sort((a, b) => b.y - a.y);
      const i = same.indexOf(ref);
      const top = ref.y + 14, bottom = (same[i + 1]?.y ?? ref.y - 60) + 14;
      const inBand = (y) => y <= top && y > bottom - 0.01;
      const shapes = paths.filter((p) => { const ys = p.pts.map((q) => q[1]), xs = p.pts.map((q) => q[0]); const cy = (Math.min(...ys) + Math.max(...ys)) / 2, cx = (Math.min(...xs) + Math.max(...xs)) / 2; return inBand(cy) && cx > ref.x + 20 && cx < next - 5 && Math.max(...xs) - Math.min(...xs) < 200 && Math.max(...ys) - Math.min(...ys) < 90; });
      const words = texts.filter((t) => inBand(t.y) && t.x > ref.x + 20 && t.x < next - 5 && !/^2\.\d+\.\d+$/.test(t.s)).sort((a, b) => a.x - b.x || b.y - a.y).map((t) => t.s);
      rows.push({ sheet: f, ref: ref.s, words, shapes: shapes.map((p) => ({ pts: p.pts, closed: p.closed, fill: p.fill && !p.stroke })) });
    }
    void left;
  }
  writeFileSync(out, JSON.stringify(rows));
  console.log(`${rows.length} rows -> ${out}`);
  if (preview) {
    mkdirSync(preview, { recursive: true });
    const cell = (r) => { const all = r.shapes.flatMap((s) => s.pts); if (!all.length) return ''; const xs = all.map((p) => p[0]), ys = all.map((p) => p[1]); const x0 = Math.min(...xs), y1 = Math.max(...ys), w = Math.max(...xs) - x0, h = y1 - Math.min(...ys), k = Math.min(70 / Math.max(w, 1), 40 / Math.max(h, 1), 3); return `<div style="display:inline-block;width:150px;height:92px;vertical-align:top;font:9px system-ui;overflow:hidden"><svg width="150" height="52" viewBox="0 0 150 52">${r.shapes.map((s) => `<${s.closed ? 'polygon' : 'polyline'} points="${s.pts.map((p) => `${6 + (p[0] - x0) * k},${46 - (y1 - p[1]) * k * -1 - h * k}`).join(' ')}" fill="${s.fill ? '#000' : 'none'}" stroke="#000" stroke-width="0.8"/>`).join('')}</svg><br><b>${r.ref}</b> ${r.words.join(' | ').slice(0, 70)}</div>`; };
    const bySheet = {};
    for (const r of rows) (bySheet[r.sheet] ??= []).push(r);
    for (const [f, rs] of Object.entries(bySheet)) writeFileSync(path.join(preview, f.replace('.EPS', '.html')), `<body style="margin:6px;background:#fff">${rs.map(cell).join('')}</body>`);
  }
}
