// The person's relay (TURN) settings: kept in this browser only, used by p2p.svelte.ts as the fallback
// when two devices cannot connect directly. See p2p-ice.ts for what the fields mean.

import { credentialsUrlOk, emptyRelay, normalizeServers, relayCount, staticServers, type Relay } from './p2p-ice';

const KEY = 'cavecad-relay';

function load(): Relay {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (v && typeof v === 'object') return { ...emptyRelay(), ...Object.fromEntries(Object.entries(v).filter(([, x]) => typeof x === 'string')) };
  } catch {
    /* unreadable settings are the same as none */
  }
  return emptyRelay();
}

/** The relay link built into this copy of the app (set when it is built, from a repository secret); people can still use their own. */
const BUILT_IN: string = (import.meta.env.VITE_RELAY_URL as string | undefined)?.trim() ?? '';

/** True when this copy of the app came with a relay built in, so people need to set nothing up. */
export const relayBuiltIn = BUILT_IN !== '';

export const relay = $state<Relay & { testing: boolean; result: string }>({ ...load(), testing: false, result: '' });

export function saveRelay() {
  try {
    const { url, urls, username, credential } = relay;
    localStorage.setItem(KEY, JSON.stringify({ url, urls, username, credential }));
  } catch {
    /* private mode: it stays for this visit */
  }
}

export const relayConfigured = (): boolean => !!BUILT_IN || !!relay.url.trim() || staticServers(relay).length > 0;

/** The credentials a provider gave, kept for a few minutes: every invite and every join asks, and the answer does not change that fast. */
let cached: { key: string; at: number; servers: RTCIceServer[] } | null = null;
const CACHE_MS = 5 * 60_000;

/** The relay servers to offer to a new connection. Never throws: a relay that cannot be reached just means direct-only. */
export async function relayServers(): Promise<RTCIceServer[]> {
  const key = `${relay.url}|${BUILT_IN}|${relay.urls}|${relay.username}`;
  if (cached && cached.key === key && Date.now() - cached.at < CACHE_MS) return cached.servers;
  const servers = await fetchRelayServers();
  if (relayCount(servers)) cached = { key, at: Date.now(), servers };
  return servers;
}

async function fetchRelayServers(): Promise<RTCIceServer[]> {
  const out = staticServers(relay);
  for (const url of [relay.url.trim(), BUILT_IN]) {
    if (!url || !credentialsUrlOk(url)) continue;
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(5000) });
      if (res.ok) out.push(...normalizeServers(await res.json()));
    } catch {
      /* offline or a wrong link: carry on without it */
    }
    if (relayCount(out)) break; // their own link first; the built-in one only if theirs gave nothing
  }
  return out;
}

/** Ask the relay for an address from this device alone: if one comes back, the login works. */
export async function testRelay(): Promise<void> {
  relay.result = '';
  if (!relayConfigured()) {
    relay.result = 'Fill in the relay first.';
    return;
  }
  if (relay.url.trim() && !credentialsUrlOk(relay.url)) {
    relay.result = 'The link must start with https://';
    return;
  }
  relay.testing = true;
  try {
    const servers = await relayServers();
    if (!relayCount(servers)) {
      relay.result = relay.url.trim() ? 'The link gave no relay servers. Check it, and that the key is right.' : 'Nothing usable: a relay needs an address starting turn: or turns:, plus the username and password.';
      return;
    }
    const pc = new RTCPeerConnection({ iceServers: servers, iceTransportPolicy: 'relay' });
    pc.createDataChannel('t');
    let found = false;
    pc.onicecandidate = (e) => {
      if (e.candidate?.type === 'relay') found = true;
    };
    await pc.setLocalDescription(await pc.createOffer());
    await new Promise<void>((resolve) => {
      const t = setTimeout(resolve, 8000);
      pc.onicegatheringstatechange = () => {
        if (pc.iceGatheringState === 'complete') {
          clearTimeout(t);
          resolve();
        }
      };
      if (found) {
        clearTimeout(t);
        resolve();
      }
    });
    pc.close();
    relay.result = found ? `Relay works (${relayCount(servers)} servers found).` : 'The relay did not answer. Check the username, password and address.';
  } catch {
    relay.result = 'This browser could not test the relay.';
  } finally {
    relay.testing = false;
  }
}
