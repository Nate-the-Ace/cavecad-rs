// The desktop app's sign-in: the system browser opens TinyAuth's sign-in (Google or Microsoft), TinyAuth sends
// the browser back to cavecad://auth with a code, and the CaveCAD server swaps the code for an ID token, which
// the app sends with every API call. In a browser none of this runs: the TinyAuth cookie does the work.
import { isDesktop, openUrlNative } from './desktop.ts';

const KEY = 'cavecad-desktop-signin';
const ISSUER = (import.meta.env?.VITE_OIDC_ISSUER as string | undefined) ?? '';
const CLIENT = (import.meta.env?.VITE_OIDC_CLIENT_ID as string | undefined) ?? 'cavecad-desktop';
const BASE = (import.meta.env?.VITE_ACCOUNT_BASE as string | undefined) ?? '';
const b64url = (b: ArrayBuffer | Uint8Array) => btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

type Saved = { idToken: string; refresh: string | null; until: number };
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
/** TinyAuth's sign-in address for this attempt. */
export function authorizeUrl(endpoint: string, clientId: string, challenge: string, state: string): string {
  const u = new URL(endpoint);
  Object.entries({ response_type: 'code', client_id: clientId, redirect_uri: 'cavecad://auth', scope: 'openid email profile', code_challenge: challenge, code_challenge_method: 'S256', state }).forEach(([k, v]) => u.searchParams.set(k, v));
  return u.toString();
}
/** The code from a returning `cavecad://auth` link, or null when it is not ours. */
export function readAuthLink(link: string, state: string): string | null {
  if (!link.startsWith('cavecad://auth')) return null;
  const q = new URL(link.replace('cavecad://auth', 'https://x/auth')).searchParams;
  return q.get('state') === state ? q.get('code') : null;
}

/** The desktop app's current sign-in token, or null in a browser or when signed out. */
export function desktopBearer(): string | null {
  if (!isDesktop() || !saved) return null;
  return saved.until > Date.now() ? saved.idToken : null;
}

function keep(out: { id_token: string; refresh_token: string | null; expires_in: number }) {
  saved = { idToken: out.id_token, refresh: out.refresh_token, until: Date.now() + Math.max(60, out.expires_in - 60) * 1000 };
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

/** Refreshes an expired token if TinyAuth gave a refresh token. True when signed in afterwards. */
export async function refreshDesktop(): Promise<boolean> {
  if (desktopBearer()) return true;
  return !!saved?.refresh && exchange({ refresh: saved.refresh });
}

let pending: { verifier: string; state: string } | null = null;

/** Opens the system browser on TinyAuth's sign-in. The answer arrives as a cavecad://auth link. */
export async function startDesktopSignIn(): Promise<void> {
  const conf = (await (await fetch(ISSUER.replace(/\/$/, '') + '/.well-known/openid-configuration')).json()) as { authorization_endpoint: string };
  const { verifier, challenge } = await pkcePair();
  const state = b64url(crypto.getRandomValues(new Uint8Array(16)));
  pending = { verifier, state };
  await openUrlNative(authorizeUrl(conf.authorization_endpoint, CLIENT, challenge, state));
}

/** Handles a cavecad://auth link. True when it signed this app in. */
export async function finishDesktopSignIn(link: string): Promise<boolean> {
  if (!pending) return false;
  const code = readAuthLink(link, pending.state);
  if (!code) return false;
  const ok = await exchange({ code, verifier: pending.verifier });
  pending = null;
  return ok;
}

/** Forgets the desktop sign-in. */
export function signOutDesktop(): void {
  saved = null;
  try {
    localStorage.removeItem(KEY);
  } catch {
    // nothing kept
  }
}
