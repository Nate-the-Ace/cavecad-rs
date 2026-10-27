// Reading a latitude/longitude a person typed: decimal degrees ("36.99, -86.44") or degrees, minutes and
// seconds with a hemisphere letter before or after each half (`36°59'24"N 86°26'24"W`, "N 36 59.4 W 86 26.4").
// Minute and second marks may be straight, curly (’ ” as a phone or word processor types them) or primes (′ ″).
// Pure, so it can be checked without a browser (tests/latlon.check.ts). Never log what it reads.

type LatLon = { lat: number; lon: number };

const inRange = (p: LatLon): LatLon | null => (Number.isFinite(p.lat) && Number.isFinite(p.lon) && Math.abs(p.lat) <= 90 && Math.abs(p.lon) <= 180 ? p : null);

/** One half of a hemisphere-lettered coordinate as signed degrees, or NaN. */
function degrees(nums: number[], letter: string): number {
  if (nums.length < 1 || nums.length > 3) return NaN;
  const [d, m = 0, s = 0] = nums;
  if (m >= 60 || s >= 60) return NaN;
  // only the last number may carry a fraction
  if (nums.slice(0, -1).some((n) => !Number.isInteger(n))) return NaN;
  const v = d + m / 60 + s / 3600;
  return letter === 'S' || letter === 'W' ? -v : v;
}

/** A typed latitude/longitude, or null when it cannot be read or is out of range. */
export function parseLatLon(text: string): LatLon | null {
  const t = text.trim().toUpperCase().replace(/[°º'′‘’"″“”,;:]/g, ' ').replace(/\s+/g, ' ').trim();
  if (!t) return null;
  if (!/[NSEW]/.test(t)) {
    const dec = t.match(/^(-?\d+(?:\.\d+)?) (-?\d+(?:\.\d+)?)$/);
    return dec ? inRange({ lat: +dec[1], lon: +dec[2] }) : null;
  }
  if (/[^0-9. NSEW]/.test(t)) return null;
  const tokens = t.match(/\d+(?:\.\d+)?|[NSEW]/g) ?? [];
  if (tokens.join('').length !== t.replace(/ /g, '').length) return null; // stray dots
  const leading = /[NSEW]/.test(tokens[0] ?? '');
  const groups: { nums: number[]; letter: string }[] = [];
  let cur: { nums: number[]; letter: string } = { nums: [], letter: '' };
  for (const k of tokens) {
    if (/[NSEW]/.test(k)) {
      if (leading) {
        if (cur.letter || cur.nums.length) groups.push(cur);
        cur = { nums: [], letter: k };
      } else {
        if (!cur.nums.length) return null;
        cur.letter = k;
        groups.push(cur);
        cur = { nums: [], letter: '' };
      }
    } else cur.nums.push(+k);
  }
  if (leading) groups.push(cur);
  else if (cur.nums.length) return null; // numbers after the last letter
  if (groups.length !== 2 || groups.some((g) => !g.letter)) return null;
  const a = groups.find((g) => g.letter === 'N' || g.letter === 'S');
  const o = groups.find((g) => g.letter === 'E' || g.letter === 'W');
  if (!a || !o) return null;
  return inRange({ lat: degrees(a.nums, a.letter), lon: degrees(o.nums, o.letter) });
}
