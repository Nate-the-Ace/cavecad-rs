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

const NUM = String.raw`-?\d+(?:\.\d*)?|-?\.\d+`;
const SINGLE = new RegExp(String.raw`^(${NUM})\s*(mm|cm|m|ft|feet|foot|in|inch|inches|'|")?$`);
const FEET_INCHES = new RegExp(String.raw`^(${NUM})\s*(?:ft|feet|foot|')\s*(${NUM})\s*(?:in|inch|inches|")?$`);

/**
 * Reads what a person typed into a length field, as metres. A bare number is in the display
 * unit (feet when imperial). An explicit unit always wins: "30 cm", "12 ft", `5'6"`, "5 ft 6".
 * Returns `null` for anything that is not a length.
 */
export function parseLength(text: string, u: Units): number | null {
  const t = text.trim().toLowerCase().replace(/\s+/g, ' ');
  if (!t) return null;
  const fi = FEET_INCHES.exec(t);
  if (fi) {
    const feet = parseFloat(fi[1]);
    const inches = parseFloat(fi[2]);
    const sign = feet < 0 || fi[1].startsWith('-') ? -1 : 1;
    return sign * (Math.abs(feet) * FOOT + inches * (FOOT / 12));
  }
  const m = SINGLE.exec(t);
  if (!m) return null;
  const n = parseFloat(m[1]);
  if (!Number.isFinite(n)) return null;
  switch (m[2]) {
    case 'mm': return n / 1000;
    case 'cm': return n / 100;
    case 'm': return n;
    case 'ft': case 'feet': case 'foot': case "'": return n * FOOT;
    case 'in': case 'inch': case 'inches': case '"': return n * (FOOT / 12);
    default: return n * metresPer(u);
  }
}

/** A length in metres as the text a field shows: just the number, in the display unit. */
export function lengthText(m: number, u: Units): string {
  return `${+inUnits(m, u).toFixed(u === 'imperial' ? 2 : 3)}`;
}
