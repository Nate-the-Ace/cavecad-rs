// Live editing with other devices. A device joins a named room on the relay (served by the dev
// server at /sync); the first one in supplies the cave, later ones receive it. After that every
// local edit (including undo and redo) is sent as ops and every op from the others is applied. The
// ops merge in any order, so nothing here decides who wins: that is the document's job.
import { engine } from './engine';
import { cave, changed } from './cave.svelte';

/** Someone in the room. */
export interface Person {
  id: string;
  name: string;
  color: string;
}

const NAME_KEY = 'cavecad.name';
const COLORS = ['#3b82f6', '#e5484d', '#2fb86a', '#f0a020', '#8e6cef', '#14b8c4', '#e0559a', '#7c8a3a'];
const deviceName = () => (/iPad|Android(?!.*Mobile)/.test(navigator.userAgent) || (navigator.maxTouchPoints > 1 && /Mac/.test(navigator.platform)) ? 'iPad' : /Mobi/.test(navigator.userAgent) ? 'Phone' : 'Laptop');
const savedName = () => {
  try {
    return localStorage.getItem(NAME_KEY) || deviceName();
  } catch {
    return deviceName();
  }
};
const myId = Math.random().toString(36).slice(2, 10);

/** Who this device is in a room: a name it can change, and a colour that follows its id. */
export const me = $state({ name: savedName(), color: COLORS[[...myId].reduce((n, c) => n + c.charCodeAt(0), 0) % COLORS.length] });

/** Renames this device (remembered in this browser; takes effect the next time it joins). */
export function rename(name: string) {
  me.name = name.slice(0, 40);
  try {
    localStorage.setItem(NAME_KEY, me.name);
  } catch {
    /* not remembered */
  }
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

export const live = $state({
  /** `off`, `connecting`, `live`, or `reconnecting` (the connection dropped; edits wait and are sent on return). */
  status: 'off' as 'off' | 'connecting' | 'live' | 'reconnecting',
  room: '',
  /** Who is in the room, counting this device. */
  people: [] as Person[],
  /** This device's id in the room. */
  me: myId,
  /** Why the last attempt failed, in plain words. */
  error: '',
  /** The others' pointers, by id. */
  cursors: {} as Record<string, Cursor>,
});

const FLUSH_MS = 80;
const PRESENCE_MS = 70;
/** A pointer not heard from for this long is not shown. */
export const CURSOR_FADE_MS = 6000;
const RETRY_MS = [1000, 2000, 4000, 8000, 15000];

let socket: WebSocket | null = null;
let timer: ReturnType<typeof setInterval> | undefined;
let retry: ReturnType<typeof setTimeout> | undefined;
let attempts = 0;
let wanted = false;
let roomName = '';
let lastPresence = 0;

function flush() {
  // pointers not heard from lately are dropped, so they vanish from the views
  const now = Date.now();
  for (const id of Object.keys(live.cursors)) if (now - live.cursors[id].seen > CURSOR_FADE_MS) delete live.cursors[id];
  if (!socket || socket.readyState !== WebSocket.OPEN || live.status !== 'live') return;
  const ops = engine.syncOutbox();
  if (ops !== '[]') socket.send(`{"t":"ops","ops":${ops}}`);
}

/** Tells the others where this person's pointer is. Cheap to call on every move: it is rate limited. */
export function reportCursor(view: string, ctx: string, x: number, y: number, tool: string) {
  if (live.status !== 'live' || !socket || socket.readyState !== WebSocket.OPEN) return;
  const now = performance.now();
  if (now - lastPresence < PRESENCE_MS) return;
  lastPresence = now;
  socket.send(JSON.stringify({ t: 'presence', view, ctx, x, y, tool }));
}

/** The others' pointers that are in this view (and this run or station) and recent. */
export function cursorsFor(view: string, ctx: string): (Cursor & { person: Person | undefined })[] {
  const now = Date.now();
  return Object.values(live.cursors)
    .filter((c) => c.view === view && c.ctx === ctx && now - c.seen < CURSOR_FADE_MS)
    .map((c) => ({ ...c, person: live.people.find((p) => p.id === c.id) }));
}

/** What a person is doing, in words, from their last pointer report. */
export function activityOf(id: string): string {
  const c = live.cursors[id];
  if (!c || Date.now() - c.seen > CURSOR_FADE_MS) return '';
  const where = c.view === 'map' ? 'the Map' : c.view === 'profile' ? 'the Profile' : 'a Cross Section';
  return c.tool ? `drawing ${c.tool.replace(/^sym-/, '').replace(/-/g, ' ')} in ${where}` : `looking at ${where}`;
}

function stopTimers() {
  clearInterval(timer);
  clearTimeout(retry);
}

/** Leaves the room. The cave stays open on this device exactly as it is. */
export function leave() {
  wanted = false;
  stopTimers();
  const s = socket;
  socket = null;
  live.status = 'off';
  live.people = [];
  live.cursors = {};
  if (s && s.readyState <= WebSocket.OPEN) s.close();
}

/** Joins (or starts) a room. */
export function join(room: string) {
  leave();
  const name = room.trim();
  if (!name) {
    live.error = 'Give the room a name first.';
    return;
  }
  roomName = name;
  live.room = name;
  live.error = '';
  live.status = 'connecting';
  wanted = true;
  attempts = 0;
  connect(false);
}

/**
 * Opens the connection. `rejoin` is true after a drop: then this device keeps its cave (it holds
 * edits the room has not seen), applies whatever the room has that it missed, and sends what it
 * made while away. Ops merge in any order, so nothing is lost or doubled.
 */
function connect(rejoin: boolean) {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  let ws: WebSocket;
  try {
    ws = new WebSocket(`${proto}//${location.host}/sync`);
  } catch {
    giveUp('Could not reach the sync server.');
    return;
  }
  socket = ws;
  ws.onopen = () => ws.send(JSON.stringify({ t: 'join', room: roomName, id: myId, name: me.name.trim() || deviceName(), color: me.color }));
  ws.onerror = () => {
    if (socket === ws && live.status === 'connecting') live.error = 'Could not reach the sync server. Live editing needs the dev server (npm run dev) on one of the devices.';
  };
  ws.onclose = () => {
    if (socket !== ws || !wanted) return;
    socket = null;
    stopTimers();
    if (live.status === 'connecting') {
      // never got in: say so rather than retrying forever
      giveUp(live.error || 'Could not reach the sync server.');
      return;
    }
    // it dropped: keep editing, keep trying
    live.status = 'reconnecting';
    live.error = '';
    live.cursors = {};
    const wait = RETRY_MS[Math.min(attempts, RETRY_MS.length - 1)];
    attempts++;
    retry = setTimeout(() => wanted && connect(true), wait);
  };
  ws.onmessage = (ev) => {
    let m: { t: string; log?: unknown[]; seed?: boolean; ops?: unknown[]; people?: Person[]; message?: string } & Partial<Cursor>;
    try {
      m = JSON.parse(String(ev.data));
    } catch {
      return;
    }
    if (m.t === 'welcome') {
      if (m.seed) {
        engine.syncOutbox(); // everything is in the snapshot already
        ws.send(`{"t":"ops","ops":${engine.syncSnapshot()}}`);
      } else if (rejoin) {
        engine.syncApply(JSON.stringify(m.log ?? [])); // what the room has that this device missed
      } else {
        const r = engine.syncLoad(JSON.stringify(m.log ?? []));
        if (r !== 'ok') {
          giveUp(r.replace(/^error: /, ''));
          return;
        }
        cave.fileName = `room ${roomName}`;
        cave.opened += 1; // the canvas re-reads and re-frames
      }
      live.status = 'live';
      live.error = '';
      attempts = 0;
      changed();
      clearInterval(timer);
      timer = setInterval(flush, FLUSH_MS);
      flush(); // what was made while away
    } else if (m.t === 'ops' && m.ops) {
      engine.syncApply(JSON.stringify(m.ops));
      changed();
    } else if (m.t === 'peers') {
      live.people = m.people ?? [];
      // pointers of people who left go with them
      const here = new Set(live.people.map((p) => p.id));
      for (const id of Object.keys(live.cursors)) if (!here.has(id)) delete live.cursors[id];
    } else if (m.t === 'presence' && m.id && m.id !== myId) {
      live.cursors[m.id] = { id: m.id, view: m.view ?? '', ctx: m.ctx ?? '', x: Number(m.x), y: Number(m.y), tool: m.tool ?? '', seen: Date.now() };
    } else if (m.t === 'error') {
      giveUp(m.message ?? 'The room refused this device.');
    }
  };
}

/** Stops trying and says why. */
function giveUp(why: string) {
  leave();
  live.error = why;
}

// Dev-only handle for tests and debugging in the browser console.
if (import.meta.env.DEV) (window as unknown as { __sync: typeof live }).__sync = live;
