import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { makeCatalog } from '../src/handbook/catalog.ts';
let bad = 0;
const check = (ok: boolean, what: string) => { if (!ok) { bad++; console.log('FAIL', what); } };

const cat = makeCatalog(
  [{ id: 'a', group: 'Tasks', title: 'Bringing data in' }, { id: 'map', group: 'Panels', title: 'Map', panel: 'map' }, { id: 'splays', group: 'Panels', title: 'Splays' }],
  { a: '# Bringing data in\n\nSplay shots come in too.', map: '# Map\n\nThe plan.', splays: '# Splays\n\nShots to walls.' },
);
check(cat.pages.map((p) => p.id).join() === 'a,map,splays', 'keeps index order');
check(cat.forPanel('map')?.id === 'map', 'page for a panel');
check(cat.byId('nope') === undefined, 'unknown id');
check(cat.search('splay').map((p) => p.id).join() === 'splays,a', 'title hits first');
check(cat.search('  ').length === 0, 'empty query');
check(cat.groups().map((g) => g.name).join() === 'Tasks,Panels', 'groups in first-appearance order');

// --- Coverage, links, images, sections, staleness (against the real files) ---
const root = new URL('../src/handbook/', import.meta.url);
const repoRoot = new URL('../../../../', root);
const index = JSON.parse(readFileSync(new URL('index.json', root), 'utf8')) as { id: string; panel?: string }[];
const ws = readFileSync(new URL('../src/lib/workspace.svelte.ts', import.meta.url), 'utf8');
const start = ws.indexOf('export const PANELS');
const panelsBlock = ws.slice(start, ws.indexOf('\n};', start));
const panels = [...panelsBlock.matchAll(/^\s*(?:\.\.\.\(\w+ \? \{ )?(\w+): \{ title:/gm)].map((m) => m[1]).filter((p) => p !== 'handbook');
check(panels.length >= 10 && panels.includes('survey') && panels.includes('map'), `PANELS parse looks wrong: ${panels.join()}`);
const files = readdirSync(new URL('pages/', root)).filter((f) => f.endsWith('.md')).map((f) => f.replace(/\.md$/, ''));
const shotFiles = existsSync(new URL('shots/', root)) ? readdirSync(new URL('shots/', root)) : [];
const ids = new Set(index.map((e) => e.id));
for (const p of panels) check(index.some((e) => e.panel === p), `panel "${p}" has no Handbook page`);
for (const e of index) if (e.panel) check(panels.includes(e.panel), `page "${e.id}" documents panel "${e.panel}", which is gone`);
for (const f of files) check(ids.has(f), `pages/${f}.md is not in index.json`);
for (const id of ids) check(files.includes(id), `index.json lists "${id}" with no pages/${id}.md`);
for (const e of index) {
  if (!files.includes(e.id)) continue;
  const text = readFileSync(new URL(`pages/${e.id}.md`, root), 'utf8');
  for (const m of text.matchAll(/(!?)\[[^\]]*\]\(([^)\s]+)\)/g)) {
    if (m[1]) check(shotFiles.includes(m[2]), `${e.id}: image ${m[2]} is not in shots/`);
    else if (!/^[a-z]+:|^#/.test(m[2])) check(ids.has(m[2]), `${e.id}: link to "${m[2]}" goes nowhere`);
  }
  if (e.panel) {
    for (const h of ['When you reach for it', 'How to use it', 'What can go wrong', 'Related']) check(new RegExp(`^## ${h}\\s*$`, 'm').test(text), `${e.id}: missing "## ${h}"`);
    check(/^!\[/m.test(text), `${e.id}: no screenshot`);
    check(/^# .+\r?\n\r?\n[^#\n!\r]/m.test(text), `${e.id}: no lead paragraph under the title`);
  }
}
if (process.env.HANDBOOK_STRICT === '1' && existsSync(new URL('shots.json', root))) {
  const shots = JSON.parse(readFileSync(new URL('shots.json', root), 'utf8')) as Record<string, { source: string; hash: string }>;
  for (const [file, s] of Object.entries(shots)) {
    const src = new URL(s.source, repoRoot);
    const now = existsSync(src) ? createHash('sha256').update(readFileSync(src)).digest('hex') : '';
    check(now === s.hash, `shots/${file} is older than ${s.source}: run node tools/handbook-shots.mjs <dev url>`);
  }
}

console.log(bad ? 'FAILED' : 'handbook ok');
process.exit(bad ? 1 : 0);
