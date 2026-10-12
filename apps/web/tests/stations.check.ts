import { KINDS, checklistCsv, effectiveStatus, filterRows, kindCounts, type StationRow } from '../src/lib/station-table.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const row = (station: string, kinds: string[], extra: Partial<StationRow> = {}): StationRow => ({
  id: station, station, kinds, trips: [0], degree: 1, notes: [], note_text: '', lead_notes: [], z: -3.2, loops: [], flags: [], status: '', team: '', who: '', suggest: '', ...extra,
});
const rows = [
  row('A1', ['control']),
  row('A5', ['lead', 'openEnd', 'noted'], { note_text: 'LEAD W, tight', who: 'Sam' }),
  row('B2', ['openEnd']),
  row('C3', ['lead'], { status: 'pushed' }),
  row('D4', ['junction'], { team: 'rig here, "bolts"' }),
];
ok(effectiveStatus(rows[1]) === 'open', 'an unmarked lead is open');
ok(effectiveStatus(rows[3]) === 'pushed', 'a mark wins');
ok(effectiveStatus(rows[0]) === '', 'a plain station has none');
ok(filterRows(rows, { kinds: [], text: '', status: '' }).length === 5, 'no filter keeps all');
ok(filterRows(rows, { kinds: ['lead', 'junction'], text: '', status: '' }).map((r) => r.station).join() === 'A5,C3,D4', 'kinds are any-of');
ok(filterRows(rows, { kinds: [], text: 'SAM', status: '' }).length === 1, 'text searches who has it, ignoring case');
ok(filterRows(rows, { kinds: [], text: 'bolts', status: '' })[0].station === 'D4', 'text searches the team note');
ok(filterRows(rows, { kinds: [], text: '', status: 'open' }).map((r) => r.station).join() === 'A5', 'status filters on the effective status');
const counts = kindCounts(rows);
ok(counts.lead === 2 && counts.openEnd === 2 && counts.control === 1 && counts.flagged === undefined, 'counts per kind');
const csv = checklistCsv(rows, 'metric').split('\n');
ok(csv[0] === 'Station,Kinds,Trips,Elevation (m),Status,Note,Team notes,Assigned', 'header');
ok(csv[2].startsWith('A5,Lead; Open end; Noted,1,-3.2,open,"LEAD W, tight",,Sam'), `a lead row: ${csv[2]}`);
ok(csv[5].includes('"rig here, ""bolts"""'), 'quotes are doubled and commas protected');
ok(checklistCsv(rows, 'imperial').split('\n')[0].includes('(ft)'), 'imperial says feet');
ok(!/east|north|\bx\b|position/i.test(csv[0]), 'no position column');
ok(KINDS.length === 7 && KINDS[0].key === 'lead', 'the kinds');
console.log(bad ? 'FAILED' : 'station table checks ok'); process.exit(bad ? 1 : 0);
