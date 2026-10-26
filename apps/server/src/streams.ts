// Live listeners: for each cave, the open streams waiting for new entries. In memory only; a restart drops them
// and devices reconnect, asking for what they missed by number.
import type { ServerResponse } from 'node:http';

const byCave = new Map<string, Set<ServerResponse>>();

/** Starts listening for a cave's new entries on an open stream; returns the function that stops it. */
export function listen(caveId: string, res: ServerResponse): () => void {
  let set = byCave.get(caveId);
  if (!set) byCave.set(caveId, (set = new Set()));
  set.add(res);
  return () => {
    set!.delete(res);
    if (set!.size === 0) byCave.delete(caveId);
  };
}

/** Sends one JSON line to every open stream of a cave. */
export function broadcast(caveId: string, line: object): void {
  const text = JSON.stringify(line) + '\n';
  for (const res of byCave.get(caveId) ?? []) res.write(text);
}

/** How many streams are open for a cave (for tests). */
export function listeners(caveId: string): number {
  return byCave.get(caveId)?.size ?? 0;
}
