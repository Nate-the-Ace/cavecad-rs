// What a running command offers to set. A command that is running (a drawing tool today; more
// later) gets a contextual tab in the ribbon listing its options, defined here as data so the
// ribbon can show any command's settings the same way and every consumer reads the same values.

export type OptionDef =
  | { id: string; label: string; hint: string; kind: 'range'; min: number; max: number; step: number; default: number; show: (v: number) => string }
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
    id: 'pressure',
    label: 'Pen pressure',
    hint: 'Pressing harder draws a thicker line. It changes how the line looks, never where it is.',
    kind: 'toggle',
    default: true,
  },
];

/** The options of a running command (`wall`, `ledge`, ...), or none. */
export function optionsFor(command: string): OptionDef[] {
  return command ? DRAW_OPTIONS : [];
}
