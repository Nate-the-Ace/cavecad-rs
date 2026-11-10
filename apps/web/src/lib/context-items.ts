// What the right-click menu offers in the drawing views (Map, Profile, Cross Section): on a drawn line, on a
// station, and on empty space. Each view finds what was pressed on and opens the menu with one of these lists;
// the actions are the ones the ribbon and the keyboard already have, so the menu is a shortcut, not a second way of working.

import { cyclingOn, offerStack, setCycling, type Candidate } from './cycle.svelte';
import { cave, changed, focusStation, selectIds, selectedIds, setPick } from './cave.svelte';
import { engine, type Scope } from './engine';
import { type MenuEntry, shortcut } from './context-menu.svelte';
import { copySelection, deleteSelection, pasteFromMenu, startCopy } from './view-clip.svelte';
import { markViewUsed, selectAllInLastView } from './select-all';
import { showPanel } from './workspace.svelte';
import { isStampKind, symbolDef } from './symbols';
import { fillLabel, isFillKind } from './fills';
import { startTripAt } from './caves';

/** The line types whose marks sit on one side, so Flip side means something (as in the ribbon's line tab). */
const SHAPED = ['ledge', 'pit', 'flowstone', 'rimstone', 'slope', 'dripline', 'glyphs'];

type View = 'map' | 'profile' | 'section';

let labels: Record<string, string> | null = null;
/** What a type is called, as the ribbon names it. */
export function kindName(kind: string): string {
  labels ??= Object.fromEntries(engine.drawTools().map((t) => [t.key, t.label]));
  return labels[kind] ?? symbolDef(kind)?.name ?? fillLabel(kind) ?? kind;
}

/** Runs an edit on each selected line, then says what went wrong, if anything. */
function each(f: (id: string) => string) {
  let problem = '';
  for (const id of selectedIds()) {
    const r = f(id);
    if (r !== 'ok') problem = r.replace(/^error: /, '');
  }
  if (problem) engine.log('warn', 'edit', problem);
  changed();
}

/**
 * The menu on a drawn line (already selected by the view). `similar` finds the lines of its type in the view;
 * `editWords`, when given, edits the words of a symbol, text or leader in place.
 */
export function lineItems(view: View, similar: () => string[], editWords: (() => void) | null): MenuEntry[] {
  const ids = selectedIds();
  const infos = ids.map((id) => engine.lineInfo(id)).filter((i): i is NonNullable<typeof i> => !!i);
  if (!infos.length) return [];
  const kind = infos[0].kind;
  const alike = infos.every((i) => i.kind === kind);
  const stamps = infos.every((i) => isStampKind(i.kind));
  const lines = !stamps && !infos.some((i) => isFillKind(i.kind));
  const closed = infos.every((i) => i.closed);
  const flipped = infos.every((i) => i.flip);
  markViewUsed(view);
  return [
    { heading: ids.length > 1 ? `${ids.length} selected` : kindName(kind) },
    ...(editWords && ids.length === 1 ? [{ label: 'Edit text', run: editWords }] : []),
    { label: 'Cut', keys: shortcut('X'), run: () => void copySelection(true) },
    { label: 'Copy', keys: shortcut('C'), run: () => void copySelection() },
    { label: 'Copy and place…', run: startCopy },
    'separator',
    ...(lines ? [{ label: closed ? 'Open the curve' : 'Close the curve', run: () => each((id) => engine.setLineClosed(id, !closed)) }] : []),
    ...(alike && SHAPED.includes(kind) ? [{ label: 'Flip side', run: () => each((id) => engine.setLineFlip(id, !flipped)) }] : []),
    // always named Select similar, so it is found where people look for it; what it selects is the selection's type
    { label: 'Select similar', run: () => selectIds(similar()) },
    'separator',
    { label: 'Delete', keys: 'Del', danger: true, run: deleteSelection },
  ];
}

/** The menu on a station (Map): look at it, cut a cross section there, or survey on from it. */
export function stationItems(id: string, label: string): MenuEntry[] {
  return [
    { heading: `Station ${label}` },
    { label: 'Station details', run: () => setPick('station', id) },
    { label: 'Show on the map', run: () => (showPanel('map'), focusStation(id)) },
    { label: 'Cross section here', run: () => ((cave.station = id), showPanel('section')) },
    'separator',
    {
      label: `Start a new trip from ${label}`,
      run: () => {
        const problem = startTripAt(label);
        if (problem) engine.log('error', 'notebook', problem);
      },
    },
  ];
}

/** The menu on empty space in a view: undo and redo of that view, paste, select all, and bringing the drawing back into sight. */
export function viewItems(view: View, fit: () => void): MenuEntry[] {
  const scope: Scope = view;
  markViewUsed(view);
  return [
    { label: 'Undo', keys: shortcut('Z'), run: () => engine.undo(scope) && changed() },
    { label: 'Redo', keys: shortcut('Z', true), run: () => engine.redo(scope) && changed() },
    'separator',
    { label: 'Paste', keys: shortcut('V'), run: () => void pasteFromMenu() },
    { label: 'Select all', keys: shortcut('A'), run: () => void selectAllInLastView() },
    ...(selectedIds().length ? [{ label: 'Select none', run: () => selectIds([]) }] : []),
    'separator',
    { label: view === 'map' ? 'Show the whole cave' : 'Fit to the panel', run: fit },
    'separator',
    { label: `${cyclingOn() ? '✓ ' : ''}List stacked things on a click`, run: () => setCycling(!cyclingOn()) },
  ];
}

/** A drawn line as a row of the selection cycling list: its type, and its words if it has any. */
export function lineCandidate(id: string, kind: string, text: string | undefined, choose: (final: boolean) => void): Candidate {
  const words = (text ?? '').replace(/\s+/g, ' ').trim();
  return { key: `line:${id}`, label: kindName(kind), detail: words ? `“${words.length > 40 ? words.slice(0, 39) + '…' : words}”` : undefined, choose };
}

/**
 * After a click in a canvas view (Profile, Cross Section, the sketchpad): the drawn lines that were under it (`ids`, topmost
 * first, the first already selected by the press) and anything else there (`extra`, e.g. a station) go to the selection
 * cycling list. `lines` is what the view draws, to name each one.
 */
export function offerLineStack(x: number, y: number, ids: string[], lines: { id: string; kind?: string; text?: string }[], extra: Candidate[] = []) {
  const rows = ids.map((id) => {
    const k = lines.find((l) => l.id === id);
    return lineCandidate(id, k?.kind ?? '', k?.text, () => (setPick('', ''), selectIds([id])));
  });
  offerStack(x, y, [...rows, ...extra]);
}
