// What one entry of a cave's history changed, in a few words, so a person can pick the point to go back to. An entry is the
// text of a list of ops (`[{stamp, action}]`, an action being `{Create|Remove|Set: {...}}`). Plain TypeScript, checked on its own.

const words = (name: string): string => name.replace(/([a-z0-9])([A-Z])/g, '$1 $2').toLowerCase();

/** A short description of an entry: what it created, removed and set (`set station walls left x2`). */
export function describeEntry(ops: string): string {
  let list: unknown;
  try {
    list = JSON.parse(ops);
  } catch {
    return 'an edit';
  }
  if (!Array.isArray(list)) return 'an edit';
  const made = new Map<string, number>();
  const bump = (key: string) => made.set(key, (made.get(key) ?? 0) + 1);
  for (const op of list) {
    const action = (op as { action?: Record<string, { kind?: unknown; field?: unknown }> } | null)?.action;
    if (!action || typeof action !== 'object') continue;
    const [verb] = Object.keys(action);
    const body = action[verb];
    if (verb === 'Set') bump(`set ${words(String(typeof body?.field === 'string' ? body.field : Object.keys((body?.field as object) ?? {})[0] ?? 'a field'))}`);
    else if (verb === 'Create' || verb === 'Remove') bump(`${verb === 'Create' ? 'added' : 'removed'} ${words(String(body?.kind ?? 'something'))}`);
  }
  if (!made.size) return 'an edit';
  return [...made].map(([k, n]) => (n > 1 ? `${k} x${n}` : k)).join(', ');
}
