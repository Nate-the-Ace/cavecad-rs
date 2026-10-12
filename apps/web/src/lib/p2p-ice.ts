// The relay (TURN) a device falls back on when two devices cannot reach each other directly. Devices
// try the direct way first; the relay only forwards the already-encrypted data when that fails, and
// cannot read it. The person brings their own relay (for example a Metered account): its details are
// kept in this browser only and are never put in a code or sent to the other device.

/** What the person typed. */
export interface Relay {
  /** An address that returns the relay's servers as JSON (Metered's "credentials API" link). */
  url: string;
  /** Or the servers by hand: addresses separated by commas, spaces or lines... */
  urls: string;
  /** ...with the username... */
  username: string;
  /** ...and password the provider gave. */
  credential: string;
}

export const emptyRelay = (): Relay => ({ url: '', urls: '', username: '', credential: '' });

const isServerUrl = (u: unknown): u is string => typeof u === 'string' && /^(stun|stuns|turn|turns):[^\s]+$/i.test(u.trim());

/** The servers in a provider's JSON answer: a list of `{urls, username, credential}`. Anything else is dropped. */
export function normalizeServers(json: unknown): RTCIceServer[] {
  const list = Array.isArray(json) ? json : Array.isArray((json as { iceServers?: unknown })?.iceServers) ? (json as { iceServers: unknown[] }).iceServers : [];
  const out: RTCIceServer[] = [];
  for (const item of list) {
    const s = item as { urls?: unknown; username?: unknown; credential?: unknown };
    const urls = (Array.isArray(s?.urls) ? s.urls : [s?.urls]).filter(isServerUrl).map((u) => u.trim());
    if (!urls.length) continue;
    const turn = urls.some((u) => /^turns?:/i.test(u));
    if (turn && (typeof s.username !== 'string' || typeof s.credential !== 'string')) continue; // a relay with no login is no use
    out.push(turn ? { urls, username: s.username as string, credential: s.credential as string } : { urls });
  }
  return out;
}

/** Servers typed in by hand. Empty unless there is a relay address (turn: or turns:) with its username and password. */
export function staticServers(r: Relay): RTCIceServer[] {
  const urls = r.urls.split(/[\s,]+/).filter(isServerUrl);
  const relay = urls.filter((u) => /^turns?:/i.test(u));
  if (!relay.length || !r.username.trim() || !r.credential) return [];
  return [{ urls: relay, username: r.username.trim(), credential: r.credential }];
}

/** The link to a provider's answer may only be a secure web address (it carries the account's key). */
export const credentialsUrlOk = (u: string): boolean => {
  try {
    return new URL(u.trim()).protocol === 'https:';
  } catch {
    return false;
  }
};

/** How many relay servers (turn:) a list holds, for saying "found 3". */
export const relayCount = (servers: RTCIceServer[]): number => servers.reduce((n, s) => n + (Array.isArray(s.urls) ? s.urls : [s.urls]).filter((u) => /^turns?:/i.test(u)).length, 0);
