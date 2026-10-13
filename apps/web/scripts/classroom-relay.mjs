// The classroom side of the room relay. A room started as a class has one teacher (the owner) and any
// number of students who only watch: their edits are refused here, they never get to seed the cave, and
// what they send is limited to a pointer (seen by the teacher), a raised hand and a line of chat. The
// teacher's view (which panels, where the cameras are, what is picked) is passed on as one small state
// message that a latecomer is given first. Chat and hand events are logged: kept in memory for the
// session, optionally appended to a file, and the teacher can fetch the whole transcript.
import { appendFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

const MAX_CHAT = 5000;
const MAX_TEXT = 500;
const MAX_MIRROR = 400_000;
/** A student may say one thing this often (ms). */
const SAY_EVERY = 700;
const BACKLOG = 300;

/** The state of one class. Plain data, so the tests can look at it. */
export function newClass(name, logDir) {
  const started = Date.now();
  let file = '';
  if (logDir) {
    const stamp = new Date(started).toISOString().replace(/[:.]/g, '-').slice(0, 19);
    file = join(logDir, `${String(name).replace(/[^A-Za-z0-9._-]/g, '_').slice(0, 60) || 'class'}-${stamp}.jsonl`);
  }
  return { name, started, ended: false, seq: 0, chat: [], hands: new Map(), mirror: '', lastSay: new Map(), file, logDir };
}

/** Appends one entry to the transcript (and the log file, if the server keeps one). Returns it. */
export function record(cls, entry) {
  const e = { n: ++cls.seq, at: Date.now(), ...entry };
  cls.chat.push(e);
  if (cls.chat.length > MAX_CHAT) cls.chat.shift();
  if (cls.file) {
    try {
      mkdirSync(cls.logDir, { recursive: true });
      appendFileSync(cls.file, `${JSON.stringify(e)}\n`);
    } catch {
      /* the transcript is still kept in memory and sent to the teacher */
    }
  }
  return e;
}

/** Raised hands, longest waiting first. */
export const handsOf = (cls) => [...cls.hands].map(([id, h]) => ({ id, name: h.name, since: h.since })).sort((a, b) => a.since - b.since);

/** What a person who joins a class is given with the welcome. */
export function classWelcome(cls, role) {
  return { role, name: cls.name, started: cls.started, ended: cls.ended, chat: cls.chat.slice(-BACKLOG), hands: handsOf(cls), mirror: cls.mirror };
}

/**
 * Handles a message that only means something in a class. Returns true when it was one (handled, or
 * refused). `ws` is the sender (`ws.person`, `ws.role`), `room` the relay's room, `send` and `toAll`
 * how to answer one person and everyone.
 */
export function classMessage(room, ws, msg, { send, toAll }) {
  const cls = room.class;
  if (!cls) return false;
  const teacher = ws.role === 'teacher';
  const who = ws.person;
  if (msg.t === 'say') {
    if (cls.ended) return send(ws, { t: 'denied', what: 'chat', message: 'The class has ended.' }), true;
    const text = String(msg.text ?? '').replace(/\s+/g, ' ').trim().slice(0, MAX_TEXT);
    if (!text) return true;
    const last = cls.lastSay.get(who.id) ?? 0;
    if (!teacher && Date.now() - last < SAY_EVERY) return send(ws, { t: 'denied', what: 'chat', message: 'Slow down a little.' }), true;
    cls.lastSay.set(who.id, Date.now());
    toAll({ t: 'chat', entry: record(cls, { kind: 'say', id: who.id, name: who.name, role: ws.role, text }) });
    return true;
  }
  if (msg.t === 'hand') {
    if (cls.ended) return true;
    // A student raises or lowers their own hand; the teacher can lower anyone's (calling on them).
    const target = teacher && typeof msg.who === 'string' ? msg.who : who.id;
    const person = [...room.peers].find((p) => p.person?.id === target)?.person;
    if (!person || (!teacher && target !== who.id)) return true;
    if (msg.up === true && !cls.hands.has(target)) {
      cls.hands.set(target, { name: person.name, since: Date.now() });
      toAll({ t: 'chat', entry: record(cls, { kind: 'hand', id: target, name: person.name, text: 'raised a hand' }) });
    } else if (msg.up !== true && cls.hands.delete(target)) {
      const called = teacher && target !== who.id;
      toAll({ t: 'chat', entry: record(cls, { kind: 'hand', id: target, name: person.name, text: called ? 'was called on' : 'lowered their hand' }) });
    } else return true;
    toAll({ t: 'hands', hands: handsOf(cls) });
    return true;
  }
  if (msg.t === 'mirror') {
    if (!teacher) return send(ws, { t: 'denied', what: 'view', message: 'Only the teacher controls the view.' }), true;
    const state = JSON.stringify(msg.state ?? null);
    if (state.length > MAX_MIRROR) return true;
    cls.mirror = state;
    for (const p of room.peers) if (p !== ws) send(p, { t: 'mirror', state: msg.state });
    return true;
  }
  if (msg.t === 'end' && teacher) {
    cls.ended = true;
    cls.hands.clear();
    toAll({ t: 'chat', entry: record(cls, { kind: 'system', text: 'The class has ended.' }) });
    toAll({ t: 'ended' });
    toAll({ t: 'hands', hands: [] });
    return true;
  }
  if (msg.t === 'getlog' && teacher) {
    send(ws, { t: 'log', name: cls.name, started: cls.started, entries: cls.chat });
    return true;
  }
  return false;
}

/** A person left a class room: their hand goes down. */
export function classLeft(room, ws, { toAll }) {
  const cls = room.class;
  if (!cls || !ws.person || !cls.hands.delete(ws.person.id)) return;
  toAll({ t: 'hands', hands: handsOf(cls) });
}
