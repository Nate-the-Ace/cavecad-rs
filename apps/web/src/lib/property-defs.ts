// The properties of every kind of object (see `properties.ts`): what a station, a leg or splay, and a drawn object each
// expose, classified into groups so a selection can show what it shares. A property that two kinds both have (the Note,
// for instance) is ONE property here, which is what lets stations, legs and symbols be changed together.
import { engine, type ShotDetail, type ShotEdit, type StationDetail } from './engine';
import { cave } from './cave.svelte';
import { isStampKind } from './symbols';
import { commonProps, type Prop, type Subject, type Value } from './properties';

export type DrawnInfo = NonNullable<ReturnType<typeof engine.lineInfo>>;
export type StationSubject = Subject<StationDetail> & { type: 'station' };
export type ShotSubject = Subject<ShotDetail> & { type: 'shot' };
export type DrawnSubject = Subject<DrawnInfo> & { type: 'drawn' };
export type AnySubject = StationSubject | ShotSubject | DrawnSubject;

const station = (s: Subject): StationDetail => s.data as StationDetail;
const shot = (s: Subject): ShotDetail => s.data as ShotDetail;
const drawn = (s: Subject): DrawnInfo => s.data as DrawnInfo;

type Edge = 'left' | 'right' | 'up' | 'down';
const EDGES: Edge[] = ['left', 'right', 'up', 'down'];
const EDGE_LABEL: Record<Edge, string> = { left: 'Left', right: 'Right', up: 'Up', down: 'Down' };

function wallProp(edge: Edge): Prop {
  return {
    id: `station.${edge}`,
    label: EDGE_LABEL[edge],
    group: 'Dimensions',
    control: 'length',
    min: 0,
    applies: (s) => s.type === 'station' && !!station(s).walls,
    read: (s) => station(s).walls![edge],
    write: (s, v) => editWalls(station(s), { [edge]: v as number | null }),
  };
}
function openProp(edge: Edge): Prop {
  return {
    id: `station.${edge}_open`,
    label: `${EDGE_LABEL[edge]} open`,
    group: 'Dimensions',
    control: 'toggle',
    applies: (s) => s.type === 'station' && !!station(s).walls,
    read: (s) => station(s).walls![`${edge}_open` as const],
    write: (s, v) => editWalls(station(s), { [`${edge}_open`]: !!v }),
  };
}
function editWalls(d: StationDetail, patch: Record<string, number | boolean | null>): string {
  const w = d.walls!;
  return engine.setStationWalls(d.id, { left: w.left, right: w.right, up: w.up, down: w.down, left_open: w.left_open, right_open: w.right_open, up_open: w.up_open, down_open: w.down_open, ...patch });
}

type ShotNumber = 'distance' | 'azimuth' | 'inclination' | 'back_azimuth' | 'back_inclination';
type ShotFlag = 'exclude_from_plot' | 'exclude_from_all' | 'no_adjust';
function editShot(d: ShotDetail, patch: Partial<ShotEdit>): string {
  const next: ShotEdit = { distance: d.distance, azimuth: d.azimuth, inclination: d.inclination, back_azimuth: d.back_azimuth, back_inclination: d.back_inclination, exclude_from_plot: d.exclude_from_plot, exclude_from_all: d.exclude_from_all, no_adjust: d.no_adjust, ...patch };
  return engine.setShot(d.id, next);
}
const shotNumber = (key: ShotNumber, label: string, control: 'length' | 'angle'): Prop => ({
  id: `shot.${key}`,
  label,
  group: 'Measured',
  control,
  min: control === 'length' ? 0 : undefined,
  applies: (s) => s.type === 'shot',
  read: (s) => shot(s)[key] as Value,
  write: (s, v) => editShot(shot(s), { [key]: v as number | null }),
});
const shotFlag = (key: ShotFlag, label: string): Prop => ({
  id: `shot.${key}`,
  label,
  group: 'Flags',
  control: 'toggle',
  applies: (s) => s.type === 'shot',
  read: (s) => shot(s)[key],
  write: (s, v) => editShot(shot(s), { [key]: !!v }),
});

/** Every property, by the kinds of object it applies to. A property several kinds have is listed once. */
export const PROPERTIES: Prop[] = [
  // Note: the words kept with a station, a shot, or a drawn symbol or text
  {
    id: 'note',
    label: 'Note',
    group: 'Note',
    control: 'text',
    applies: (s) => (s.type === 'station' ? station(s).notes.length > 0 : s.type === 'shot' ? true : isStampKind(drawn(s).kind) || drawn(s).kind === 'text' || drawn(s).kind === 'callout'),
    read: (s) => (s.type === 'station' ? (station(s).notes[0]?.text ?? '') : s.type === 'shot' ? shot(s).note : drawn(s).text),
    write: (s, v) => (s.type === 'station' ? engine.setStationNote(station(s).notes[0].source, String(v ?? '')) : s.type === 'shot' ? engine.setStationNote(s.id, String(v ?? '')) : engine.setLineText(s.id, String(v ?? ''))),
  },
  ...EDGES.map(wallProp),
  ...EDGES.map(openProp),
  shotNumber('distance', 'Tape', 'length'),
  shotNumber('azimuth', 'Azimuth', 'angle'),
  shotNumber('inclination', 'Inclination', 'angle'),
  shotNumber('back_azimuth', 'Back azimuth', 'angle'),
  shotNumber('back_inclination', 'Back inclination', 'angle'),
  shotFlag('exclude_from_plot', 'Not on map'),
  shotFlag('exclude_from_all', 'Void shot'),
  shotFlag('no_adjust', 'Hold fixed'),
  {
    id: 'station.benchmark',
    label: 'Benchmark',
    group: 'Mark',
    control: 'toggle',
    applies: (s) => s.type === 'station',
    read: (s) => station(s).benchmark,
    write: (s, v) => engine.setStationBenchmark(s.id, !!v),
  },
];

/** What a selection is made of: survey things picked (`stations`, `shots`: ids) and drawn objects selected (ids). */
export function loadSubjects(sel: { stations: string[]; shots: string[]; drawn: string[] }): AnySubject[] {
  const out: AnySubject[] = [];
  for (const id of sel.stations) {
    const data = engine.stationDetail(id, cave.adjusted);
    if (data) out.push({ type: 'station', id, data });
  }
  for (const id of sel.shots) {
    const data = engine.shotDetail(id);
    if (data) out.push({ type: 'shot', id, data });
  }
  for (const id of sel.drawn) {
    const data = engine.lineInfo(id);
    if (data) out.push({ type: 'drawn', id, data });
  }
  return out;
}

/** The properties a selection shares. */
export const sharedProps = (subjects: AnySubject[]): Prop[] => commonProps(subjects, PROPERTIES);
