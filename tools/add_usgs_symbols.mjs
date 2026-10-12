#!/usr/bin/env node
// Adds chosen symbols from the USGS Open-File Report 95-526 sheets (public domain) to the catalogue
// as the `usgs` set. The sheets carry the drawings and their cartographic specifications but, for
// most symbols, not their names (those are in the printed standard, OFR 95-525), so each pick says
// which reference number it wants and what to call it. Re-running replaces the `usgs` set.
//
//   node tools/add_usgs_symbols.mjs <folder with GSTSYM*.EPS> [tools/usgs_picks.json]
//
// A pick: { "sheet": "GSTSYM23.EPS", "ref": "2.44.1", "name": "Rapids", "category": "Water", "layer": "..." }
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { readSheet } from './extract_usgs_eps.mjs';

const DATA = path.resolve(import.meta.dirname, '../crates/cave-symbols/data/symbols.json');
const [dir, picksFile = path.resolve(import.meta.dirname, 'usgs_picks.json')] = process.argv.slice(2);
if (!dir) { console.log('usage: node tools/add_usgs_symbols.mjs <EPS folder> [picks.json]'); process.exit(1); }
const picks = JSON.parse(readFileSync(picksFile, 'utf8'));
const k4 = (v) => Math.round(v * 1e4) / 1e4;
const sheets = new Map();
const catalogue = JSON.parse(readFileSync(DATA, 'utf8')).filter((s) => s.set !== 'usgs');
let added = 0;
for (const pick of picks) {
  if (!sheets.has(pick.sheet)) sheets.set(pick.sheet, readSheet(readFileSync(path.join(dir, pick.sheet), 'latin1')));
  const { paths, texts } = sheets.get(pick.sheet);
  const refs = texts.filter((t) => /^2\.\d+\.\d+$/.test(t.s));
  const ref = refs.find((t) => t.s === pick.ref);
  if (!ref) { console.log('no such reference on that sheet:', pick.ref); continue; }
  // the symbol column sits just right of its reference number (the specifications are further right)
  const column = refs.filter((r) => Math.abs(r.x - ref.x) <= 5).sort((a, b) => b.y - a.y);
  const i = column.indexOf(ref);
  const top = ref.y + 14, bottom = (column[i + 1]?.y ?? ref.y - 60) + 14;
  const shapes = paths.filter((p) => {
    const xs = p.pts.map((q) => q[0]), ys = p.pts.map((q) => q[1]);
    const cx = (Math.min(...xs) + Math.max(...xs)) / 2, cy = (Math.min(...ys) + Math.max(...ys)) / 2;
    return cy <= top && cy > bottom && cx > ref.x + 20 && cx < ref.x + 135;
  });
  if (!shapes.length) { console.log('nothing drawn for', pick.ref); continue; }
  const pts = shapes.flatMap((p) => p.pts);
  const cx = (Math.min(...pts.map((p) => p[0])) + Math.max(...pts.map((p) => p[0]))) / 2, cy = (Math.min(...pts.map((p) => p[1])) + Math.max(...pts.map((p) => p[1]))) / 2;
  const far = Math.max(1e-9, ...pts.map((p) => Math.hypot(p[0] - cx, p[1] - cy)));
  catalogue.push({
    key: 'sym-usgs-' + pick.name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, ''),
    block: '', set: 'usgs', name: pick.name, uis: pick.name, category: pick.category ?? 'Other', layer: pick.layer ?? 'NOTES-ANNOTATION',
    prims: shapes.map((p) => ({ t: 'poly', pts: p.pts.map((q) => [k4((q[0] - cx) / far), k4((q[1] - cy) / far)]), ...(p.closed ? { closed: true } : {}), ...(p.fill && !p.stroke ? { fill: true } : {}) })),
  });
  added++;
}
catalogue.sort((a, b) => (a.category + a.name).localeCompare(b.category + b.name));
writeFileSync(DATA, JSON.stringify(catalogue) + '\n');
console.log(`${added} USGS symbols; catalogue now ${catalogue.length}`);
