// What a running command offers to set. A command that is running (a drawing tool today; more
// later) gets a contextual tab in the ribbon listing its options, defined here as data so the
// ribbon can show any command's settings the same way and every consumer reads the same values.

import { formatLength, type Units } from './units.ts';

export type OptionDef =
  | { id: string; label: string; hint: string; kind: 'range'; min: number; max: number; step: number; default: number; show: (v: number, units: Units) => string }
  | { id: string; label: string; hint: string; kind: 'toggle'; default: boolean };

/** How closely a drawn curve follows the hand: the tolerance, in screen pixels, a freehand stroke is reduced within. */
export function tolerancePx(detail: number): number {
  const d = Math.min(10, Math.max(1, detail));
  return 8 * Math.pow(0.075, (d - 1) / 9); // 1 -> 8 px (few points), 5 -> 2.7 px, 10 -> 0.6 px (follows the hand)
}

/** The options every drawing tool shares. */
export const DRAW_OPTIONS: OptionDef[] = [
  {
    id: 'detail',
    label: 'Curve detail',
    hint: 'How many fit points a drawn curve keeps. Low gives few points and a smooth, easy-to-edit curve; high follows your hand closely.',
    kind: 'range',
    min: 1,
    max: 10,
    step: 1,
    default: 5,
    show: (v) => (v <= 3 ? 'Few points' : v >= 8 ? 'Close' : 'Balanced'),
  },
  {
    id: 'close',
    label: 'Close curve',
    hint: 'Draw each curve as a closed loop: it runs from its last point back to its first, smoothly. A loop needs at least three points.',
    kind: 'toggle',
    default: false,
  },
  {
    id: 'finger',
    label: 'Draw with finger',
    hint: 'A single finger draws while a tool is running; two fingers pan and zoom. Turn off to draw with a pen only and pan with one finger.',
    kind: 'toggle',
    default: true,
  },
  {
    id: 'pressure',
    label: 'Pen pressure',
    hint: 'Pressing harder draws a thicker line. It changes how the line looks, never where it is.',
    kind: 'toggle',
    default: true,
  },
];

/** The options of a placing-a-symbol command: how big a click makes it (a drag sets size and direction). */
export const SYMBOL_OPTIONS: OptionDef[] = [
  {
    id: 'symbolSize',
    label: 'Symbol size',
    hint: 'How big a symbol is when you just click. Drag instead to set its size and which way it points: the line you drag is its radius and its up.',
    kind: 'range',
    min: 0.25,
    max: 6,
    step: 0.25,
    default: 1.5,
    show: (v, units) => formatLength(v, units, 2),
  },
  DRAW_OPTIONS.find((o) => o.id === 'finger')!,
];

/** The options of a running command (`wall`, `ledge`, ...), or none. */
export function optionsFor(command: string): OptionDef[] {
  if (!command) return [];
  return command.startsWith('sym-') ? SYMBOL_OPTIONS : DRAW_OPTIONS;
}
