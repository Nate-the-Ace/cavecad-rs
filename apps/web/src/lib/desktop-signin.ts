// The desktop app's sign-in: the system browser opens CaveCAD's sign-in page (Google or an emailed code) with a
// PKCE challenge; when done the page sends the browser to cavecad://auth with a one-time code, and the app swaps
// it (with its verifier) at the CaveCAD server for a 30-day session token it sends with every API call.
// In a browser none of this runs: the session cookie does the work.
import { isDesktop, openUrlNative } from './desktop.ts';

const KEY = 'cavecad-desktop-signin';
const BASE = (import.meta.env?.VITE_ACCOUNT_BASE as string | undefined) ?? '';
const SIGNIN = (import.meta.env?.VITE_SIGNIN_URL as string | undefined) ?? 'https://cavecad.com/signin';
const b64url = (b: ArrayBuffer | Uint8Array) => btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

type Saved = { idToken: string; until: number };
let saved: Saved | null = null;
try {
  saved = JSON.parse(localStorage.getItem(KEY) ?? 'null');
} catch {
  saved = null;
}

/** The S256 code challenge for a verifier. */
export async function challengeFor(verifier: string): Promise<string> {
  return b64url(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier)));
}
/** A fresh PKCE verifier and its challenge. */
export async function pkcePair(): Promise<{ verifier: string; challenge: string }> {
  const verifier = b64url(crypto.getRandomValues(new Uint8Array(32)));
  return { verifier, challenge: await challengeFor(verifier) };
}
export type AuthLink = { kind: 'code'; code: string } | { kind: 'failed' };
/** What a returning `cavecad://auth` link says: the code, or that the sign-in didn't finish (an `error=`, a
 *  wrong or missing state, no code). Null when the link is not a sign-in link at all. */
export function readAuthLink(link: string, state: string | null): AuthLink | null {
  let u: URL;
  try {
    u = new URL(link);
  } catch {
    return null;
  }
  if (u.protocol !== 'cavecad:' || u.host !== 'auth') return null;
  const code = u.searchParams.get('code');
  if (!state || u.searchParams.has('error') || u.searchParams.get('state') !== state || !code) return { kind: 'failed' };
  return { kind: 'code', code };
}

/** The desktop app's current sign-in token, or null in a browser or when signed out. */
export function desktopBearer(): string | null {
  if (!isDesktop() || !saved) return null;
  return saved.until > Date.now() ? saved.idToken : null;
}

/** Keeps a new session token. */
export function keep(out: { token: string; expires_in: number }) {
  saved = { idToken: out.token, until: Date.now() + Math.max(60, out.expires_in - 60) * 1000 };
  try {
    localStorage.setItem(KEY, JSON.stringify(saved));
  } catch {
    // private window: the sign-in lasts for this run only
  }
}

async function exchange(body: Record<string, string>): Promise<boolean> {
  try {
    const res = await fetch(BASE + '/api/desktop-token', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    if (!res.ok) return false;
    keep(await res.json());
    return true;
  } catch {
    return false;
  }
}

/** Sessions are not refreshed: true while the kept one is live, false once it ran out or the server refused it. */
export async function refreshDesktop(force = false): Promise<boolean> {
  if (force) signOutDesktop();
  return !!desktopBearer();
}

let pending: { verifier: string; state: string } | null = null;

/** Opens the system browser on CaveCAD's sign-in. The answer arrives as a cavecad://auth link. */
export async function startDesktopSignIn(): Promise<void> {
  const { verifier, challenge } = await pkcePair();
  const state = b64url(crypto.getRandomValues(new Uint8Array(16)));
  pending = { verifier, state };
  const u = new URL(SIGNIN);
  u.searchParams.set('desktop', '1');
  u.searchParams.set('challenge', challenge);
  u.searchParams.set('state', state);
  await openUrlNative(u.toString());
}

/** Handles a cavecad://auth link: 'signed-in', 'failed' (show "Sign-in didn't finish"), or 'not-ours'. */
export async function finishDesktopSignIn(link: string): Promise<'signed-in' | 'failed' | 'not-ours'> {
  const read = readAuthLink(link, pending?.state ?? null);
  if (!read) return 'not-ours';
  const p = pending;
  pending = null;
  if (read.kind === 'failed' || !p) return 'failed';
  return (await exchange({ code: read.code, verifier: p.verifier })) ? 'signed-in' : 'failed';
}

/** The kept sign-in, for tests. */
export const keptSignIn = () => saved;

/** Forgets the desktop sign-in. */
export function signOutDesktop(): void {
  saved = null;
  try {
    localStorage.removeItem(KEY);
  } catch {
    // nothing kept
  }
}
