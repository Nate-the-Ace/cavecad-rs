// The room relay for live editing: a dumb forwarder. Devices join a named room; the relay keeps
// every op the room has seen (in memory, for as long as the server runs) and passes each device's
// new ops to the others. It never reads an op: all merging happens on the devices, which already
// converge in any order. Nothing is stored on disk and nothing leaves the machine the server runs on.
import { randomBytes } from 'node:crypto';
import { WebSocketServer } from 'ws';

const MAX_ROOM = 64;
const MAX_MESSAGE = 64 * 1024 * 1024;
const MAX_INVITES = 50;
const MAX_HOURS = 24 * 30;
/** A random secret as text (an invite, or the key that proves a device is the room's owner). */
const secret = () => randomBytes(16).toString('hex');

/** Room state is plain data so the tests can look at it. */
export function createRelay() {
  /**
   * A room also has an owner (the first device in; it holds `ownerKey` and can reclaim the room with it),
   * invites it has made (secret tokens with an expiry and a number of uses), whether it is `locked` to
   * people with an invite, and who was admitted already (so a reconnect does not use up an invite).
   * @type {Map<string, { log: unknown[]; peers: Set<import('ws').WebSocket>; seeder: import('ws').WebSocket | null; ownerKey: string; locked: boolean; invites: Map<string, { label: string; expires: number; uses: number; used: number }>; admitted: Set<string> }>}
   */
  const rooms = new Map();

  const send = (ws, msg) => ws.readyState === ws.OPEN && ws.send(JSON.stringify(msg));
  const announce = (room) => {
    const people = [...room.peers].map((p) => p.person);
    const owner = [...room.peers].find((p) => p.isOwner)?.person.id ?? '';
    room.peers.forEach((p) => send(p, { t: 'peers', n: room.peers.size, people, owner, locked: room.locked }));
  };
  const invitesOf = (room) => [...room.invites].map(([token, i]) => ({ token, label: i.label, expires: i.expires, uses: i.uses, used: i.used }));
  const liveInvite = (room, token) => {
    const i = room.invites.get(token);
    if (!i) return null;
    if (i.expires <= Date.now()) return room.invites.delete(token), null;
    return i;
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
        const fresh = !rooms.has(name);
        if (fresh) rooms.set(name, { log: [], peers: new Set(), seeder: null, ownerKey: secret(), locked: false, invites: new Map(), admitted: new Set() });
        room = rooms.get(name);
        // Who may come in: the owner (by its key), anyone while the room is open, else only with a live invite.
        const id = clean(msg.id, 40);
        const isOwner = fresh || (typeof msg.ownerKey === 'string' && msg.ownerKey === room.ownerKey);
        let invite = null;
        if (!isOwner && room.locked && !room.admitted.has(id)) {
          invite = liveInvite(room, String(msg.invite ?? ''));
          if (!invite || (invite.uses > 0 && invite.used >= invite.uses)) {
            send(ws, { t: 'error', message: msg.invite ? 'That invite has expired or was already used. Ask the owner of the room for a new one.' : 'This room is invite only. Ask its owner for an invite link.' });
            return void (room = null);
          }
        }
        if (room.peers.size >= MAX_ROOM) return send(ws, { t: 'error', message: 'That room is full.' }), (room = null);
        if (invite) invite.used += 1;
        if (id) room.admitted.add(id);
        ws.isOwner = isOwner;
        // The first device into an empty room supplies the cave; everyone else receives it.
        const seed = room.log.length === 0 && room.seeder === null;
        if (seed) room.seeder = ws;
        // Who this is, shown as an avatar to the others. Taken as text only.
        ws.person = { id: clean(msg.id, 40), name: clean(msg.name, 40) || 'Guest', color: /^#[0-9a-f]{6}$/i.test(msg.color) ? msg.color : '#888888' };
        room.peers.add(ws);
        send(ws, { t: 'welcome', log: room.log, seed, owner: isOwner, ...(fresh ? { ownerKey: room.ownerKey } : {}), ...(isOwner ? { invites: invitesOf(room) } : {}) });
        announce(room);
      } else if (msg.t === 'invite' && room && ws.isOwner) {
        // The owner makes an invite: a secret link token that works for a while and for some people.
        if (room.invites.size >= MAX_INVITES) return send(ws, { t: 'error', message: 'Too many invites: remove some first.' });
        const hours = Math.min(MAX_HOURS, Math.max(1, Number(msg.hours) || 24));
        const uses = Math.min(100, Math.max(0, Math.floor(Number(msg.uses) || 0))); // 0: anyone with the link
        const token = secret();
        room.invites.set(token, { label: clean(msg.label, 60), expires: Date.now() + hours * 3600_000, uses, used: 0 });
        room.locked = true; // having made an invite, the room is for people who hold one
        send(ws, { t: 'invites', invites: invitesOf(room), made: token });
        announce(room);
      } else if (msg.t === 'revoke' && room && ws.isOwner) {
        room.invites.delete(String(msg.token ?? ''));
        send(ws, { t: 'invites', invites: invitesOf(room) });
      } else if (msg.t === 'lock' && room && ws.isOwner) {
        room.locked = !!msg.on;
        send(ws, { t: 'invites', invites: invitesOf(room) });
        announce(room);
      } else if (msg.t === 'presence' && room && ws.person) {
        // Where a person is and what they are doing: passed on to the others, never stored, and
        // stamped with the sender's id here so nobody can speak for someone else.
        const { t: _t, ...rest } = msg;
        const note = JSON.stringify({ ...rest, t: 'presence', id: ws.person.id });
        if (note.length <= 2000) for (const p of room.peers) if (p !== ws && p.readyState === p.OPEN) p.send(note);
      } else if (msg.t === 'ops' && room && Array.isArray(msg.ops) && msg.ops.length) {
        room.log.push(...msg.ops);
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
