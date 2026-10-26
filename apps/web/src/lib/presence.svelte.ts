// Other people's pointers in the views. Kept for room presence (rooms step 4): the views already draw what is
// here (`presence-draw.ts`) and report this person's pointer; nothing sends or receives pointers until a room
// connection plugs in with `setPresenceSender` and fills `presence`.

/** Someone in the cave with this person. */
export interface Person {
  id: string;
  name: string;
  color: string;
}

/** What someone else is doing: where their pointer is (in a view's own metres) and with which tool. */
export interface Cursor {
  id: string;
  /** `map`, `profile` or `section`. */
  view: string;
  /** Which run (profile) or station (section) they are looking at; empty for the map. */
  ctx: string;
  x: number;
  y: number;
  /** The drawing tool they have picked ('' for none). */
  tool: string;
  /** When we last heard from them (ms). */
  seen: number;
}

/** A pointer not heard from for this long is not shown. */
export const CURSOR_FADE_MS = 6000;
const PRESENCE_MS = 70;

export const presence = $state({
  /** Who else is here. */
  people: [] as Person[],
  /** The others' pointers, by id. */
  cursors: {} as Record<string, Cursor>,
});

let sender: ((msg: { view: string; ctx: string; x: number; y: number; tool: string }) => void) | null = null;
let lastSent = 0;

/** A room connection plugs in here to send this person's pointer; null unplugs it. */
export function setPresenceSender(send: typeof sender) {
  sender = send;
}

/** Tells the others where this person's pointer is. Cheap to call on every move: it is rate limited. */
export function reportCursor(view: string, ctx: string, x: number, y: number, tool: string) {
  if (!sender) return;
  const now = performance.now();
  if (now - lastSent < PRESENCE_MS) return;
  lastSent = now;
  sender({ view, ctx, x, y, tool });
}

/** The others' pointers that are in this view (and this run or station) and recent. */
export function cursorsFor(view: string, ctx: string): (Cursor & { person: Person | undefined })[] {
  const now = Date.now();
  return Object.values(presence.cursors)
    .filter((c) => c.view === view && c.ctx === ctx && now - c.seen < CURSOR_FADE_MS)
    .map((c) => ({ ...c, person: presence.people.find((p) => p.id === c.id) }));
}
