// Who is signed in to CaveCAD, for the whole page. Sign-in is required: the app starts by asking the server
// (startAccount); with no network it opens on the offline pass this device was given last time it was online.
// The display name and colour are the ones others see (`me` in me.svelte.ts).
import { accountApi, offlinePhase, passMember, singleFlight, type AccountApi, type Drive, type Member, type NotInvitedReason } from './account-api';
import { verifyPass } from './pass';
import { me } from './me.svelte';
import { setAccountLock } from './readonly';
import { desktopBearer, finishDesktopSignIn, refreshDesktop, signOutDesktop } from './desktop-signin';
import { isDesktop } from './desktop';

export type Phase = 'loading' | 'ready' | 'welcome' | 'not-invited' | 'removed' | 'signed-out' | 'offline' | 'offline-expired';

export const account = $state({
  phase: 'loading' as Phase,
  member: null as Member | null,
  drives: [] as Drive[],
  signups: 'invite' as 'invite' | 'open',
  /** Reached through Tailscale (the private copy): who you are is your Tailscale account, so there is no signing out here. */
  tailnet: false,
  /** Not-invited and removed pages: the email they signed in with, why, and who sent the invite. */
  email: '',
  reason: 'none' as NotInvitedReason,
  inviter: '',
  /** Signed out because the sign-in ran out (not because they never signed in): the gate says so. */
  expired: false,
  /** A desktop sign-in that came back without finishing. */
  signInError: '',
});

const OLD_PASS = "You've been offline over 30 days. Connect once to keep editing.";
const PASS_PUBLIC_KEY = (import.meta.env.VITE_PASS_PUBLIC_KEY as string | undefined) ?? '';
const BASE = (import.meta.env.VITE_ACCOUNT_BASE as string | undefined) ?? '';
export const api: AccountApi = accountApi({
  base: BASE,
  fetch: (...a) => fetch(...a),
  bearer: desktopBearer,
  onUnauthorized: () => (isDesktop() ? refreshDesktop(true) : Promise.resolve(false)),
  // The sign-in ran out mid-session. Nothing is cleared: the shelf keeps the open cave, and signing in again picks it up.
  onSignedOut: () => {
    account.expired = true;
    account.phase = 'signed-out';
  },
});

// ---- the offline pass, kept in IndexedDB ----
const DB = 'cavecad-account';
function store<T>(mode: IDBTransactionMode, f: (s: IDBObjectStore) => IDBRequest<T>): Promise<T | undefined> {
  return new Promise((resolve) => {
    let r: IDBOpenDBRequest;
    try {
      r = indexedDB.open(DB, 1);
    } catch {
      return resolve(undefined);
    }
    r.onupgradeneeded = () => r.result.createObjectStore('kv');
    r.onerror = () => resolve(undefined);
    r.onsuccess = () => {
      const db = r.result;
      const done = (v: T | undefined) => {
        resolve(v);
        db.close();
      };
      let q: IDBRequest<T>;
      try {
        q = f(db.transaction('kv', mode).objectStore('kv'));
      } catch {
        return done(undefined);
      }
      q.onsuccess = () => done(q.result);
      q.onerror = () => done(undefined);
    };
  });
}
const savePass = (pass: string, member: Member) => store('readwrite', (s) => s.put({ pass, member }, 'pass'));
const loadPass = () => store<{ pass: string; member: Member }>('readonly', (s) => s.get('pass'));
const forgetPass = () => store('readwrite', (s) => s.delete('pass'));

// ---- the invite link's sign-up token: out of the address at once, kept for this tab until it has worked ----
const SIGNUP = 'cavecad-signup';
function signupToken(): string | undefined {
  const u = new URL(location.href);
  const t = u.searchParams.get('signup');
  if (t) {
    u.searchParams.delete('signup');
    history.replaceState(history.state, '', u.toString());
    try {
      sessionStorage.setItem(SIGNUP, t);
    } catch {
      return t; // no session storage: this attempt only
    }
    return t;
  }
  try {
    return sessionStorage.getItem(SIGNUP) ?? undefined;
  } catch {
    return undefined;
  }
}
function signupUsed() {
  try {
    sessionStorage.removeItem(SIGNUP);
  } catch {
    // nothing kept
  }
}

function useMember(m: Member) {
  account.member = m;
  me.name = m.displayName;
  me.color = m.colour;
}

// ---- while offline, try again every minute, when the page is looked at again, and when the network returns ----
let stopRetry: (() => void) | null = null;
function retryWhileOffline() {
  if (stopRetry) return;
  const again = () => void startAccount();
  const seen = () => document.visibilityState === 'visible' && again();
  const timer = setInterval(again, 60_000);
  addEventListener('online', again);
  document.addEventListener('visibilitychange', seen);
  stopRetry = () => {
    clearInterval(timer);
    removeEventListener('online', again);
    document.removeEventListener('visibilitychange', seen);
    stopRetry = null;
  };
}

/** Asks the server who is signed in and sets `account.phase`. Call at start-up and after signing in; a call
 *  while one is running waits for that one, so the pass is never saved and forgotten at once. */
export const startAccount: () => Promise<void> = singleFlight(async () => {
  if (isDesktop()) await refreshDesktop();
  const r = await api.me(signupToken());
  if (r.kind === 'offline') return openOffline();
  stopRetry?.();
  setAccountLock(false);
  if (r.kind === 'member') {
    signupUsed();
    useMember(r.member);
    account.drives = r.drives;
    account.signups = r.signups;
    account.tailnet = r.via === 'tailnet';
    account.expired = false;
    account.signInError = '';
    account.phase = r.firstVisit ? 'welcome' : 'ready';
    await savePass(r.pass, r.member);
    return;
  }
  if (r.kind === 'signed-out') {
    // Someone was signed in here (online or on the pass): their sign-in ran out. Their work stays on the device.
    if (account.member || (await loadPass())) account.expired = true;
    account.phase = 'signed-out';
    return;
  }
  await forgetPass();
  account.member = null;
  account.tailnet = r.via === 'tailnet';
  if (r.kind === 'not-invited') Object.assign(account, { phase: 'not-invited', email: r.email, reason: r.reason, inviter: r.inviter ?? '' });
  else Object.assign(account, { phase: 'removed', email: r.email });
});

async function openOffline() {
  const kept = await loadPass();
  const check = kept ? await verifyPass(kept.pass, PASS_PUBLIC_KEY, Date.now()) : null;
  const phase = check ? offlinePhase(check.ok ? 'ok' : check.why) : 'signed-out';
  if (phase === 'signed-out') {
    if (kept) await forgetPass();
    setAccountLock(false);
    account.phase = 'signed-out';
    retryWhileOffline(); // once online, the server can sign them in
    return;
  }
  if (check?.ok) useMember(passMember(kept!.member, check.claims));
  else useMember(kept!.member); // an expired pass is still CaveCAD's signature: only too old to edit with
  account.phase = phase;
  setAccountLock(phase === 'offline-expired', OLD_PASS);
  retryWhileOffline();
}

/** Handles a cavecad://auth link in the desktop app. False when the link is not a sign-in link. */
export async function handleAuthLink(url: string): Promise<boolean> {
  const r = await finishDesktopSignIn(url);
  if (r === 'not-ours') return false;
  if (r === 'signed-in') await startAccount();
  else account.signInError = "Sign-in didn't finish. Try again.";
  return true;
}

/** Leaves the first-sign-in page, keeping the name the person typed. */
export async function finishWelcome(displayName: string): Promise<void> {
  const { member } = await api.updateMe({ welcomed: true, displayName });
  useMember(member);
  account.phase = 'ready';
}

/** Saves the display name and/or colour. */
export async function saveProfile(p: { displayName?: string; colour?: string }): Promise<void> {
  useMember((await api.updateMe(p)).member);
}

/** Records a connected drive (piece 2 supplies the key; until then the row shows Reconnect). */
export async function recordDrive(provider: Drive['provider'], accountLabel: string): Promise<void> {
  account.drives = [...account.drives, (await api.addDrive(provider, accountLabel)).drive];
}

/** Disconnects a drive everywhere: forgets the record (piece 2 also drops this device's key). */
export async function disconnectDrive(id: string): Promise<void> {
  await api.removeDrive(id);
  account.drives = account.drives.filter((d) => d.id !== id);
}

/** Signs out: forgets the pass and sends the browser through TinyAuth's sign-out (set at build time). The
 *  desktop app only forgets its own token: the system browser's TinyAuth session is the browser's. */
export async function signOut(): Promise<void> {
  await forgetPass();
  signOutDesktop();
  stopRetry?.();
  setAccountLock(false);
  if (isDesktop()) {
    account.member = null;
    account.expired = false;
    account.phase = 'signed-out';
    return;
  }
  location.href = (import.meta.env.VITE_SIGNOUT_URL as string | undefined) || '/';
}
