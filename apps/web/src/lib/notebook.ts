// The Survey Notebook's pure parts: what the engine sends for a page, and how a cell's text is read
// and shown. Lengths go through `units.ts` (the View tab's unit decides what people read and type);
// compass and clino cells are degrees and never change with the unit.

import { lengthText, parseLength, type Units } from './units.ts';

export interface Lrud {
  left: number | null;
  right: number | null;
  up: number | null;
  down: number | null;
  left_open: boolean;
  right_open: boolean;
  up_open: boolean;
  down_open: boolean;
}
export type RowKind = 'station' | 'shot' | 'splay' | 'gap';
export interface NotebookRow {
  kind: RowKind;
  label: string;
  station: string | null;
  shot: string | null;
  anchor: boolean;
  lrud: Lrud | null;
  lrud_editable: boolean;
  note: string;
  note_source: string | null;
  distance: number | null;
  az_fs: number | null;
  az_bs: number | null;
  inc_fs: number | null;
  inc_bs: number | null;
  total: number | null;
  excluded: boolean;
  warn: string;
}
export interface NotebookClosure {
  kind: 'loop' | 'tie';
  shot: string;
  row: number;
  path: string[];
  horizontal: number;
  vertical: number;
  length: number;
  percent: number | null;
}
export interface NotebookPage {
  trip: string;
  index: number;
  name: string;
  date: string;
  team: string;
  instruments: string;
  unit: 'ft' | 'm';
  declination: number | null;
  rows: NotebookRow[];
  totals: { length: number; shots: number; splays: number; stations: number; warnings: number };
  closures: NotebookClosure[];
}
export interface TripChoice {
  id: string;
  index: number;
  name: string;
  date: string;
  shots: number;
}

export type Side = 'left' | 'right' | 'up' | 'down';
export const SIDES: Side[] = ['left', 'right', 'up', 'down'];
export const SIDE_LETTER: Record<Side, string> = { left: 'L', right: 'R', up: 'U', down: 'D' };

/** A passage dimension as the cell shows it: the number in the display unit, `P` for open passage. */
export function lrudText(value: number | null, open: boolean, u: Units): string {
  if (value !== null) return lengthText(value, u);
  return open ? 'P' : '';
}

/**
 * Reads a passage-dimension cell. Blank is not measured, `P` is open passage (looked, no wall),
 * anything else is a length in the display unit. `undefined` means it is not understood.
 */
export function parseLrud(text: string, u: Units): { value: number | null; open: boolean } | undefined {
  const t = text.trim();
  if (t === '') return { value: null, open: false };
  if (/^p$/i.test(t)) return { value: null, open: true };
  const m = parseLength(t, u);
  return m === null || m < 0 ? undefined : { value: m, open: false };
}

/** A compass or clino reading as the cell shows it (one decimal, no trailing zero). */
export function angleText(v: number | null): string {
  return v === null ? '' : `${+v.toFixed(1)}`;
}

/** Reads a compass (0 to 360) or clino (-90 to 90) cell. `null` is blank, `undefined` not understood. */
export function parseAngle(text: string, kind: 'compass' | 'clino'): number | null | undefined {
  const t = text.trim();
  if (t === '' || t === '--') return null;
  if (!/^[+-]?\d+(\.\d+)?$/.test(t)) return undefined;
  const v = parseFloat(t);
  if (kind === 'clino') return v >= -90 && v <= 90 ? v : undefined;
  return v >= 0 && v <= 360 ? v % 360 : undefined;
}

/** A station's dimensions with one side replaced. */
export function withSide(l: Lrud | null, side: Side, value: number | null, open: boolean): Lrud {
  const base: Lrud = l ?? { left: null, right: null, up: null, down: null, left_open: false, right_open: false, up_open: false, down_open: false };
  return { ...base, [side]: value, [`${side}_open`]: open };
}
