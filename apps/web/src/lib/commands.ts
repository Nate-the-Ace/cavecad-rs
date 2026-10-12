// What a running command offers to set. A command that is running (a drawing tool today; more
// later) gets a contextual tab in the ribbon listing its options, defined here as data so the
// ribbon can show any command's settings the same way and every consumer reads the same values.

import { formatLength, type Units } from './units.ts';

export type OptionDef =
  | { id: string; label: string; hint: string; kind: 'range'; min: number; max: number; step: number; default: number; show: (v: number, units: Units) => string }
  | { id: string; label: string; hint: string; kind: 'toggle'; default: boolean }
  | { id: string; label: string; hint: string; kind: 'text'; default: string; placeholder: string };

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

/** The options of a placing-a-symbol command: its label and how big a click makes it (a drag sets size and direction). Two rows only, to fit the ribbon; "Draw with finger" keeps the value set on a line tool. */
export const SYMBOL_OPTIONS: OptionDef[] = [
  {
    id: 'symbolSize',
    label: 'Size',
    hint: 'How big a symbol is when you just click. Drag instead to set its size and which way it points: the line you drag is its radius and its up.',
    kind: 'range',
    min: 0.25,
    max: 6,
    step: 0.25,
    default: 1.5,
    show: (v, units) => formatLength(v, units, 2),
  },
  {
    id: 'label',
    label: 'Label',
    hint: 'Words to put beside each symbol you place (a dip, a depth, a number). Leave empty for none. You can change a placed symbol\'s label by selecting it.',
    kind: 'text',
    default: '',
    placeholder: 'e.g. 30',
  },
];

/** The options of the Text tool: the words, and how big a click makes them (a drag sets size and direction). */
export const TEXT_OPTIONS: OptionDef[] = [
  {
    id: 'label',
    label: 'Text',
    hint: 'The words to place. Click to place them at the chosen size; drag to set their size and direction.',
    kind: 'text',
    default: '',
    placeholder: 'Type the text, then click in a view',
  },
  SYMBOL_OPTIONS[0],
];

/** The options of a running command (`wall`, `ledge`, ...), or none. */
export function optionsFor(command: string): OptionDef[] {
  if (!command) return [];
  if (command === 'text') return TEXT_OPTIONS;
  return command.startsWith('sym-') ? SYMBOL_OPTIONS : DRAW_OPTIONS;
}
