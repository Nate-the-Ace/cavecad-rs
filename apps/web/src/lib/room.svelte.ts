// Keeps the open cave in step with its rooms on the server (a guest's shared cave too; a watcher's is locked). One RoomClient per open cave. New caves are open
// (stored as plain text on the server, no keys); a cave locked during rooms step 1 is converted to open by the first
// device holding its key that opens it (room-client.ts). The device key stays: it identifies this device to the
// server, and it opens the keys of caves not converted yet.
//
// One choke point: every way a cave comes on screen bumps `cave.opened`, so `startRooms()` watches that (with the
// account) instead of each opener calling in. Sample caves never get a room (`noRoomForThisCave()` after
// `openDemo`), so no room id is ever written into one.
//
// The room id lives in the cave (a `room-id` setting), so it travels wherever the cave's own format goes: the shelf
// and a saved CaveCAD file both keep it (a DXF export does not, and a DXF opened is a new cave). Setting it is kept
// on the shelf at once, so a reload or a reopen finds the same room. There is no "duplicate cave" yet; Save as writes
// the same cave to another place (same room). A future copy-to-a-new-cave must clear `room-id` in the copy so it
// gets a room of its own.
import { untrack } from 'svelte';
import { engine } from './engine';
import { cave, changed } from './cave.svelte';
import { flush as keepOnShelf } from './shelf.svelte';
import { account } from './account.svelte';
import { desktopBearer } from './desktop-signin';
import { deviceKey, keySeen, pendingEntries, quarantinedEntries, syncedHash } from './device-key';
import { RoomClient, handOverKeys, type Role, type RoomStatus, type ServerAccess } from './room-client';
import { setWatchLock } from './readonly';

/** Each status in plain words (the label beside the user icon, and the Rooms panel). */
export const ROOM_TEXT: Record<RoomStatus, string> = {
  connecting: 'Connecting…',
  synced: 'Synced',
  offline: 'Offline: your edits will join when you reconnect',
  'waiting-key': 'Waiting for your other device to share this cave',
  error: "Sync stopped: something didn't check out",
};

/** `reason` says why for `error`; `info` is a one-off note shown for a while in place of the status. */
export const room = $state({ status: '' as RoomStatus | '', reason: '', info: '', role: '' as Role | '' });

const BASE = (import.meta.env.VITE_ACCOUNT_BASE as string | undefined) ?? '';
const headers = (): Record<string, string> => {
  const t = desktopBearer();
  return t ? { Authorization: 'Bearer ' + t } : {};
};
let current: RoomClient | null = null;
/** Bumped on every open and close, so a slow open that was overtaken gives up. */
let generation = 0;

/** The `cave.opened` count of a cave that must not get a room (a sample). */
let roomless = -1;
let lastKey = '';
/** The member whose device could not be registered yet (the server was out of reach): retried every 30 s. */
let waitingForDevice = '';
/** The member whose keys were last handed on when they signed in. */
let sweptFor = '';
let deviceRetry: ReturnType<typeof setTimeout> | null = null;
let infoTimer: ReturnType<typeof setTimeout> | null = null;

/** The signed-in member's id ('' when signed out): a stored pass is enough, the server may be out of reach. */
const memberNow = (): string => (account.phase === 'ready' || account.phase === 'offline' ? (account.member?.id ?? '') : '');

/** This device's way to the member's caves on the server; null when signed out or the device is not registered yet. */
export async function serverAccess(): Promise<ServerAccess | null> {
  const member = memberNow();
  if (!member) return null;
  const dev = await deviceKey(BASE, member, headers);
  if (!dev || memberNow() !== member) return null;
  return { base: BASE, fetch: (...a) => fetch(...a), device: dev, headers, loadKeySeen: keySeen.load, saveKeySeen: keySeen.save };
}

/** Hands this device's keys of caves still locked to the member's devices that have none (open caves need none). */
export async function handOnKeys(): Promise<void> {
  const access = await serverAccess();
  if (access) await handOverKeys(access).catch(() => 0);
}

/** The label kept on the server for the open cave: its name and shelf summary, for the shelf on other devices. */
function caveLabel(): string | null {
  const summary = engine.shelfSummary();
  if (!summary) return null;
  const full = JSON.stringify({ name: summary.name, summary });
  return full.length <= 150_000 ? full : JSON.stringify({ name: summary.name }); // a huge trip list stays off the list
}

/** The cave just put on screen is a sample: it never gets a room. Call right after `cave.opened` is bumped. */
export function noRoomForThisCave(): void {
  roomless = cave.opened;
}

/** Starts syncing the cave now open, for this member. */
async function openRoom(member: string): Promise<void> {
  closeRoom();
  const mine = ++generation;
  room.status = 'connecting';
  await startClient(member, mine);
}

/** Registers this device if it has to, then starts the room client; until the device is known, retries. */
async function startClient(member: string, mine: number): Promise<void> {
  if (deviceRetry) clearTimeout(deviceRetry);
  deviceRetry = null;
  const dev = await deviceKey(BASE, member, headers);
  if (mine !== generation || current) return;
  if (!dev) {
    room.status = 'offline';
    waitingForDevice = member;
    deviceRetry = setTimeout(() => void startClient(member, mine), 30_000);
    return;
  }
  waitingForDevice = '';
  const client = new RoomClient({
    base: BASE,
    fetch: (...a) => fetch(...a),
    engine: {
      roomId: engine.roomId,
      setRoomId: (id) => {
        const r = engine.setRoomId(id);
        if (r === 'ok') {
          changed();
          void keepOnShelf(); // kept now, not at the next edit: a reload must find the same room
        }
        return r;
      },
      outbox: engine.syncOutbox,
      apply: engine.syncApply, // the client calls `changed` (once after catch-up, then per entry)
      snapshot: engine.syncSnapshot,
      load: (ops) => {
        const r = engine.syncLoad(ops);
        changed();
        return r;
      },
    },
    changed: () => changed(), // the views re-read, and the shelf keeps what arrived
    label: caveLabel,
    device: dev,
    headers,
    loadKeySeen: keySeen.load,
    saveKeySeen: keySeen.save,
    loadPending: pendingEntries.load,
    savePending: pendingEntries.save,
    saveQuarantine: quarantinedEntries.save,
    loadSynced: syncedHash.load,
    saveSynced: syncedHash.save,
    onStatus: (s, reason) => {
      if (current !== client) return;
      room.status = s;
      room.reason = reason ?? '';
      if (reason) console.warn('[rooms]', reason);
    },
    onRole: (role) => {
      if (current !== client) return;
      room.role = role;
      setWatchLock(role === 'watcher');
    },
    onInfo: (message) => {
      if (current !== client) return;
      room.info = message;
      if (infoTimer) clearTimeout(infoTimer);
      infoTimer = setTimeout(() => (room.info = ''), 10_000);
    },
  });
  current = client;
  await client.open();
}

/** Stops syncing (the cave is closing, or another is opening). */
function closeRoom(): void {
  generation++;
  current?.stop();
  current = null;
  if (deviceRetry) clearTimeout(deviceRetry);
  deviceRetry = null;
  waitingForDevice = '';
  room.status = '';
  room.reason = '';
  room.role = '';
  setWatchLock(false);
}

/** Watches for caves opening (and the account) and keeps one room open to match. Call once. */
export function startRooms(): () => void {
  // A device signed in elsewhere may be waiting for keys this one has: hand them on now and then, not only on open.
  const sweep = setInterval(() => void handOnKeys(), 10 * 60_000);
  return $effect.root(() => {
    $effect(() => {
      const n = cave.opened;
      const has = cave.hasCave;
      const phase = account.phase;
      const who = account.member;
      const member = phase === 'ready' || phase === 'offline' ? (who?.id ?? '') : '';
      untrack(() => {
        if (member && member !== sweptFor) {
          sweptFor = member;
          void handOnKeys();
        }
        const want = has && member !== '' && n !== roomless;
        const key = want ? `${n}|${member}` : '';
        if (key === lastKey) {
          // Still the same cave and member: if this device is not registered yet, the account changing
          // (back online, signed in again) is the moment to try again.
          if (want && waitingForDevice === member) void startClient(member, generation);
          return;
        }
        lastKey = key;
        if (want) void openRoom(member);
        else closeRoom();
      });
    });
    return () => {
      clearInterval(sweep);
      closeRoom();
    };
  });
}
