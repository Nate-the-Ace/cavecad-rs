// Keeps the open cave in step with its Private room on the server (rooms step 1). One RoomClient per open cave.
//
// One choke point: every way a cave comes on screen bumps `cave.opened`, so `startRooms()` watches that (with the
// account and the live sessions) instead of each opener calling in. Which caves never get a room:
//   - sample caves (`noRoomForThisCave()` after `openDemo`), so no room id is ever written into one;
//   - any cave that arrived through a live session (peer-to-peer, relay room, a class): it is someone else's;
//   - nothing at all while a live session runs, because the session drains `engine.syncOutbox` too. The room
//     closes when a session starts and opens again for the same cave when it ends.
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
import { deviceKey, keySeen, lastNumber } from './device-key';
import { RoomClient, type RoomStatus } from './room-client';
import { p2p } from './p2p.svelte';
import { live } from './sync.svelte';
import { classroom } from './classroom.svelte';

export const room = $state({ status: '' as RoomStatus | '' });

const BASE = (import.meta.env.VITE_ACCOUNT_BASE as string | undefined) ?? '';
const headers = (): Record<string, string> => {
  const t = desktopBearer();
  return t ? { Authorization: 'Bearer ' + t } : {};
};
let current: RoomClient | null = null;
/** Bumped on every open and close, so a slow open that was overtaken gives up. */
let generation = 0;

/** The `cave.opened` count of a cave that must not get a room (a sample, or one that came through a session). */
let roomless = -1;
let seenOpened = -1;
let lastKey = '';

/** The cave just put on screen is a sample: it never gets a room. Call right after `cave.opened` is bumped. */
export function noRoomForThisCave(): void {
  roomless = cave.opened;
}

/** Starts syncing the cave now open, for this member. */
async function openRoom(member: string): Promise<void> {
  closeRoom();
  const mine = ++generation;
  room.status = 'connecting';
  const dev = await deviceKey(BASE, member, headers);
  if (mine !== generation) return;
  if (!dev) {
    room.status = 'offline';
    return;
  }
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
      apply: (ops) => {
        const n = engine.syncApply(ops);
        changed(); // the views re-read, and the shelf keeps what arrived
        return n;
      },
      snapshot: engine.syncSnapshot,
      load: (ops) => {
        const r = engine.syncLoad(ops);
        changed();
        return r;
      },
    },
    device: dev,
    headers,
    loadLast: lastNumber.load,
    saveLast: lastNumber.save,
    loadKeySeen: keySeen.load,
    saveKeySeen: keySeen.save,
    onStatus: (s) => {
      if (current === client) room.status = s;
    },
  });
  current = client;
  await client.open();
}

/** Stops syncing (the cave is closing, another is opening, or a live session took over). */
function closeRoom(): void {
  generation++;
  current?.stop();
  current = null;
  room.status = '';
}

/** Watches for caves opening (and the account and live sessions) and keeps one room open to match. Call once. */
export function startRooms(): () => void {
  seenOpened = cave.opened;
  return $effect.root(() => {
    $effect(() => {
      const n = cave.opened;
      const has = cave.hasCave;
      const busy = p2p.role !== 'none' || live.status !== 'off';
      const student = classroom.role === 'student';
      const member = account.phase === 'ready' || account.phase === 'offline' ? (account.member?.id ?? '') : '';
      untrack(() => {
        if (busy && n !== seenOpened) roomless = n; // a cave that came through a session is not this device's own
        seenOpened = n;
        const want = has && !busy && !student && member !== '' && n !== roomless;
        const key = want ? `${n}|${member}` : '';
        if (key === lastKey) return;
        lastKey = key;
        if (want) void openRoom(member);
        else closeRoom();
      });
    });
    return () => closeRoom();
  });
}
