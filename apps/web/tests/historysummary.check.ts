import { describeEntry } from '../src/lib/history-summary.ts';

let bad = false;
const ok = (c: boolean, m: string) => {
  if (!c) {
    bad = true;
    console.error('FAIL:', m);
  }
};
const ops = JSON.stringify([
  { stamp: { t: 1 }, action: { Set: { id: 'a', field: 'StationLabel', value: { Text: 'A1' } } } },
  { stamp: { t: 2 }, action: { Set: { id: 'b', field: 'StationLabel', value: { Text: 'A2' } } } },
  { stamp: { t: 3 }, action: { Create: { id: 'c', kind: 'Station' } } },
  { stamp: { t: 4 }, action: { Remove: { id: 'd', kind: 'Shot' } } },
]);
ok(describeEntry(ops) === 'set station label x2, added station, removed shot', 'a mixed entry reads as what it set, added and removed: ' + describeEntry(ops));
ok(describeEntry('nonsense') === 'an edit' && describeEntry('[]') === 'an edit' && describeEntry('{}') === 'an edit', 'anything unreadable is "an edit"');

if (bad) process.exit(1);
console.log('historysummary ok');
