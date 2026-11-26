import { byGroup, commonProps, sharedValue, writeAll, type Prop, type Subject } from '../src/lib/properties.ts';

let bad = false;
const ok = (c: boolean, m: string) => {
  if (!c) {
    bad = true;
    console.error('FAIL:', m);
  }
};

type S = Subject<{ note: string; tape?: number }>;
const written: string[] = [];
const note: Prop<S> = { id: 'note', label: 'Note', group: 'Note', control: 'text', applies: () => true, read: (s) => s.data.note, write: (s, v) => (written.push(`${s.id}=${v}`), 'ok') };
const tape: Prop<S> = { id: 'tape', label: 'Tape', group: 'Measured', control: 'length', applies: (s) => s.type === 'shot', read: (s) => s.data.tape ?? null, write: () => 'ok' };
const st: S = { type: 'station', id: 'a', data: { note: 'x' } };
const sh: S = { type: 'shot', id: 'b', data: { note: 'x', tape: 3 } };
const sh2: S = { type: 'shot', id: 'c', data: { note: 'y', tape: 3 } };

ok(commonProps([st, sh], [note, tape]).map((p) => p.id).join() === 'note', 'a station and a shot share only what both have');
ok(commonProps([sh, sh2], [note, tape]).map((p) => p.id).join() === 'note,tape', 'two shots share both');
ok(commonProps([], [note, tape]).length === 0, 'nothing selected, nothing shared');
ok(sharedValue([st, sh], note).various === false && sharedValue([st, sh], note).value === 'x', 'a value they hold alike is shown');
ok(sharedValue([sh, sh2], note).various === true && sharedValue([sh, sh2], note).value === null, 'a value that differs is VARIOUS');
ok(sharedValue([sh, sh2], tape).value === 3, 'equal numbers are shared');
ok(writeAll([sh, sh2], note, 'x') === 'ok' && written.join() === 'c=x', 'only the objects that differ are written');
ok(byGroup([tape, note]).map((g) => g.group).join() === 'Note,Measured', 'groups come in their fixed order');

if (bad) process.exit(1);
console.log('properties ok');
