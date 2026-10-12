// Keeps each room's op log on disk so a room survives a server restart. One append-only JSON-lines
// file per room (named by a hash of the room name, so no room name can reach outside the folder).
// The relay never reads an op; neither does this: it stores and returns them as they came.
import { appendFile, mkdir } from 'node:fs/promises';
import { mkdirSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';

/** Opens (creating if needed) a store that keeps room logs in `dir`. */
export function createRoomStore(dir) {
  mkdirSync(dir, { recursive: true });
  /** @type {Map<string, Promise<void>>} writes for one room run in order */
  const queues = new Map();
  /** Rooms whose file ends in a torn line: their next write must start on a fresh line. */
  const torn = new Set();
  const fileFor = (room) => path.join(dir, `${createHash('sha256').update(room).digest('hex').slice(0, 32)}.jsonl`);

  return {
    /** The room's saved ops, oldest first. A half-written last line (a crash mid-write) is dropped. */
    load(room) {
      let text;
      try {
        text = readFileSync(fileFor(room), 'utf8');
      } catch {
        return [];
      }
      if (text && !text.endsWith('\n')) torn.add(room);
      const ops = [];
      for (const line of text.split('\n')) {
        if (!line) continue;
        try {
          const v = JSON.parse(line);
          if (Array.isArray(v.ops)) ops.push(...v.ops);
        } catch {
          // a torn line: everything before it is still good
        }
      }
      return ops;
    },
    /** Saves a batch of new ops. Returns when it is on disk. */
    append(room, ops) {
      const line = `${torn.delete(room) ? '\n' : ''}${JSON.stringify({ room, ops })}\n`;
      const prev = queues.get(room) ?? Promise.resolve();
      const next = prev
        .then(() => mkdir(dir, { recursive: true }))
        .then(() => appendFile(fileFor(room), line))
        .catch((e) => console.error(`[cave-server] could not save ops for a room: ${e.code ?? e.message}`));
      queues.set(room, next);
      return next;
    },
    /** Resolves once every write started so far has finished (used on shutdown and in tests). */
    async flush() {
      await Promise.all([...queues.values()]);
    },
  };
}
