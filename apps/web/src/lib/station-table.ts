// The station table's pure parts: which rows a filter keeps, the status a row shows, and the
// checklist file. The rows themselves come from the engine (derived from the survey every time);
// only the team's mark on a row is stored.

import { formatLength, type Units } from './units.ts';

export interface StationRow {
  id: string;
  station: string;
  kinds: string[];
  trips: number[];
  degree: number;
  notes: { text: string; trip: number }[];
  note_text: string;
  lead_notes: { text: string; trip: number }[];
  z: number | null;
  loops: { percent: number | null; error: number }[];
  flags: string[];
  status: string;
  team: string;
  who: string;
  suggest: string;
}

/** The kinds of place, in the order badges are listed, with the words people see. */
export const KINDS: { key: string; label: string; hint: string }[] = [
  { key: 'lead', label: 'Lead', hint: 'A note at the station says LEAD' },
  { key: 'openEnd', label: 'Open end', hint: 'The survey line stops here and nobody wrote it off' },
  { key: 'junction', label: 'Junction', hint: 'Three or more legs meet here' },
  { key: 'control', label: 'Control', hint: 'A fixed station, or where the survey starts' },
  { key: 'loop', label: 'Loop', hint: 'The station lies on a closed loop' },
  { key: 'noted', label: 'Noted', hint: 'There is a note written at it' },
  { key: 'flagged', label: 'Flagged', hint: 'A shot here could not be used' },
];

/** The marks a team can put on a station. */
export const STATUSES = ['open', 'assigned', 'pushed', 'closed', 'not a lead'];

/** The status a row shows: its mark, or "open" for an unmarked lead. */
export function effectiveStatus(row: Pick<StationRow, 'status' | 'kinds'>): string {
  if (row.status) return row.status;
  return row.kinds.includes('lead') ? 'open' : '';
}

export interface Query {
  /** Any of these kinds keeps a row; none means every kind. */
  kinds: string[];
  /** Case-insensitive, over the station name, its notes, the team's note and who has it. */
  text: string;
  /** The effective status; empty is any. */
  status: string;
}

/** The rows a query keeps. */
export function filterRows(rows: StationRow[], q: Query): StationRow[] {
  const text = q.text.trim().toLowerCase();
  return rows.filter((r) => {
    if (q.kinds.length && !q.kinds.some((k) => r.kinds.includes(k))) return false;
    if (text && !`${r.station} ${r.note_text} ${r.team} ${r.who}`.toLowerCase().includes(text)) return false;
    if (q.status && effectiveStatus(r) !== q.status) return false;
    return true;
  });
}

/** How many rows have each kind (for the filter buttons). */
export function kindCounts(rows: StationRow[]): Record<string, number> {
  const out: Record<string, number> = {};
  for (const r of rows) for (const k of r.kinds) out[k] = (out[k] ?? 0) + 1;
  return out;
}

const cell = (v: unknown): string => {
  const s = v === undefined || v === null ? '' : String(v);
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};

/**
 * The rows as CSV for a trip planner's checklist. Elevation only, never a position: a cave's
 * location does not leave the cave. The elevation is in the display unit.
 */
export function checklistCsv(rows: StationRow[], units: Units): string {
  const label = (k: string) => KINDS.find((x) => x.key === k)?.label ?? k;
  const lines = [['Station', 'Kinds', 'Trips', `Elevation (${units === 'imperial' ? 'ft' : 'm'})`, 'Status', 'Note', 'Team notes', 'Assigned'].join(',')];
  for (const r of rows) {
    lines.push(
      [
        cell(r.station),
        cell(r.kinds.map(label).join('; ')),
        cell(r.trips.map((t) => t + 1).join(' ')), // people count trips from 1
        cell(r.z === null ? '' : formatLength(r.z, units, 1).replace(/ (m|ft)$/, '')),
        cell(effectiveStatus(r)),
        cell(r.note_text),
        cell(r.team),
        cell(r.who),
      ].join(','),
    );
  }
  return lines.join('\n') + '\n';
}
