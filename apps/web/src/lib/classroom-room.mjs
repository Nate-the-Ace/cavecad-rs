// The rules of a class, with no network in them: who may say what, the raised-hand queue, the record, and
// the teacher's view. One copy of these rules serves both ways a class can run, so they cannot drift apart:
// on the room relay (`scripts/classroom-relay.mjs`, a server) and on a peer-to-peer session
// (`p2p.svelte.ts`, the teacher's page is the hub). Each gives this module an `io` that says how to reach
// the people in the room.
//
//   io.members()     the people here now: [{ id, name, role }]
//   io.send(id, m)   a message to one person
//   io.all(m)        a message to everyone, the sender too
//   io.others(m)     a message to everyone but the sender
//
// A student may do three things: raise or lower their own hand, say a line, and move a pointer (which the
// carrier of the class sends to the teacher only). Everything else a student sends is refused.

const MAX_CHAT = 5000;
const MAX_TEXT = 500;
const MAX_MIRROR = 400_000;
/** A student may say one thing this often (ms). */
const SAY_EVERY = 700;
const BACKLOG = 300;

/**
 * A new class. `onRecord(entry)` is called with every line of the record as it is made (the relay appends
 * it to a file; a page has no file and leaves it out).
 * @param {string} name
 * @param {(entry: object) => void} [onRecord]
 */
export function newClass(name, onRecord) {
  return { name, started: Date.now(), ended: false, seq: 0, chat: [], hands: new Map(), mirror: '', lastSay: new Map(), onRecord: onRecord ?? null };
}

/** Appends one entry to the record. Returns it. */
export function record(cls, entry) {
  const e = { n: ++cls.seq, at: Date.now(), ...entry };
  cls.chat.push(e);
  if (cls.chat.length > MAX_CHAT) cls.chat.shift();
  try {
    cls.onRecord?.(e);
  } catch {
    /* the record is still kept in memory and can be fetched */
  }
  return e;
}

/** Raised hands, longest waiting first. */
export const handsOf = (cls) => [...cls.hands].map(([id, h]) => ({ id, name: h.name, since: h.since })).sort((a, b) => a.since - b.since);

/** What a person who joins is given with the welcome. */
export function classWelcome(cls, role) {
  return { role, name: cls.name, started: cls.started, ended: cls.ended, chat: cls.chat.slice(-BACKLOG), hands: handsOf(cls), mirror: cls.mirror };
}

/**
 * Handles a message that only means something in a class. Returns true when it was one (handled, or refused).
 * @param {ReturnType<typeof newClass>} cls
 * @param {{ id: string; name: string; role: string }} sender
 * @param {{ t: string } & Record<string, any>} msg
 */
export function classMessage(cls, sender, msg, io) {
  const teacher = sender.role === 'teacher';
  const refuse = (what, message) => (io.send(sender.id, { t: 'denied', what, message }), true);
  if (msg.t === 'say') {
    if (cls.ended) return refuse('chat', 'The class has ended.');
    const text = String(msg.text ?? '').replace(/\s+/g, ' ').trim().slice(0, MAX_TEXT);
    if (!text) return true;
    const last = cls.lastSay.get(sender.id) ?? 0;
    if (!teacher && Date.now() - last < SAY_EVERY) return refuse('chat', 'Slow down a little.');
    cls.lastSay.set(sender.id, Date.now());
    io.all({ t: 'chat', entry: record(cls, { kind: 'say', id: sender.id, name: sender.name, role: sender.role, text }) });
    return true;
  }
  if (msg.t === 'hand') {
    if (cls.ended) return true;
    // A student raises or lowers their own hand; the teacher can lower anyone's (calling on them).
    const target = teacher && typeof msg.who === 'string' ? msg.who : sender.id;
    const person = io.members().find((p) => p.id === target);
    if (!person || (!teacher && target !== sender.id)) return true;
    if (msg.up === true && !cls.hands.has(target)) {
      cls.hands.set(target, { name: person.name, since: Date.now() });
      io.all({ t: 'chat', entry: record(cls, { kind: 'hand', id: target, name: person.name, text: 'raised a hand' }) });
    } else if (msg.up !== true && cls.hands.delete(target)) {
      const called = teacher && target !== sender.id;
      io.all({ t: 'chat', entry: record(cls, { kind: 'hand', id: target, name: person.name, text: called ? 'was called on' : 'lowered their hand' }) });
    } else return true;
    io.all({ t: 'hands', hands: handsOf(cls) });
    return true;
  }
  if (msg.t === 'mirror') {
    if (!teacher) return refuse('view', 'Only the teacher controls the view.');
    const state = JSON.stringify(msg.state ?? null);
    if (state.length > MAX_MIRROR) return true;
    cls.mirror = state;
    io.others({ t: 'mirror', state: msg.state });
    return true;
  }
  if (msg.t === 'end' && teacher) {
    cls.ended = true;
    cls.hands.clear();
    io.all({ t: 'chat', entry: record(cls, { kind: 'system', text: 'The class has ended.' }) });
    io.all({ t: 'ended' });
    io.all({ t: 'hands', hands: [] });
    return true;
  }
  if (msg.t === 'getlog' && teacher) {
    io.send(sender.id, { t: 'log', name: cls.name, started: cls.started, entries: cls.chat });
    return true;
  }
  return false;
}

/** A person joined: the record says so, for everyone (and for the file the teacher keeps). */
export function classJoined(cls, person, io) {
  if (cls.ended) return;
  io.all({ t: 'chat', entry: record(cls, { kind: 'system', id: person.id, name: person.name, role: person.role, text: 'joined the class' }) });
}

/** A person left: their hand goes down, and the record says they went. `person` is who left (they are no longer in `io.members()`). */
export function classLeft(cls, id, io, person) {
  if (cls.hands.delete(id)) io.all({ t: 'hands', hands: handsOf(cls) });
  if (!cls.ended && person?.name) io.all({ t: 'chat', entry: record(cls, { kind: 'system', id, name: person.name, role: person.role, text: 'left the class' }) });
}

/** The messages that mean something only inside a class. */
export const CLASS_MESSAGES = ['say', 'hand', 'mirror', 'end', 'getlog'];
