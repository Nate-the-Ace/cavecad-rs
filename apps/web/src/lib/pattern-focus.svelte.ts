// Opening the Patterns panel on the pattern a selected line or area uses, for editing it in detail. The ribbon and the
// right-click menu only offer the button; the panel listens for `patternFocus` and turns to it.
import { engine } from './engine';
import { isFillKind } from './fills';
import { patternName, parseStyle } from './line-style-text';
import type { PatternKind } from './patterns';
import { isStampKind } from './symbols';
import { showPanel } from './workspace.svelte';

/** The pattern the panel should turn to: bumped `seq` means "now". `name` '' opens the tab with nothing picked. */
export const patternFocus = $state({ kind: 'line' as PatternKind, name: '', seq: 0 });

/** A line or area type's look in the Patterns panel's starting points, to copy and change when it has no pattern of its own. */
const STARTING_POINT: Record<string, string> = {
  presumed: 'Presumed wall',
  narrow: 'Dashed',
  underlying: 'Underlying passage',
  joint: 'Joint',
  ledge: 'Ledge',
  pit: 'Pit edge',
  flowline: 'Flowline',
  'fill-sand': 'Sand',
  'fill-pebbles': 'Pebbles',
  'fill-clay': 'Clay',
  'fill-water': 'Water',
};

/** What the Patterns panel would open for this line (by id): its own pattern, or the matching starting point; null for symbols and words. */
export function patternFor(id: string): { kind: PatternKind; name: string; own: boolean } | null {
  const info = engine.lineInfo(id);
  if (!info) return null;
  const fill = isFillKind(info.kind);
  const own = patternName(parseStyle(info.style ?? '').lt);
  if (own) return { kind: fill ? 'fill' : 'line', name: own, own: true };
  if (info.kind === 'text' || info.kind === 'callout' || isStampKind(info.kind)) return null;
  return { kind: fill ? 'fill' : 'line', name: STARTING_POINT[info.kind] ?? '', own: false };
}

/** Opens the Patterns panel on what `patternFor` says. */
export function openPatternFor(id: string) {
  const p = patternFor(id);
  if (!p) return;
  showPanel('patterns');
  patternFocus.kind = p.kind;
  patternFocus.name = p.name;
  patternFocus.seq += 1;
}
