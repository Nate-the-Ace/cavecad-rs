// Read-only mode for the engine: while a person is only watching a class, every call that would change
// the cave (or replace it) is refused here, whatever part of the interface tried it. The panels, the
// ribbon and the keyboard are also kept from acting (see `classroom.svelte.ts`), but this is the
// guarantee: nothing a student does can reach their copy of the cave, so nothing can reach anyone else's.
//
// Every method of `engine` is listed below as one that reads or one that writes. A test
// (`tests/classroom.check.ts`) fails when a method of `engine` is in neither list, so a new engine call
// has to be classified when it is added.

/** Engine calls that change or replace the open cave. */
export const WRITES: readonly string[] = [
  'openDemo',
  'setShot',
  'setStationWalls',
  'setStationNote',
  'setStationBenchmark',
  'setStationMark',
  'notebookSetReading',
  'notebookSetWalls',
  'notebookSetNote',
  'notebookSetTripField',
  'notebookSetDeclination',
  'notebookAddShot',
  'notebookRemoveShot',
  'notebookRenameStation',
  'notebookNewTrip',
  'scanLink',
  'scanUnlink',
  'scanAddAnchor',
  'scanRemoveAnchor',
  'scanSetMode',
  'scanSetTrim',
  'scanSetComplete',
  'newCave',
  'openDxf',
  'openNative',
  'addSectionSketch',
  'placeSection',
  'addMapLine',
  'editLine',
  'setLineClosed',
  'setLineKind',
  'setLineText',
  'setLineFlip',
  'setLineStyle',
  'setDrawingScale',
  'deleteLine',
  'addSketch',
  'undo',
  'redo',
];

/** Engine calls that only look. (`syncLoad` and `syncApply` are here on purpose: they are how the teacher's
 *  cave and edits arrive, and nothing a person does can call them.) */
export const READS: readonly string[] = [
  'version',
  'log',
  'events',
  'clear',
  'fills',
  'fillMarks',
  'drawingScale',
  'linetypes',
  'symbols',
  'stationRows',
  'stationDetail',
  'shotDetail',
  'notebookTrips',
  'notebookPage',
  'scanLinks',
  'scanPlacements',
  'scanViewStations',
  'scanAlignment',
  'scanTrim',
  'scanCompleted',
  'shelfSummary',
  'shelfSummarize',
  'shelfSummarizeDemo',
  'shelfSummarizeNative',
  'revision',
  'syncOutbox',
  'syncApply',
  'syncSnapshot',
  'syncLoad',
  'scene',
  'shading',
  'saveDxf',
  'saveNative',
  'sectionStations',
  'sectionAt',
  'mapSections',
  'mapProfileSketches',
  'sheetOptions',
  'sheetPlan',
  'ornament',
  'flippedLines',
  'lineInfo',
  'mapLines',
  'drawTools',
  'drawToolGroups',
  'palette',
  'profileRuns',
  'profileBand',
  'passageMesh',
  'editState',
];

/** Whether the engine refuses writes, and what it says. Set by the classroom for students. */
export const readOnly = {
  on: false,
  /** The teacher asked that the cave not be kept: saving, exporting and printing are off, and it is cleared when the student leaves. */
  noKeep: false,
  why: 'This is a class: only the teacher can change the cave.',
};

const writes = new Set(WRITES);

/** What a refused call gives back, in the shape its caller expects. */
function refusal(name: string): unknown {
  if (name === 'undo' || name === 'redo') return false;
  if (name === 'openDxf') return { ok: false, message: readOnly.why, warnings: [] as string[] };
  return `error: ${readOnly.why}`;
}

/** Wraps the engine so that, while `readOnly.on`, every write is refused. Reads pass straight through. */
export function guardEngine<T extends object>(raw: T): T {
  return new Proxy(raw, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, receiver);
      if (typeof prop === 'string' && typeof value === 'function' && writes.has(prop)) {
        return (...args: unknown[]) => (readOnly.on ? refusal(prop) : (value as (...a: unknown[]) => unknown)(...args));
      }
      return value;
    },
  });
}
