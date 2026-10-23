// Who is signed in to CaveCAD, for the whole page. Sign-in is required: the app starts by asking the server
// (startAccount); with no network it opens on the offline pass this device was given last time it was online.
// The display name and colour are the ones live sessions show (`me` in sync.svelte.ts).
import { accountApi, type AccountApi, type Drive, type Member, type NotInvitedReason } from './account-api';
import { verifyPass } from './pass';
import { me } from './sync.svelte';
import { readOnly } from './readonly';
import { desktopBearer, refreshDesktop, signOutDesktop } from './desktop-signin';
import { isDesktop } from './desktop';

export type Phase = 'loading' | 'ready' | 'welcome' | 'not-invited' | 'removed' | 'signed-out' | 'offline' | 'offline-expired';

export const account = $state({
  phase: 'loading' as Phase,
  member: null as Member | null,
  drives: [] as Drive[],
  signups: 'invite' as 'invite' | 'open',
  /** Not-invited and removed pages: the email they signed in with, why, and who sent the invite. */
  email: '',
  reason: 'none' as NotInvitedReason,
  inviter: '',
});

const PASS_PUBLIC_KEY = (import.meta.env.VITE_PASS_PUBLIC_KEY as string | undefined) ?? '';
const BASE = (import.meta.env.VITE_ACCOUNT_BASE as string | undefined) ?? '';
export const api: AccountApi = accountApi({ base: BASE, fetch: (...a) => fetch(...a), bearer: desktopBearer });

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
      const q = f(r.result.transaction('kv', mode).objectStore('kv'));
      q.onsuccess = () => resolve(q.result);
      q.onerror = () => resolve(undefined);
    };
  });
}
const savePass = (pass: string, member: Member) => store('readwrite', (s) => s.put({ pass, member }, 'pass'));
const loadPass = () => store<{ pass: string; member: Member }>('readonly', (s) => s.get('pass'));
const forgetPass = () => store('readwrite', (s) => s.delete('pass'));

function takeSignupToken(): string | undefined {
  const u = new URL(location.href);
  const t = u.searchParams.get('signup') ?? undefined;
  if (t) {
    u.searchParams.delete('signup');
    history.replaceState(history.state, '', u.toString());
  }
  return t;
}

function useMember(m: Member) {
  account.member = m;
  me.name = m.displayName;
  me.color = m.colour;
}

/** Asks the server who is signed in and sets `account.phase`. Call once at start-up and after signing in. */
export async function startAccount(): Promise<void> {
  if (isDesktop()) await refreshDesktop();
  const r = await api.me(takeSignupToken());
  if (r.kind === 'member') {
    useMember(r.member);
    account.drives = r.drives;
    account.signups = r.signups;
    account.phase = r.firstVisit ? 'welcome' : 'ready';
    readOnly.on = false;
    await savePass(r.pass, r.member);
    return;
  }
  if (r.kind === 'offline') return openOffline();
  await forgetPass();
  account.member = null;
  if (r.kind === 'not-invited') Object.assign(account, { phase: 'not-invited', email: r.email, reason: r.reason, inviter: r.inviter ?? '' });
  else if (r.kind === 'removed') Object.assign(account, { phase: 'removed', email: r.email });
  else account.phase = 'signed-out';
}

async function openOffline() {
  const kept = await loadPass();
  if (!kept) {
    account.phase = 'signed-out';
    return;
  }
  const check = await verifyPass(kept.pass, PASS_PUBLIC_KEY, Date.now());
  useMember(kept.member);
  if (check.ok) {
    account.phase = 'offline';
  } else {
    account.phase = 'offline-expired';
    readOnly.on = true;
  }
  addEventListener('online', () => void startAccount(), { once: true });
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

/** Signs out: forgets the pass and sends the browser through TinyAuth's sign-out. */
export async function signOut(): Promise<void> {
  await forgetPass();
  signOutDesktop();
  if (isDesktop()) {
    account.member = null;
    account.phase = 'signed-out';
    return;
  }
  location.href = (import.meta.env.VITE_SIGNOUT_URL as string | undefined) || '/';
}
