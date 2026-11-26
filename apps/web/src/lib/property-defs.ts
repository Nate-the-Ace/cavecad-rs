// The properties of every kind of object (see `properties.ts`): what a station, a leg or splay, and a drawn object each
// expose, classified into groups so a selection can show what it shares. A property that two kinds both have (the Note,
// for instance) is ONE property here, which is what lets stations, legs and symbols be changed together.
import { engine, type ShotDetail, type ShotEdit, type StationDetail } from './engine';
import { cave } from './cave.svelte';
import { isStampKind } from './symbols';
import { commonProps, type Prop, type Subject, type Value } from './properties';
import { ALIGN_LABELS, ARROW_LABELS, FONT_LABELS, FRAME_LABELS, LINETYPE_LABELS, SHAPE_LABELS, parseStyle, styleText, withFlag, type LineStyle } from './line-style-text';
import { placedPoints, setShape, shapeOf } from './symbol-shape';
import { isFillKind } from './fills';
import { isSymbolKind } from './symbols';

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


// ---- drawn objects: lines, areas, symbols, text and leaders ----
const kindOf = (s: Subject): string => drawn(s).kind;
const isDrawn = (s: Subject): boolean => s.type === 'drawn';
/** Lines and areas (anything with a pattern of its own): not symbols, text or a custom fill. */
const hasPattern = (s: Subject): boolean => isDrawn(s) && !isStampKind(kindOf(s)) && kindOf(s) !== 'fill-custom';
const isArea = (s: Subject): boolean => isDrawn(s) && isFillKind(kindOf(s)) && kindOf(s) !== 'fill-custom';
const isPlainLine = (s: Subject): boolean => isDrawn(s) && !isStampKind(kindOf(s)) && !isFillKind(kindOf(s));
const hasWords = (s: Subject): boolean => isDrawn(s) && isStampKind(kindOf(s)) && (kindOf(s) === 'text' || kindOf(s) === 'callout' || !!drawn(s).text);
const isLeader = (s: Subject): boolean => isDrawn(s) && kindOf(s) === 'callout';
const SHAPED = ['ledge', 'pit', 'flowstone', 'rimstone', 'slope', 'dripline', 'glyphs'];

const styleOf = (s: Subject): LineStyle => parseStyle(drawn(s).style);
const setStyle = (s: Subject, patch: Partial<LineStyle>): string => engine.setLineStyle(s.id, styleText({ ...styleOf(s), ...patch }));
const times = (v: number): string => `×${v.toFixed(2)}`;

/** The draw types a line or an area may be changed to. */
const typeOptions = (area: boolean) => (): [string, string][] => {
  const out: [string, string][] = [];
  try {
    for (const g of engine.drawToolGroups()) for (const t of g.tools) if (!isStampKind(t.key) && isFillKind(t.key) === area) out.push([t.key, t.label]);
  } catch {
    /* the engine is not ready: no choices */
  }
  return out;
};

const styleSelect = (id: string, label: string, group: Prop['group'], field: 'lt' | 'arrow' | 'shape' | 'frame' | 'font' | 'align', options: [string, string][], applies: (s: Subject) => boolean): Prop => ({
  id,
  label,
  group,
  control: 'select',
  options: () => options,
  applies,
  read: (s) => styleOf(s)[field],
  write: (s, v) => setStyle(s, { [field]: String(v ?? '') }),
});
const styleRange = (id: string, label: string, group: Prop['group'], field: 'scale' | 'density' | 'size' | 'angle', min: number, max: number, step: number, applies: (s: Subject) => boolean, format: (v: number) => string = times): Prop => ({
  id,
  label,
  group,
  control: 'range',
  min,
  max,
  step,
  format,
  applies,
  read: (s) => styleOf(s)[field],
  write: (s, v) => setStyle(s, { [field]: Number(v) }),
});
const styleFlag = (id: string, label: string, group: Prop['group'], letter: string, applies: (s: Subject) => boolean, inverted = false): Prop => ({
  id,
  label,
  group,
  control: 'toggle',
  applies,
  read: (s) => (inverted ? !styleOf(s).flags.includes(letter) : styleOf(s).flags.includes(letter)),
  write: (s, v) => setStyle(s, { flags: withFlag(styleOf(s).flags, letter, inverted ? !v : !!v) }),
});

const shapeOfSubject = (s: Subject) => shapeOf(placedPoints(s.id, drawn(s).view) ?? []);

export const DRAWN_PROPERTIES: Prop[] = [
  // what a symbol is like
  {
    id: 'symbol.size',
    label: 'Size',
    group: 'Symbol',
    control: 'length',
    min: 0,
    applies: (s) => isDrawn(s) && isSymbolKind(kindOf(s)),
    read: (s) => shapeOfSubject(s)?.size ?? null,
    write: (s, v) => setShape(s.id, drawn(s).view, { size: Number(v) }),
  },
  {
    id: 'symbol.turn',
    label: 'Rotation',
    group: 'Symbol',
    control: 'angle',
    applies: (s) => isDrawn(s) && isSymbolKind(kindOf(s)),
    read: (s) => {
      const r = shapeOfSubject(s)?.rotation;
      return r === undefined ? null : Math.round(r);
    },
    write: (s, v) => setShape(s.id, drawn(s).view, { rotation: Number(v) }),
  },
  // how a line or an area is drawn
  {
    id: 'draw.kind',
    label: 'Type',
    group: 'Line',
    control: 'select',
    options: typeOptions(false),
    applies: isPlainLine,
    read: (s) => kindOf(s),
    write: (s, v) => engine.setLineKind(s.id, String(v)),
  },
  {
    id: 'draw.area',
    label: 'Area',
    group: 'Line',
    control: 'select',
    options: typeOptions(true),
    applies: isArea,
    read: (s) => kindOf(s),
    write: (s, v) => engine.setLineKind(s.id, String(v)),
  },
  styleSelect('draw.linetype', 'Linetype', 'Line', 'lt', LINETYPE_LABELS, hasPattern),
  styleRange('draw.scale', 'Linetype scale', 'Line', 'scale', 0.25, 4, 0.05, hasPattern),
  styleRange('draw.density', 'Hatch density', 'Line', 'density', 0.25, 4, 0.05, isArea),
  styleRange('draw.angle', 'Pattern angle', 'Line', 'angle', -90, 90, 1, isArea, (v) => `${v.toFixed(0)}°`),
  {
    id: 'draw.closed',
    label: 'Closed curve',
    group: 'Line',
    control: 'toggle',
    applies: isPlainLine,
    read: (s) => drawn(s).closed,
    write: (s, v) => engine.setLineClosed(s.id, !!v),
  },
  {
    id: 'draw.flip',
    label: 'Marks on the other side',
    group: 'Line',
    control: 'toggle',
    applies: (s) => isDrawn(s) && SHAPED.includes(kindOf(s)),
    read: (s) => drawn(s).flip,
    write: (s, v) => engine.setLineFlip(s.id, !!v),
  },
  // the words of a note, a text or a label
  styleSelect('text.font', 'Type face', 'Text format', 'font', FONT_LABELS, hasWords),
  styleRange('text.size', 'Size', 'Text format', 'size', 0.4, 4, 0.05, hasWords),
  styleFlag('text.bold', 'Bold', 'Text format', 'b', hasWords),
  styleFlag('text.italic', 'Italic', 'Text format', 'i', hasWords),
  styleFlag('text.underline', 'Underline', 'Text format', 'u', hasWords),
  styleSelect('text.align', 'Align', 'Text format', 'align', ALIGN_LABELS, hasWords),
  {
    id: 'text.color',
    label: 'Colour',
    group: 'Text format',
    control: 'color',
    applies: hasWords,
    read: (s) => styleOf(s).color,
    write: (s, v) => setStyle(s, { color: String(v ?? '') }),
  },
  // a leader: its arrow, its line and the frame round its note
  styleSelect('leader.arrow', 'Arrowhead', 'Leader', 'arrow', ARROW_LABELS, isLeader),
  styleSelect('leader.shape', 'Leader line', 'Leader', 'shape', SHAPE_LABELS, isLeader),
  styleSelect('leader.frame', 'Note frame', 'Leader', 'frame', FRAME_LABELS, isLeader),
  styleFlag('leader.outline', 'Frame outline', 'Leader', 'n', isLeader, true),
  styleFlag('leader.backing', 'Paper backing', 'Leader', 'f', isLeader),
  styleFlag('leader.halo', 'Letter outline', 'Leader', 'h', isLeader),
];

/** Every property, by the kinds of object it applies to. A property several kinds have is listed once. */
export const PROPERTIES: Prop[] = [
  ...DRAWN_PROPERTIES,
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
