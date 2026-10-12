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

export const live = $state({
  /** `off`, `connecting` or `live`. */
  status: 'off' as 'off' | 'connecting' | 'live',
  room: '',
  /** Who is in the room, counting this device. */
  people: [] as Person[],
  /** This device's id in the room. */
  me: myId,
  /** Why the last attempt failed, in plain words. */
  error: '',
});

const FLUSH_MS = 80;
let socket: WebSocket | null = null;
let timer: ReturnType<typeof setInterval> | undefined;

function flush() {
  if (!socket || socket.readyState !== WebSocket.OPEN || live.status !== 'live') return;
  const ops = engine.syncOutbox();
  if (ops !== '[]') socket.send(`{"t":"ops","ops":${ops}}`);
}

/** Leaves the room. The cave stays open on this device exactly as it is. */
export function leave() {
  clearInterval(timer);
  const s = socket;
  socket = null;
  live.status = 'off';
  live.people = [];
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
  live.room = name;
  live.error = '';
  live.status = 'connecting';
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  let ws: WebSocket;
  try {
    ws = new WebSocket(`${proto}//${location.host}/sync`);
  } catch {
    live.status = 'off';
    live.error = 'Could not reach the sync server.';
    return;
  }
  socket = ws;
  ws.onopen = () => ws.send(JSON.stringify({ t: 'join', room: name, id: myId, name: me.name.trim() || deviceName(), color: me.color }));
  ws.onerror = () => {
    if (socket === ws && live.status === 'connecting') live.error = 'Could not reach the sync server. Live editing needs the dev server (npm run dev) on one of the devices.';
  };
  ws.onclose = () => {
    if (socket !== ws) return;
    if (live.status === 'live') live.error = 'The connection to the room was lost. Edits made now are not shared.';
    leave();
  };
  ws.onmessage = (ev) => {
    let m: { t: string; log?: unknown[]; seed?: boolean; ops?: unknown[]; people?: Person[]; message?: string };
    try {
      m = JSON.parse(String(ev.data));
    } catch {
      return;
    }
    if (m.t === 'welcome') {
      if (m.seed) {
        engine.syncOutbox(); // what loading did is in the snapshot already
        ws.send(`{"t":"ops","ops":${engine.syncSnapshot()}}`);
      } else {
        const r = engine.syncLoad(JSON.stringify(m.log ?? []));
        if (r !== 'ok') {
          live.error = r.replace(/^error: /, '');
          leave();
          return;
        }
        cave.fileName = `room ${name}`;
        cave.opened += 1; // the canvas re-reads and re-frames
      }
      live.status = 'live';
      changed();
      timer = setInterval(flush, FLUSH_MS);
    } else if (m.t === 'ops' && m.ops) {
      engine.syncApply(JSON.stringify(m.ops));
      changed();
    } else if (m.t === 'peers') {
      live.people = m.people ?? [];
    } else if (m.t === 'error') {
      live.error = m.message ?? 'The room refused this device.';
      leave();
    }
  };
}
