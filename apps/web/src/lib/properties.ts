// Every setting an object exposes, classified, so that a selection of any objects can show the settings they have in
// common. A property is one named setting (a note, a left wall distance, a shot's tape, the benchmark mark...) with the
// objects it applies to, how to read it from one and how to write it to one. The properties of a selection are the ones
// that apply to EVERY object in it; a value the objects share is shown, one they differ in is VARIOUS, and a change is
// written to all of them. The list of properties is in `property-defs.ts` (it needs the engine); this file is the
// engine-free part, so it can be tested.

/** What sort of thing an object is: where its settings live. */
export type SubjectType = 'station' | 'shot' | 'drawn';
/** A setting's value: a number (a length in metres, an angle in degrees), a switch, words, or nothing set. */
export type Value = number | boolean | string | null;
/** How a property is edited. */
export type Control = 'length' | 'angle' | 'toggle' | 'text';

/** One object in a selection, with what the engine says about it (read once, when the selection changes). */
export interface Subject<D = unknown> {
  type: SubjectType;
  id: string;
  data: D;
}

/** The groups settings are listed under, in order. */
export const GROUPS = ['Note', 'Dimensions', 'Measured', 'Flags', 'Mark'] as const;
export type Group = (typeof GROUPS)[number];

/** One setting. */
export interface Prop<S extends Subject = Subject> {
  id: string;
  label: string;
  group: Group;
  control: Control;
  /** Smallest a length may be. */
  min?: number;
  /** Whether this setting exists for that object. */
  applies(s: S): boolean;
  /** The value now, or null when it is not set. */
  read(s: S): Value;
  /** Changes it; `ok` or `error: ...`. */
  write(s: S, v: Value): string;
}

/** The properties every one of `subjects` has, in the order of `props`. None when there are no subjects. */
export function commonProps<S extends Subject>(subjects: S[], props: Prop<S>[]): Prop<S>[] {
  return subjects.length ? props.filter((p) => subjects.every((s) => p.applies(s))) : [];
}

const same = (a: Value, b: Value): boolean => (typeof a === 'number' && typeof b === 'number' ? Math.abs(a - b) < 1e-9 : a === b);

/** The value the subjects share, or `various` when they differ. */
export function sharedValue<S extends Subject>(subjects: S[], p: Prop<S>): { value: Value; various: boolean } {
  const vals = subjects.map((s) => p.read(s));
  const various = vals.length > 0 && !vals.every((v) => same(v, vals[0]));
  return { value: various || vals.length === 0 ? null : vals[0], various };
}

/** Writes `v` to every subject that does not already hold it. Returns `ok`, or the last error. */
export function writeAll<S extends Subject>(subjects: S[], p: Prop<S>, v: Value): string {
  let out = 'ok';
  for (const s of subjects) {
    if (same(p.read(s), v)) continue;
    const r = p.write(s, v);
    if (r !== 'ok') out = r;
  }
  return out;
}

/** The properties grouped, in the order of `GROUPS`. */
export function byGroup<S extends Subject>(props: Prop<S>[]): { group: Group; props: Prop<S>[] }[] {
  return GROUPS.map((group) => ({ group, props: props.filter((p) => p.group === group) })).filter((g) => g.props.length > 0);
}
