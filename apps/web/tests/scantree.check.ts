import { scanKind, treeRows } from '../src/lib/scan-tree.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

ok(scanKind('a/p1.PNG') === 'image' && scanKind('x.jpeg') === 'image' && scanKind('s.pdf') === 'pdf', 'images and pdfs are scans');
ok(scanKind('notes.txt') === null && scanKind('.DS_Store') === null && scanKind('a/.hidden/p.png') === null && scanKind('__MACOSX/p.png') === null, 'other files and system junk are not');

const paths = ['2025/trip 2/p10.png', '2025/trip 2/p2.png', '2025/trip 10/p1.png', 'cover.jpg', 'readme.txt', '2025/.DS_Store'];
const rows = treeRows(paths, new Set());
ok(rows.map((r) => r.name).join('|') === '2025|trip 2|p2.png|p10.png|trip 10|p1.png|cover.jpg', 'folders first, numbers read as numbers: ' + rows.map((r) => r.name).join('|'));
ok(rows[0].count === 3 && rows[1].count === 2 && rows[1].depth === 1 && rows[2].depth === 2, 'counts and depth');
const closed = treeRows(paths, new Set(['2025/trip 2']));
ok(closed.length === rows.length - 2 && closed.find((r) => r.path === '2025/trip 2')?.collapsed === true, 'a collapsed folder hides what is in it');
ok(treeRows(paths, new Set(), 'P1').map((r) => r.name).join('|') === '2025|trip 2|p10.png|trip 10|p1.png', 'the filter keeps matching scans and their folders');
ok(treeRows([], new Set()).length === 0, 'empty folder');

if (bad) process.exit(1);
console.log('scan tree ok');
