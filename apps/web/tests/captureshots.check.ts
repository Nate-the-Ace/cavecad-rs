import { cellFor, colorFor, counts, cssColor, fnv32, onBand, placeKey, pointsKey, roomAt, tagSpot, POINT_BUDGET } from '../src/lib/capture-pure.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

ok(fnv32('') === 0x811c9dc5 && fnv32('a') === 0xe40c292c, 'fnv32 is FNV-1a');
ok(colorFor('shot-1') === colorFor('shot-1'), 'a colour is stable');
const cs = ['s1', 's2', 's3', 's4', 's5'].map(colorFor);
ok(new Set(cs).size === cs.length, 'consecutive ids get different colours');
ok(cs.every((c) => c >= 0 && c <= 0xffffff), 'colours are 0xRRGGBB');

ok(cellFor(0) === 0.08, 'nothing: the finest cell');
ok(cellFor(1_000_000) === 0.08, 'one room fits at 8 cm');
const big = 400_000_000;
const c = cellFor(big);
ok(c > 0.08 && big * (0.02 / c) ** 2 <= POINT_BUDGET, `a big set is coarsened until it fits (${c})`);
ok(big * (0.02 / (c / 2)) ** 2 > POINT_BUDGET, 'and no coarser than needed');
ok(cellFor(1e15) <= 10, 'the cell stops growing');

const row = { file: 'a.cvcap', modifiedMs: 5, rigid: [0.1234567, 1.0004, 2, 3], anchor: [10, 20, 30.0001] };
ok(pointsKey(row, 0.08) === pointsKey({ ...row, anchor: [10, 20, 30.0002] }, 0.08), 'a sub-mm change keeps the key');
ok(pointsKey(row, 0.08) !== pointsKey({ ...row, modifiedMs: 6 }, 0.08), 'a rewritten file changes the key');
ok(pointsKey(row, 0.08) !== pointsKey(row, 0.16), 'a new cell changes the key');
ok(pointsKey(row, 0.08) !== pointsKey({ ...row, rigid: [0.2, 1, 2, 3] }, 0.08), 'a new placement changes the key');

const shown = new Map<string, [number, number, number]>([['st', [1, 2, 3]]]);
ok(roomAt({ stationId: 'st', anchor: [9, 9, 9] }, shown)?.join() === '1,2,3', 'a room follows its station as shown');
ok(roomAt({ stationId: 'gone', anchor: [9, 9, 9] }, shown)?.join() === '9,9,9', 'else its anchor');
ok(roomAt({ stationId: null, anchor: null }, shown) === null, 'nothing to place it by');

const n = counts([{ status: 'placed' }, { status: 'placed' }, { status: 'waiting' }, { status: 'gone' }, { status: 'unreadable' }]);
ok(n.placed === 2 && n.waiting === 1 && n.gone === 1 && n.unreadable === 1, 'counts');

ok(placeKey(row) === placeKey({ ...row }), 'a placement key is stable');
ok(placeKey(row) !== placeKey({ ...row, rigid: [0.2, 1, 2, 3] }), 'a moved shot slices again');
const band = onBand([[[-1, -0.5], [2, 1.5]]], { s: 10, z: 300 });
ok(band[0][0].join() === '9,299.5' && band[0][1].join() === '12,301.5', 'profile walls sit on the band at their station');
ok(cssColor(0xd63384) === '#d63384' && cssColor(0x0000ff) === '#0000ff', 'css colours keep six digits');
ok(tagSpot([]) === null, 'no lines, no tag');
ok(tagSpot([[[0, 0], [1, 0]], [[0, 0], [0, 3], [5, 2]]])?.join() === '0,3', 'the tag sits at the top of the longest line');

if (bad) { console.log(`${bad} failed`); process.exit(1); }
console.log('captureshots: ok');
