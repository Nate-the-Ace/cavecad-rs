// The room relay for live editing: a dumb forwarder. Devices join a named room; the relay keeps
// every op the room has seen (in memory, for as long as the server runs) and passes each device's
// new ops to the others. It never reads an op: all merging happens on the devices, which already
// converge in any order. Nothing is stored on disk and nothing leaves the machine the server runs on.
import { WebSocketServer } from 'ws';

const MAX_ROOM = 64;
const MAX_MESSAGE = 64 * 1024 * 1024;

/**
 * Room state is plain data so the tests can look at it.
 * `store` is optional and makes rooms survive a restart: `store.load(room)` returns the room's saved
 * ops (synchronously) the first time anyone joins it, and `store.append(room, ops)` is told about
 * every batch of new ops. Without a store the log lives only in memory.
 */
export function createRelay(store = null) {
  /** @type {Map<string, { log: unknown[]; peers: Set<import('ws').WebSocket>; seeder: import('ws').WebSocket | null }>} */
  const rooms = new Map();

  const send = (ws, msg) => ws.readyState === ws.OPEN && ws.send(JSON.stringify(msg));
  const announce = (room) => {
    const people = [...room.peers].map((p) => p.person);
    room.peers.forEach((p) => send(p, { t: 'peers', n: room.peers.size, people }));
  };
  const clean = (v, max) => String(v ?? '').slice(0, max);

  function onConnection(ws) {
    let room = null;
    let name = '';
    ws.on('message', (data) => {
      let msg;
      try {
        msg = JSON.parse(String(data));
      } catch {
        return;
      }
      if (msg.t === 'join' && !room) {
        name = String(msg.room ?? '').trim().slice(0, 60);
        if (!name) return send(ws, { t: 'error', message: 'A room needs a name.' });
        if (!rooms.has(name)) rooms.set(name, { log: store ? store.load(name) : [], peers: new Set(), seeder: null });
        room = rooms.get(name);
        if (room.peers.size >= MAX_ROOM) return send(ws, { t: 'error', message: 'That room is full.' }), (room = null);
        // The first device into an empty room supplies the cave; everyone else receives it.
        const seed = room.log.length === 0 && room.seeder === null;
        if (seed) room.seeder = ws;
        // Who this is, shown as an avatar to the others. Taken as text only.
        ws.person = { id: clean(msg.id, 40), name: clean(msg.name, 40) || 'Guest', color: /^#[0-9a-f]{6}$/i.test(msg.color) ? msg.color : '#888888' };
        room.peers.add(ws);
        send(ws, { t: 'welcome', log: room.log, seed });
        announce(room);
      } else if (msg.t === 'presence' && room && ws.person) {
        // Where a person is and what they are doing: passed on to the others, never stored, and
        // stamped with the sender's id here so nobody can speak for someone else.
        const { t: _t, ...rest } = msg;
        const note = JSON.stringify({ ...rest, t: 'presence', id: ws.person.id });
        if (note.length <= 2000) for (const p of room.peers) if (p !== ws && p.readyState === p.OPEN) p.send(note);
      } else if (msg.t === 'ops' && room && Array.isArray(msg.ops) && msg.ops.length) {
        room.log.push(...msg.ops);
        if (store) store.append(name, msg.ops);
        if (room.seeder === ws) room.seeder = null;
        for (const p of room.peers) if (p !== ws) send(p, { t: 'ops', ops: msg.ops });
      }
    });
    ws.on('close', () => {
      if (!room) return;
      room.peers.delete(ws);
      if (room.seeder === ws) room.seeder = null; // a seeder that left without sending hands the job on
      announce(room);
    });
  }

  return { rooms, onConnection };
}

/** Accepts upgrade requests for `/sync` on an http server and hands them to the relay. */
export function attachRelay(httpServer, relay = createRelay()) {
  const wss = new WebSocketServer({ noServer: true, maxPayload: MAX_MESSAGE });
  wss.on('connection', relay.onConnection);
  httpServer.on('upgrade', (req, socket, head) => {
    if (new URL(req.url ?? '', 'http://x').pathname !== '/sync') return; // leave Vite's own sockets alone
    wss.handleUpgrade(req, socket, head, (ws) => wss.emit('connection', ws, req));
  });
  return relay;
}
