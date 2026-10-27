// Live sessions, classes and join links were retired on 2026-10-26 (caves now live on the CaveCAD server and are
// shared through rooms). An old link still lands somewhere useful: these tell such a link apart, and what the
// address looks like once it is tidied away.

/** The query keys only the retired links used (`mb`/`mk` carried a class link's mailbox). */
const OLD_KEYS = ['live', 'class', 'join', 'invite', 'mb', 'mk'];

/** True for the query of an old live, class or join link (`?live=`, `?class=`, `?join=`). */
export function isOldLiveLink(search: string): boolean {
  const q = new URLSearchParams(search);
  return !!(q.get('live') || q.get('class') || q.get('join'));
}

/** True for a link that opened the desktop app to join a session (`cavecad://join?...`). */
export function isOldJoinUrl(url: string): boolean {
  try {
    const u = new URL(url);
    return u.protocol === 'cavecad:' && (u.hostname === 'join' || /^\/*join(\/|$)/.test(u.pathname) || isOldLiveLink(u.search));
  } catch {
    return false;
  }
}

/** The address without the old link's part (the path, any other query, and the hash are kept). */
export function withoutOldLink(pathname: string, search: string, hash: string): string {
  const q = new URLSearchParams(search);
  for (const k of OLD_KEYS) q.delete(k);
  const rest = q.toString();
  return `${pathname}${rest ? '?' + rest : ''}${hash}`;
}
