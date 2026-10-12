// Display units. The cave is always stored and calculated in metres; units only change what
// people read (scale bar, axis labels, lengths). Exports are never affected.
export type Units = 'metric' | 'imperial';

/** Metres in one foot. */
export const FOOT = 0.3048;

/** Metres per display unit. */
export const metresPer = (u: Units): number => (u === 'imperial' ? FOOT : 1);

/** The short unit name. */
export const unitName = (u: Units): string => (u === 'imperial' ? 'ft' : 'm');

/** Length in metres as a number in the display unit. */
export const inUnits = (m: number, u: Units): number => m / metresPer(u);

/** A length for reading: "111 m" or "364 ft". */
export function formatLength(m: number, u: Units, digits = 0): string {
  return `${+inUnits(m, u).toFixed(digits)} ${unitName(u)}`;
}

/** A round number (1, 2 or 5 times a power of ten) at or below `target`. */
export function niceNumber(target: number): number {
  const p = Math.pow(10, Math.floor(Math.log10(target)));
  const m = target / p;
  return (m >= 5 ? 5 : m >= 2 ? 2 : 1) * p;
}
