// The classroom side of the room relay. A room started as a class has one teacher (the owner) and any
// number of students who only watch. The rules (who may say what, the hand queue, the record, the
// teacher's view) are `src/lib/classroom-room.mjs`, shared with peer-to-peer classes; this file only
// reaches the people in the relay's room, and appends the record to a file when the server keeps one.
import { appendFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { classLeft as roomLeft, classMessage as roomMessage, classWelcome, newClass as newRoomClass, CLASS_MESSAGES } from '../src/lib/classroom-room.mjs';

export { classWelcome, CLASS_MESSAGES };

/** A new class, its record appended to a file in `logDir` (when there is one) as it is made. */
export function newClass(name, logDir) {
  let file = '';
  if (logDir) {
    const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
    file = join(logDir, `${String(name).replace(/[^A-Za-z0-9._-]/g, '_').slice(0, 60) || 'class'}-${stamp}.jsonl`);
  }
  const cls = newRoomClass(name, (entry) => {
    if (!file) return;
    mkdirSync(logDir, { recursive: true });
    appendFileSync(file, `${JSON.stringify(entry)}\n`);
  });
  cls.file = file;
  return cls;
}

/** How the rules reach the people in a relay room. */
function ioFor(room, ws, send) {
  const person = (p) => ({ id: p.person.id, name: p.person.name, role: p.role });
  return {
    members: () => [...room.peers].filter((p) => p.person).map(person),
    send: (id, msg) => {
      const p = [...room.peers].find((q) => q.person?.id === id);
      if (p) send(p, msg);
    },
    all: (msg) => room.peers.forEach((p) => send(p, msg)),
    others: (msg) => room.peers.forEach((p) => p !== ws && send(p, msg)),
  };
}

/** Handles a message from `ws` that only means something in a class. Returns true when it was one. */
export function classMessage(room, ws, msg, { send }) {
  if (!room.class || !ws.person) return false;
  return roomMessage(room.class, { id: ws.person.id, name: ws.person.name, role: ws.role }, msg, ioFor(room, ws, send));
}

/** A person left a class room: their hand goes down. */
export function classLeft(room, ws, { send }) {
  if (room.class && ws.person) roomLeft(room.class, ws.person.id, ioFor(room, ws, send));
}
