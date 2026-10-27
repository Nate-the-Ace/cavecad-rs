// Read-only mode for the engine: while the account is locked (an offline pass over 30 days old), every call
// that would change the cave (or replace it) is refused here, whatever part of the interface tried it. This is
// the guarantee: nothing done while locked reaches the cave.
//
// Every method of `engine` is listed below as one that reads or one that writes. A test
// (`tests/readonly.check.ts`) fails when a method of `engine` is in neither list, so a new engine call
// has to be classified when it is added.

/** Engine calls that change or replace the open cave. */
export const WRITES: readonly string[] = [
  'setRoomId',
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
  'savePattern',
  'removePattern',
  'pasteLines',
  'editLeader',
  // The in-cave survey: every call that can put a shot (or a wall, or an undo) into the cave.
  'surveyConnect',
  'surveyFrame',
  'surveyFlushLink',
  'surveyDisconnect',
  'surveySimStation',
  'surveySettleNow',
  'surveyAccept',
  'surveyUndoLast',
  'surveySetStandingWall',
  'scanLink',
  'scanUnlink',
  'scanAddAnchor',
  'scanRemoveAnchor',
  'scanSetMode',
  'scanSetTrim',
  'scanSetComplete',
  'newCave',
  'openDxf',
  'importSurvey',
  'importFolder',
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

/** Engine calls that only look. (`syncLoad` and `syncApply` are here on purpose: they are how a cave's edits
 *  arrive from the server, and nothing a person does can call them.) */
export const READS: readonly string[] = [
  'version',
  'log',
  'events',
  'clear',
  'fills',
  'fillMarks',
  'drawingScale',
  'roomId',
  'linetypes',
  'symbols',
  'stationRows',
  'stationDetail',
  'shotDetail',
  'notebookTrips',
  'notebookPage',
  'patternLibrary',
  'motifFromDrawing',
  'checkPattern',
  'linePatternMarks',
  'linePatternMarksNamed',
  'fillPatternMarks',
  'fillPatternMarksNamed',
  'lastAddedId',
  // Object snap only chooses a point: it never changes the cave.
  'snapCatalog',
  'snapPick',
  'snapCommit',
  'snapAcquire',
  'snapOnePick',
  'snapCancel',
  'snapLastPoint',
  'snapAlong',
  // The in-cave survey's view state (where the surveyor stands, held readings), the instrument catalog and the sketchpad geometry change nothing in the cave.
  'surveySyncTrip',
  'surveySnapshot',
  'surveyLinkDown',
  'surveySetMode',
  'surveySetBacksights',
  'surveySetAuto',
  'surveySkipBacksight',
  'surveyNextStation',
  'surveySetNames',
  'surveyDiscard',
  'instrumentCatalog',
  'instrumentIdentify',
  'sketchSource',
  'sketchViews',
  'sketchAnchor',
  'sketchPlace',
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
  'folderName',
  'folderPlan',
  'roomIdOfNative',
  'revision',
  'syncOutbox',
  'syncApply',
  'syncSnapshot',
  'syncLoad',
  'scene',
  'shading',
  'saveDxf',
  'surveyFormats',
  'exportSurvey',
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

/** Whether the engine refuses writes, and what it says. The account locks it (`setAccountLock`) while the
 *  offline pass is too old to keep editing. `on` is what the engine checks. */
export const readOnly = {
  on: false,
  why: '',
};

/** The account's lock: on while the offline pass is too old to keep editing. */
export function setAccountLock(on: boolean, why = ''): void {
  readOnly.on = on;
  readOnly.why = why;
}

/** The watcher's lock: on while the open cave is one this member only watches (a guest's Watcher seat). It refuses
 *  every change to that cave, but not opening or making another (those replace the cave and end the watching). */
export const watchLock = {
  on: false,
  why: "You're watching this cave: you can look but not change it.",
};

/** The writes a watcher may still make: each replaces the open cave with another one. */
export const REPLACES: readonly string[] = ['setRoomId', 'openDemo', 'newCave', 'openDxf', 'importSurvey', 'importFolder', 'openNative'];

/** Turns the watcher's lock on or off. */
export function setWatchLock(on: boolean): void {
  watchLock.on = on;
}

const writes = new Set(WRITES);
const replaces = new Set(REPLACES);

/** What a refused call gives back, in the shape its caller expects. */
function refusal(name: string, why: string): unknown {
  if (name === 'undo' || name === 'redo') return false;
  if (name === 'openDxf' || name === 'importSurvey' || name === 'importFolder') return { ok: false, message: why, warnings: [] as string[] };
  return `error: ${why}`;
}

/** Why a write is refused now, or '' when it may go ahead. */
function refusedWhy(name: string): string {
  if (readOnly.on) return readOnly.why || 'This cave is read-only.';
  if (watchLock.on && !replaces.has(name)) return watchLock.why;
  return '';
}

/** Wraps the engine so that, while `readOnly.on` (or the watcher's lock), writes are refused. Reads pass straight through. */
export function guardEngine<T extends object>(raw: T): T {
  return new Proxy(raw, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, receiver);
      if (typeof prop === 'string' && typeof value === 'function' && writes.has(prop)) {
        return (...args: unknown[]) => {
          const why = refusedWhy(prop);
          return why ? refusal(prop, why) : (value as (...a: unknown[]) => unknown)(...args);
        };
      }
      return value;
    },
  });
}
