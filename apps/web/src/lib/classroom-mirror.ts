// What the teacher is looking at, as one small piece of data that students put themselves on. Each part
// of the interface that has a "where am I looking" (the ribbon's tab, the panels' arrangement, a view's
// camera, the trip and station on show) registers a reader and a writer under a name. The teacher's page
// reads them all every moment and sends what changed; a student's page hands each part its state.
//
// A part that is not on the page yet (a panel the teacher just opened, still being built here) is kept
// and given its state the moment it registers, so no order of arrival loses a view.

export interface Mirrored {
  /** The part's state now, as plain data (compared as JSON, so keep it small and stable). */
  read(): unknown;
  /** Puts this device's part where the teacher's is. */
  write(state: unknown): void;
}

const parts = new Map<string, Mirrored>();
/** The last state heard for each part, kept for parts that have not registered yet. */
const heard = new Map<string, unknown>();

/** A part says it exists. Gives back the function that says it is gone. */
export function registerMirror(key: string, part: Mirrored): () => void {
  parts.set(key, part);
  if (heard.has(key)) {
    const state = heard.get(key);
    queueMicrotask(() => parts.get(key) === part && safely(() => part.write(state)));
  }
  return () => {
    if (parts.get(key) === part) parts.delete(key);
  };
}

function safely(f: () => void) {
  try {
    f();
  } catch {
    /* a part that cannot take the state leaves the others alone */
  }
}

/** Everything the teacher is looking at, by part. */
export function captureMirror(): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, part] of parts) {
    try {
      const v = part.read();
      if (v !== undefined) out[key] = v;
    } catch {
      /* skip a part that cannot be read right now */
    }
  }
  return out;
}

/** Puts every part that is here where the teacher's is, and remembers the rest for when they arrive. */
export function applyMirror(state: unknown) {
  if (!state || typeof state !== 'object') return;
  for (const [key, value] of Object.entries(state as Record<string, unknown>)) {
    heard.set(key, value);
    const part = parts.get(key);
    if (part) safely(() => part.write(value));
  }
}

/** Forgets everything heard (when a class ends or is left). */
export function forgetMirror() {
  heard.clear();
}

/** A zoom or pan of a flat view as the world rectangle it shows, so a window of another size shows the same part. */
export interface Rect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** The scale and offset that fit `rect` (world, y up) into a window of `width` x `height` pixels, centred, and with the same
 *  convention the flat views use: screen x = ox + world x * scale, screen y = oy - world y * scale. */
export function fitRect(rect: Rect, width: number, height: number): { scale: number; ox: number; oy: number } {
  const w = Math.max(1e-9, rect.x1 - rect.x0);
  const h = Math.max(1e-9, rect.y1 - rect.y0);
  const scale = Math.min(width / w, height / h);
  const cx = (rect.x0 + rect.x1) / 2;
  const cy = (rect.y0 + rect.y1) / 2;
  return { scale, ox: width / 2 - cx * scale, oy: height / 2 + cy * scale };
}

/** The world rectangle a window of `width` x `height` shows at this scale and offset (the inverse of `fitRect`). */
export function visibleRect(scale: number, ox: number, oy: number, width: number, height: number): Rect {
  return { x0: (0 - ox) / scale, x1: (width - ox) / scale, y0: (oy - height) / scale, y1: oy / scale };
}

/** A rectangle rounded to centimetres, so a mirrored view does not change on every pixel of noise. */
export const roundRect = (r: Rect): Rect => ({ x0: rounded(r.x0, 2), y0: rounded(r.y0, 2), x1: rounded(r.x1, 2), y1: rounded(r.y1, 2) });

/** Rounds to a few digits, so a mirrored number does not change on every pixel of noise. */
export const rounded = (v: number, digits = 3): number => {
  const k = 10 ** digits;
  return Math.round(v * k) / k;
};

/** The arrangement of a dockview layout without the sizes and which tab is in front, so that dragging a
 *  splitter or switching tabs does not look like a new arrangement. */
export function layoutShape(layout: unknown): string {
  const strip = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(strip);
    if (v && typeof v === 'object') {
      const out: Record<string, unknown> = {};
      for (const [k, x] of Object.entries(v as Record<string, unknown>)) {
        if (k === 'size' || k === 'width' || k === 'height' || k === 'activeView' || k === 'activeGroup') continue;
        out[k] = strip(x);
      }
      return out;
    }
    return v;
  };
  return JSON.stringify(strip(layout));
}

/** The panel in front in each group of a dockview layout. */
export function frontPanels(layout: unknown): string[] {
  const out: string[] = [];
  const walk = (v: unknown) => {
    if (Array.isArray(v)) return v.forEach(walk);
    if (v && typeof v === 'object') {
      const o = v as Record<string, unknown>;
      if (typeof o.activeView === 'string') out.push(o.activeView);
      Object.values(o).forEach(walk);
    }
  };
  walk(layout);
  return out.sort();
}
