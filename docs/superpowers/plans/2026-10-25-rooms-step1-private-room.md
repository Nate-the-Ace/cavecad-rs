# Rooms step 1: device keys, cave locks, the Private room — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every cave gets a Private room on the CaveCAD server: its edits are locked on the device, stored on the server as numbered locked entries, and a member's own devices stay in step through it; the server can never read a cave.

**Architecture:** Build step 1 of `docs/superpowers/specs/2026-10-25-server-rooms-phase1-design.md`. Server (`apps/server`, zero dependencies) gains devices, caves, wrapped keys, locked history and snapshots in SQLite, and three routes groups: devices, caves, and a per-cave stream. Live delivery is a streamed HTTP response (newline-delimited JSON over `fetch`, works with the TinyAuth cookie and the desktop bearer alike, needs no WebSocket library). Devices hold an X25519 key pair (WebCrypto, private key non-extractable in IndexedDB); each cave has an AES-256-GCM lock key wrapped per device; entries are locked with it. A pure `room-client.ts` drives sync against an engine-like interface so node tests run two devices against the real server in one process.

**Tech Stack:** Node 26 (`node:sqlite`, `node:http`, `node:test`), WebCrypto (X25519, HKDF, AES-GCM) in browsers and Node, Rust (`cave-draw`, `cave-wasm`) for the cave's room id, Svelte 5 runes.

**Spec:** `docs/superpowers/specs/2026-10-25-server-rooms-phase1-design.md` (build order step 1).

**User decisions (already made):**
- "Each cave gets a basic closed private room, then the owner(s) can open it up with roles, invites, and add other rooms as needed."
- "Yes, use per-device keys."
- Server never holds a key that opens a cave; server reading is off for every cave until phase 7.
- Storage stays local files and folders; the server keeps the locked history ("the edit history is fine, just dont keep the data itself").

**Scope:** step 1 only. Steps 2-6 (extra rooms and links, presence, removal and lock changes, newcomers, retiring peer-to-peer) each get their own plan after this lands. In step 1 the only people with a cave are its owner (the member who created it); there are no seats, links or other members yet.

**Conventions for every task:**
- Work in `~/Documents/github/cavecad-rs-rooms` (branch `server-rooms`). Never touch `~/Documents/github/cavecad-rs` or `~/Documents/github/cavecad-rs-accounts`.
- Server tests: `cd apps/server && node --test`. Web tests: `node --experimental-strip-types tests/<name>.check.ts` with the `ok()` counter style; add each new one to `apps/web/package.json` `test`.
- Node's strip-only TypeScript: no parameter properties, enums or namespaces; imports use `.ts` extensions in server and node-tested web files.
- Rust: `export PATH=/opt/homebrew/opt/rustup/bin:$PATH`; `cargo test -p cave-draw`; regenerate `python3 tools/make_code_index.py` after adding public items.
- Nothing readable about a cave ever reaches the server: only base64 of locked bytes, sizes, numbers, ids.
- Commit after each task; messages end with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

## File structure

```
apps/server/src/http.ts            new: send(), readJson(req, max) shared by api.ts and rooms-api.ts
apps/server/src/db.ts              modify: room tables
apps/server/src/rooms.ts           new: devices, caves, wrapped keys, history, snapshots (pure over db)
apps/server/src/streams.ts         new: live listeners per cave (in memory)
apps/server/src/rooms-api.ts       new: /api/devices, /api/caves, /api/caves/:id/{keys,entries,stream,snapshot}
apps/server/src/api.ts             modify: use http.ts; route rooms paths to rooms-api.ts
apps/server/tests/rooms.test.ts, rooms-api.test.ts   new
crates/cave-draw/src/lib.rs        modify: room_id / set_room_id (a Setting, like drawing-scale)
crates/cave-wasm/src/lib.rs        modify: room_id / set_room_id bindings
apps/web/src/lib/engine.ts         modify: roomId / setRoomId wrappers
apps/web/src/lib/room-crypto.ts    new: key pairs, cave keys, wrap/unwrap, lock/unlock entries (pure)
apps/web/src/lib/room-client.ts    new: sync loop against an engine-like interface (pure)
apps/web/src/lib/device-key.ts     new: this device's key pair in IndexedDB, registered with the server
apps/web/src/lib/room.svelte.ts    new: wires room-client to the open cave; status for the UI
apps/web/src/components/RoomStatus.svelte  new: "Synced" / "Offline: edits will join" / "Waiting for your other device"
apps/web/tests/roomcrypto.check.ts, roomclient.check.ts   new
```

---

### Task 1: Shared HTTP helpers

**Goal:** `send` and `readJson` live in `apps/server/src/http.ts` with a size limit parameter, and `api.ts` uses them unchanged in behaviour.

**Files:**
- Create: `apps/server/src/http.ts`
- Modify: `apps/server/src/api.ts` (remove its own `send`, `BadBody`, `readJson`; import from `./http.ts`)

**Acceptance Criteria:**
- [ ] `readJson(req)` behaves as before (64 KB, plain object only, `BadBody` extends `SyntaxError`).
- [ ] `readJson(req, max)` accepts a larger limit.
- [ ] All existing server tests pass.

**Verify:** `cd apps/server && node --test` → `# fail 0`

**Steps:**

- [ ] **Step 1: Create `apps/server/src/http.ts`**

```ts
// Small HTTP helpers shared by the account API and the rooms API: JSON answers and JSON bodies with a size limit.
import type { IncomingMessage, ServerResponse } from 'node:http';

/** Answers with JSON, never cached. */
export function send(res: ServerResponse, status: number, body: unknown): void {
  res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
  res.end(JSON.stringify(body));
}

/** A body that is too big or not a JSON object; answered 400. */
export class BadBody extends SyntaxError {}

/** Reads a JSON object body of at most `max` bytes (64 KB by default). */
export async function readJson(req: IncomingMessage, max = 64 * 1024): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of req) {
    const b = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    size += b.length;
    if (size > max) throw new BadBody('too-big');
    chunks.push(b);
  }
  const s = Buffer.concat(chunks).toString('utf8');
  if (!s) return {};
  const v: unknown = JSON.parse(s);
  if (typeof v !== 'object' || v === null || Array.isArray(v)) throw new BadBody('not-an-object');
  return v as Record<string, unknown>;
}
```

- [ ] **Step 2: Use it in `api.ts`** — delete the local `send`, `class BadBody`, and `readJson` definitions and add `import { send, readJson } from './http.ts';` beside the other imports.

- [ ] **Step 3: Run** `cd apps/server && node --test` → `# fail 0`

- [ ] **Step 4: Commit**

```bash
git add apps/server/src/http.ts apps/server/src/api.ts
git commit -m "Server: shared JSON helpers with a size limit"
```

---

### Task 2: Room records and rules

**Goal:** The server stores devices, caves with their owner and Private room, wrapped keys, numbered locked history and snapshots, with step 1's rule: only a cave's owner touches it.

**Files:**
- Modify: `apps/server/src/db.ts` (append tables to `SCHEMA`)
- Create: `apps/server/src/rooms.ts`, `apps/server/tests/rooms.test.ts`

**Acceptance Criteria:**
- [ ] `registerDevice` stores a member's device public key and is idempotent for the same key.
- [ ] `createCave` makes the cave, its owner, its Private room, and stores the given wrapped keys (key version 1); a second `createCave` with the same id is refused (`exists`).
- [ ] `myCaves(member, device)` lists the member's caves with this device's wrapped key (or null if none yet).
- [ ] `devicesMissingKey(cave, member)` lists the owner's devices that have no wrapped key for the cave's current version.
- [ ] `appendEntry` numbers entries 1, 2, 3… per cave; `entriesAfter(cave, n, limit)` returns them in order.
- [ ] `putSnapshot` keeps the latest snapshot; `latestSnapshot` returns it.
- [ ] A member who is not the owner gets `not-found` from every function (step 1 has no other members).
- [ ] Blobs are stored exactly as given (base64 text) and never parsed.

**Verify:** `cd apps/server && node --test` → `# fail 0`

**Steps:**

- [ ] **Step 1: Append to `SCHEMA` in `apps/server/src/db.ts`** (before the `INSERT OR IGNORE` line):

```sql
CREATE TABLE IF NOT EXISTS devices (
  id TEXT PRIMARY KEY,
  member_id TEXT NOT NULL REFERENCES members(id),
  public_key TEXT NOT NULL UNIQUE,
  created_at INTEGER NOT NULL,
  last_seen_at INTEGER
);
CREATE TABLE IF NOT EXISTS caves (
  id TEXT PRIMARY KEY,
  key_version INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS cave_owners (
  cave_id TEXT NOT NULL REFERENCES caves(id),
  member_id TEXT NOT NULL REFERENCES members(id),
  PRIMARY KEY (cave_id, member_id)
);
CREATE TABLE IF NOT EXISTS rooms (
  id TEXT PRIMARY KEY,
  cave_id TEXT NOT NULL REFERENCES caves(id),
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('private', 'shared')),
  waiting_room INTEGER NOT NULL DEFAULT 0,
  editors_keep INTEGER NOT NULL DEFAULT 1,
  watchers_keep INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS wrapped_keys (
  cave_id TEXT NOT NULL REFERENCES caves(id),
  key_version INTEGER NOT NULL,
  device_id TEXT NOT NULL REFERENCES devices(id),
  wrapped TEXT NOT NULL,
  PRIMARY KEY (cave_id, key_version, device_id)
);
CREATE TABLE IF NOT EXISTS history (
  cave_id TEXT NOT NULL REFERENCES caves(id),
  number INTEGER NOT NULL,
  key_version INTEGER NOT NULL,
  locked TEXT NOT NULL,
  author_member TEXT NOT NULL,
  author_device TEXT NOT NULL,
  room_id TEXT NOT NULL,
  received_at INTEGER NOT NULL,
  PRIMARY KEY (cave_id, number)
);
CREATE TABLE IF NOT EXISTS snapshots (
  cave_id TEXT PRIMARY KEY REFERENCES caves(id),
  up_to INTEGER NOT NULL,
  key_version INTEGER NOT NULL,
  locked TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
```

- [ ] **Step 2: Write the failing test** `apps/server/tests/rooms.test.ts`

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openDb } from '../src/db.ts';
import { resolve, createInvite } from '../src/members.ts';
import { appendEntry, createCave, devicesMissingKey, entriesAfter, latestSnapshot, myCaves, putSnapshot, putWrappedKeys, registerDevice } from '../src/rooms.ts';

const T = 1_800_000_000_000;
function setup() {
  const db = openDb(':memory:');
  const a = resolve(db, { email: 'nathan@example.com', name: 'Nathan', provider: 'google', subject: 'a' }, { adminEmail: 'nathan@example.com', now: T });
  const owner = a.status === 'member' ? a.member.id : '';
  createInvite(db, owner, 'rick@example.com', 'member', T);
  const b = resolve(db, { email: 'rick@example.com', name: 'Rick', provider: 'google', subject: 'b' }, { adminEmail: 'nathan@example.com', now: T });
  const other = b.status === 'member' ? b.member.id : '';
  return { db, owner, other };
}

test('devices register once per key', () => {
  const { db, owner } = setup();
  const d1 = registerDevice(db, owner, 'PUB1', T);
  assert.equal(registerDevice(db, owner, 'PUB1', T + 1).id, d1.id);
  assert.notEqual(registerDevice(db, owner, 'PUB2', T).id, d1.id);
});

test('a cave is made with its owner, Private room and wrapped keys', () => {
  const { db, owner } = setup();
  const laptop = registerDevice(db, owner, 'PUB1', T);
  const tablet = registerDevice(db, owner, 'PUB2', T);
  createCave(db, owner, 'cave-1', [{ device: laptop.id, wrapped: 'W1' }], T);
  assert.throws(() => createCave(db, owner, 'cave-1', [], T), /exists/);
  const mine = myCaves(db, owner, laptop.id);
  assert.equal(mine.length, 1);
  assert.equal(mine[0].id, 'cave-1');
  assert.equal(mine[0].keyVersion, 1);
  assert.equal(mine[0].wrapped, 'W1');
  assert.equal(mine[0].privateRoom.length > 0, true);
  assert.equal(myCaves(db, owner, tablet.id)[0].wrapped, null);
  assert.deepEqual(devicesMissingKey(db, owner, 'cave-1').map((d) => d.publicKey), ['PUB2']);
  putWrappedKeys(db, owner, 'cave-1', 1, [{ device: tablet.id, wrapped: 'W2' }]);
  assert.equal(myCaves(db, owner, tablet.id)[0].wrapped, 'W2');
  assert.equal(devicesMissingKey(db, owner, 'cave-1').length, 0);
});

test('history is numbered per cave and read back in order', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1', T);
  createCave(db, owner, 'c', [{ device: d.id, wrapped: 'W' }], T);
  assert.equal(appendEntry(db, owner, d.id, 'c', 1, 'AAA', T), 1);
  assert.equal(appendEntry(db, owner, d.id, 'c', 1, 'BBB', T), 2);
  assert.equal(appendEntry(db, owner, d.id, 'c', 1, 'CCC', T), 3);
  assert.deepEqual(entriesAfter(db, owner, 'c', 1, 10).map((e) => [e.number, e.locked]), [[2, 'BBB'], [3, 'CCC']]);
  assert.deepEqual(entriesAfter(db, owner, 'c', 0, 2).map((e) => e.number), [1, 2]);
  assert.equal(entriesAfter(db, owner, 'c', 0, 10)[0].authorDevice, d.id);
});

test('snapshots keep the latest', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1', T);
  createCave(db, owner, 'c', [{ device: d.id, wrapped: 'W' }], T);
  assert.equal(latestSnapshot(db, owner, 'c'), null);
  putSnapshot(db, owner, 'c', 5, 1, 'SNAP5', T);
  putSnapshot(db, owner, 'c', 9, 1, 'SNAP9', T);
  assert.deepEqual(latestSnapshot(db, owner, 'c'), { upTo: 9, keyVersion: 1, locked: 'SNAP9' });
});

test('someone else gets not-found everywhere', () => {
  const { db, owner, other } = setup();
  const d = registerDevice(db, owner, 'PUB1', T);
  const od = registerDevice(db, other, 'PUB9', T);
  createCave(db, owner, 'c', [{ device: d.id, wrapped: 'W' }], T);
  assert.equal(myCaves(db, other, od.id).length, 0);
  assert.throws(() => appendEntry(db, other, od.id, 'c', 1, 'X', T), /not-found/);
  assert.throws(() => entriesAfter(db, other, 'c', 0, 10), /not-found/);
  assert.throws(() => putSnapshot(db, other, 'c', 1, 1, 'X', T), /not-found/);
  assert.throws(() => putWrappedKeys(db, other, 'c', 1, [{ device: od.id, wrapped: 'X' }]), /not-found/);
  assert.throws(() => devicesMissingKey(db, other, 'c'), /not-found/);
});

test('wrapped keys go only to the owner\'s own devices, and a device must be the caller\'s', () => {
  const { db, owner, other } = setup();
  const d = registerDevice(db, owner, 'PUB1', T);
  const od = registerDevice(db, other, 'PUB9', T);
  createCave(db, owner, 'c', [{ device: d.id, wrapped: 'W' }], T);
  assert.throws(() => putWrappedKeys(db, owner, 'c', 1, [{ device: od.id, wrapped: 'X' }]), /bad-device/);
  assert.throws(() => appendEntry(db, owner, od.id, 'c', 1, 'X', T), /bad-device/);
  assert.throws(() => createCave(db, owner, 'c2', [{ device: od.id, wrapped: 'X' }], T), /bad-device/);
});
```

- [ ] **Step 3: Run, expect failure** — `node --test` → `Cannot find module '../src/rooms.ts'`

- [ ] **Step 4: Write `apps/server/src/rooms.ts`**

```ts
// Rooms, step 1: each cave's Private room. The server keeps devices' public keys, each cave's lock key
// wrapped for each device (which it cannot open), and the cave's history as numbered locked entries. It never
// reads an entry: blobs are base64 text stored as given. In step 1 only a cave's owner can reach it.
import { randomUUID } from 'node:crypto';
import type { Db } from './db.ts';

export type Device = { id: string; memberId: string; publicKey: string };
export type Wrapped = { device: string; wrapped: string };
export type MyCave = { id: string; keyVersion: number; wrapped: string | null; privateRoom: string };
export type Entry = { number: number; keyVersion: number; locked: string; authorMember: string; authorDevice: string; receivedAt: number };

const fail = (code: string): never => {
  throw new Error(code);
};

/** Registers a device's public key for a member (the same key gives the same device back). */
export function registerDevice(db: Db, memberId: string, publicKey: string, now: number): Device {
  if (!/^[A-Za-z0-9_-]{20,200}$/.test(publicKey)) fail('bad-key');
  const found = db.prepare('SELECT id, member_id FROM devices WHERE public_key = ?').get(publicKey) as { id: string; member_id: string } | undefined;
  if (found) {
    if (found.member_id !== memberId) fail('bad-key');
    db.prepare('UPDATE devices SET last_seen_at = ? WHERE id = ?').run(now, found.id);
    return { id: found.id, memberId, publicKey };
  }
  const id = randomUUID();
  db.prepare('INSERT INTO devices (id, member_id, public_key, created_at, last_seen_at) VALUES (?, ?, ?, ?, ?)').run(id, memberId, publicKey, now, now);
  return { id, memberId, publicKey };
}

const isOwner = (db: Db, memberId: string, caveId: string) => !!db.prepare('SELECT 1 FROM cave_owners WHERE cave_id = ? AND member_id = ?').get(caveId, memberId);
function requireOwner(db: Db, memberId: string, caveId: string): void {
  if (!isOwner(db, memberId, caveId)) fail('not-found');
}
function requireOwnDevice(db: Db, memberId: string, deviceId: string): void {
  const row = db.prepare('SELECT member_id FROM devices WHERE id = ?').get(deviceId) as { member_id: string } | undefined;
  if (!row || row.member_id !== memberId) fail('bad-device');
}
const keyVersion = (db: Db, caveId: string) => (db.prepare('SELECT key_version FROM caves WHERE id = ?').get(caveId) as { key_version: number }).key_version;

/** Makes a cave with its owner and Private room, and stores its lock key (version 1) wrapped for the given devices. */
export function createCave(db: Db, memberId: string, caveId: string, wrapped: Wrapped[], now: number): void {
  if (!/^[A-Za-z0-9_-]{8,64}$/.test(caveId)) fail('bad-id');
  if (db.prepare('SELECT 1 FROM caves WHERE id = ?').get(caveId)) fail('exists');
  for (const w of wrapped) requireOwnDevice(db, memberId, w.device);
  db.exec('BEGIN');
  try {
    db.prepare('INSERT INTO caves (id, key_version, created_at) VALUES (?, 1, ?)').run(caveId, now);
    db.prepare('INSERT INTO cave_owners (cave_id, member_id) VALUES (?, ?)').run(caveId, memberId);
    db.prepare("INSERT INTO rooms (id, cave_id, name, kind, created_at) VALUES (?, ?, 'Private', 'private', ?)").run(randomUUID(), caveId, now);
    for (const w of wrapped) db.prepare('INSERT INTO wrapped_keys (cave_id, key_version, device_id, wrapped) VALUES (?, 1, ?, ?)').run(caveId, w.device, w.wrapped);
    db.exec('COMMIT');
  } catch (e) {
    db.exec('ROLLBACK');
    throw e;
  }
}

/** The member's caves, each with this device's wrapped key for the current version (null when it has none yet). */
export function myCaves(db: Db, memberId: string, deviceId: string): MyCave[] {
  const rows = db
    .prepare(
      `SELECT c.id, c.key_version, w.wrapped, r.id AS room FROM caves c
       JOIN cave_owners o ON o.cave_id = c.id AND o.member_id = ?
       JOIN rooms r ON r.cave_id = c.id AND r.kind = 'private'
       LEFT JOIN wrapped_keys w ON w.cave_id = c.id AND w.key_version = c.key_version AND w.device_id = ?
       ORDER BY c.created_at`,
    )
    .all(memberId, deviceId) as { id: string; key_version: number; wrapped: string | null; room: string }[];
  return rows.map((r) => ({ id: r.id, keyVersion: r.key_version, wrapped: r.wrapped, privateRoom: r.room }));
}

/** The owner's devices that have no wrapped key for the cave's current version. */
export function devicesMissingKey(db: Db, memberId: string, caveId: string): { id: string; publicKey: string }[] {
  requireOwner(db, memberId, caveId);
  return db
    .prepare(
      `SELECT d.id, d.public_key FROM devices d WHERE d.member_id = ? AND NOT EXISTS
       (SELECT 1 FROM wrapped_keys w WHERE w.cave_id = ? AND w.key_version = ? AND w.device_id = d.id) ORDER BY d.created_at`,
    )
    .all(memberId, caveId, keyVersion(db, caveId))
    .map((r) => ({ id: (r as { id: string }).id, publicKey: (r as { public_key: string }).public_key }));
}

/** Stores wrapped copies of a key version for the owner's own devices. */
export function putWrappedKeys(db: Db, memberId: string, caveId: string, version: number, wrapped: Wrapped[]): void {
  requireOwner(db, memberId, caveId);
  for (const w of wrapped) requireOwnDevice(db, memberId, w.device);
  for (const w of wrapped) db.prepare('INSERT OR REPLACE INTO wrapped_keys (cave_id, key_version, device_id, wrapped) VALUES (?, ?, ?, ?)').run(caveId, version, w.device, w.wrapped);
}

/** Adds a locked entry to the cave's history; gives back its number. */
export function appendEntry(db: Db, memberId: string, deviceId: string, caveId: string, version: number, locked: string, now: number): number {
  requireOwner(db, memberId, caveId);
  requireOwnDevice(db, memberId, deviceId);
  const room = (db.prepare("SELECT id FROM rooms WHERE cave_id = ? AND kind = 'private'").get(caveId) as { id: string }).id;
  const next = ((db.prepare('SELECT MAX(number) AS n FROM history WHERE cave_id = ?').get(caveId) as { n: number | null }).n ?? 0) + 1;
  db.prepare('INSERT INTO history (cave_id, number, key_version, locked, author_member, author_device, room_id, received_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)').run(caveId, next, version, locked, memberId, deviceId, room, now);
  return next;
}

/** Entries after `after`, oldest first, at most `limit`. */
export function entriesAfter(db: Db, memberId: string, caveId: string, after: number, limit: number): Entry[] {
  requireOwner(db, memberId, caveId);
  return (db.prepare('SELECT number, key_version, locked, author_member, author_device, received_at FROM history WHERE cave_id = ? AND number > ? ORDER BY number LIMIT ?').all(caveId, after, limit) as {
    number: number; key_version: number; locked: string; author_member: string; author_device: string; received_at: number;
  }[]).map((r) => ({ number: r.number, keyVersion: r.key_version, locked: r.locked, authorMember: r.author_member, authorDevice: r.author_device, receivedAt: r.received_at }));
}

/** Keeps a locked whole-cave snapshot covering entries up to `upTo`. */
export function putSnapshot(db: Db, memberId: string, caveId: string, upTo: number, version: number, locked: string, now: number): void {
  requireOwner(db, memberId, caveId);
  db.prepare('INSERT OR REPLACE INTO snapshots (cave_id, up_to, key_version, locked, created_at) VALUES (?, ?, ?, ?, ?)').run(caveId, upTo, version, locked, now);
}

/** The latest snapshot, or null. */
export function latestSnapshot(db: Db, memberId: string, caveId: string): { upTo: number; keyVersion: number; locked: string } | null {
  requireOwner(db, memberId, caveId);
  const r = db.prepare('SELECT up_to, key_version, locked FROM snapshots WHERE cave_id = ?').get(caveId) as { up_to: number; key_version: number; locked: string } | undefined;
  return r ? { upTo: r.up_to, keyVersion: r.key_version, locked: r.locked } : null;
}
```

- [ ] **Step 5: Run, expect pass** — `node --test` → `# fail 0` (the existing `db.test.ts` "four tables" test will now fail because there are more tables: update its expected list to `['cave_owners', 'caves', 'devices', 'drives', 'history', 'invites', 'members', 'rooms', 'settings', 'snapshots', 'wrapped_keys']`).

- [ ] **Step 6: Commit**

```bash
git add apps/server/src/db.ts apps/server/src/rooms.ts apps/server/tests/rooms.test.ts apps/server/tests/db.test.ts
git commit -m "Server: devices, caves, wrapped keys and locked history for the Private room"
```

---

### Task 3: Rooms API and the live stream

**Goal:** Members reach their caves over HTTP: register a device, list caves, make a cave, add wrapped keys, post locked entries, read a snapshot, and hold a live stream that sends missed entries then new ones as they arrive.

**Files:**
- Create: `apps/server/src/streams.ts`, `apps/server/src/rooms-api.ts`, `apps/server/tests/rooms-api.test.ts`
- Modify: `apps/server/src/api.ts` (route `/api/devices` and `/api/caves` paths to `handleRooms` after the member check)

**Acceptance Criteria:**
- [ ] `POST /api/devices {publicKey}` → `{device}`; requests after it carry `X-Cave-Device: <id>`.
- [ ] `GET /api/caves` → `{caves}` with this device's wrapped keys; `POST /api/caves {id, wrapped}` makes one; `GET /api/caves/:id/missing` and `POST /api/caves/:id/keys {version, wrapped}` hand keys to the owner's other devices.
- [ ] `POST /api/caves/:id/entries {version, locked}` (body up to 2 MB) → `{number}` and reaches every open stream of that cave.
- [ ] `GET /api/caves/:id/stream?after=N` answers `application/x-ndjson`: one JSON line per entry after N in order, then each new entry as it arrives, plus a `{"t":"ping"}` line every 25 s; closes cleanly when the client goes.
- [ ] `PUT /api/caves/:id/snapshot {upTo, version, locked}` (up to 64 MB) and `GET` it.
- [ ] Errors: `not-found` 404, `exists` 409, `bad-device`/`bad-key`/`bad-id` 400; a missing or foreign `X-Cave-Device` is `bad-device`.
- [ ] Writes still pass the CSRF rule (JSON content type, same origin or desktop bearer).

**Verify:** `cd apps/server && node --test` → `# fail 0`

**Steps:**

- [ ] **Step 1: Write `apps/server/src/streams.ts`**

```ts
// Live listeners: for each cave, the open streams waiting for new entries. In memory only; a restart drops them
// and devices reconnect, asking for what they missed by number.
import type { ServerResponse } from 'node:http';

const byCave = new Map<string, Set<ServerResponse>>();

/** Starts listening for a cave's new entries on an open stream; returns the function that stops it. */
export function listen(caveId: string, res: ServerResponse): () => void {
  let set = byCave.get(caveId);
  if (!set) byCave.set(caveId, (set = new Set()));
  set.add(res);
  return () => {
    set!.delete(res);
    if (set!.size === 0) byCave.delete(caveId);
  };
}

/** Sends one JSON line to every open stream of a cave. */
export function broadcast(caveId: string, line: object): void {
  const text = JSON.stringify(line) + '\n';
  for (const res of byCave.get(caveId) ?? []) res.write(text);
}

/** How many streams are open for a cave (for tests). */
export function listeners(caveId: string): number {
  return byCave.get(caveId)?.size ?? 0;
}
```

- [ ] **Step 2: Write `apps/server/src/rooms-api.ts`**

```ts
// The rooms API (step 1: the Private room). The member is already known (api.ts); the device is named by the
// X-Cave-Device header and must be one of the member's own. Entries are locked text the server stores and passes on.
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Db } from './db.ts';
import { readJson, send } from './http.ts';
import * as R from './rooms.ts';
import { broadcast, listen } from './streams.ts';

const STATUS: Record<string, number> = { 'not-found': 404, exists: 409, 'bad-device': 400, 'bad-key': 400, 'bad-id': 400, 'bad-request': 400 };
const PING_MS = 25_000;
const BATCH = 500;

const text = (v: unknown, max: number): string => {
  if (typeof v !== 'string' || v.length === 0 || v.length > max || !/^[A-Za-z0-9+/=_-]+$/.test(v)) throw new Error('bad-request');
  return v;
};
const int = (v: unknown): number => {
  if (typeof v !== 'number' || !Number.isInteger(v) || v < 0) throw new Error('bad-request');
  return v;
};
const wrappedList = (v: unknown): R.Wrapped[] => {
  if (!Array.isArray(v) || v.length > 100) throw new Error('bad-request');
  return v.map((w) => ({ device: text((w as R.Wrapped)?.device, 64), wrapped: text((w as R.Wrapped)?.wrapped, 4096) }));
};

/** Handles `/api/devices` and `/api/caves...` for a signed-in member. */
export async function handleRooms(db: Db, memberId: string, req: IncomingMessage, res: ServerResponse, now: number): Promise<void> {
  const url = new URL(req.url ?? '/', 'http://x');
  const path = url.pathname;
  const method = req.method ?? 'GET';
  const device = String(req.headers['x-cave-device'] ?? '');
  try {
    if (path === '/api/devices' && method === 'POST') {
      return send(res, 200, { device: R.registerDevice(db, memberId, text((await readJson(req)).publicKey, 200), now) });
    }
    if (!device) throw new Error('bad-device');
    if (path === '/api/caves' && method === 'GET') return send(res, 200, { caves: R.myCaves(db, memberId, device) });
    if (path === '/api/caves' && method === 'POST') {
      const b = await readJson(req);
      R.createCave(db, memberId, text(b.id, 64), wrappedList(b.wrapped), now);
      return send(res, 200, { ok: true });
    }
    const m = /^\/api\/caves\/([A-Za-z0-9_-]{8,64})\/(missing|keys|entries|stream|snapshot)$/.exec(path);
    if (!m) return send(res, 404, { error: 'not-found' });
    const [, cave, what] = m;
    if (what === 'missing' && method === 'GET') return send(res, 200, { devices: R.devicesMissingKey(db, memberId, cave) });
    if (what === 'keys' && method === 'POST') {
      const b = await readJson(req);
      R.putWrappedKeys(db, memberId, cave, int(b.version), wrappedList(b.wrapped));
      return send(res, 200, { ok: true });
    }
    if (what === 'entries' && method === 'POST') {
      const b = await readJson(req, 2 * 1024 * 1024);
      const version = int(b.version);
      const locked = text(b.locked, 2 * 1024 * 1024);
      const number = R.appendEntry(db, memberId, device, cave, version, locked, now);
      broadcast(cave, { t: 'entry', number, keyVersion: version, locked, authorDevice: device });
      return send(res, 200, { number });
    }
    if (what === 'snapshot' && method === 'PUT') {
      const b = await readJson(req, 64 * 1024 * 1024);
      R.putSnapshot(db, memberId, cave, int(b.upTo), int(b.version), text(b.locked, 64 * 1024 * 1024), now);
      return send(res, 200, { ok: true });
    }
    if (what === 'snapshot' && method === 'GET') return send(res, 200, { snapshot: R.latestSnapshot(db, memberId, cave) });
    if (what === 'stream' && method === 'GET') return stream(db, memberId, cave, Number(url.searchParams.get('after') ?? 0) || 0, req, res);
    return send(res, 404, { error: 'not-found' });
  } catch (e) {
    const code = e instanceof Error ? e.message : 'error';
    if (STATUS[code]) return send(res, STATUS[code], { error: code });
    if (e instanceof SyntaxError) return send(res, 400, { error: 'bad-request' });
    throw e;
  }
}

/** Sends every entry after `after`, then new ones as they arrive, one JSON line each, until the client goes. */
function stream(db: Db, memberId: string, cave: string, after: number, req: IncomingMessage, res: ServerResponse): void {
  R.entriesAfter(db, memberId, cave, after, 1); // checks the member may read this cave before anything is sent
  res.writeHead(200, { 'Content-Type': 'application/x-ndjson', 'Cache-Control': 'no-store', 'X-Accel-Buffering': 'no' });
  let last = after;
  for (;;) {
    const batch = R.entriesAfter(db, memberId, cave, last, BATCH);
    for (const e of batch) res.write(JSON.stringify({ t: 'entry', number: e.number, keyVersion: e.keyVersion, locked: e.locked, authorDevice: e.authorDevice }) + '\n');
    if (batch.length < BATCH) break;
    last = batch[batch.length - 1].number;
  }
  res.write(JSON.stringify({ t: 'live' }) + '\n');
  const stop = listen(cave, res);
  const ping = setInterval(() => res.write('{"t":"ping"}\n'), PING_MS);
  const end = () => {
    clearInterval(ping);
    stop();
  };
  req.on('close', end);
  res.on('close', end);
}
```

Note: catch-up and `listen` run in the same synchronous turn (no `await` between the last `entriesAfter` and `listen`), and `appendEntry` + `broadcast` are also synchronous, so no entry can slip between catch-up and live.

- [ ] **Step 3: Route it in `apps/server/src/api.ts`** — add `import { handleRooms } from './rooms-api.ts';` and, right after the line `if (r.status !== 'member') return ...` and `const me = r.member;` (inside the try that follows), add as the first route:

```ts
    if (path === '/api/devices' || path === '/api/caves' || path.startsWith('/api/caves/')) return handleRooms(d.db, me.id, req, res, now);
```

and add `X-Cave-Device` to the desktop CORS allow list: `'Authorization, Content-Type, X-Cave-Signup, X-Cave-Device'`.

- [ ] **Step 4: Write the test** `apps/server/tests/rooms-api.test.ts`

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { handleApi, type ApiDeps } from '../src/api.ts';
import { listeners } from '../src/streams.ts';

const ADMIN = 'nathan@example.com';
const PUB = (n: number) => 'P'.repeat(30) + n;

async function start() {
  const db = openDb(':memory:');
  const deps: ApiDeps = {
    db, adminEmail: ADMIN, passKey: generateKeyPairSync('ed25519').privateKey, publicUrl: 'http://127.0.0.1', mailer: { url: '', key: '' },
    whoIs: async (r) => { const e = r.headers['x-test-user']; return e ? { email: String(e), name: 'n', provider: null, subject: '' } : null; },
  };
  const server = createServer((req, res) => void handleApi(deps, req, res));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const call = async (user: string, method: string, path: string, body?: unknown, device = '') => {
    const res = await fetch(base + path, { method, headers: { 'x-test-user': user, 'Content-Type': 'application/json', ...(device ? { 'X-Cave-Device': device } : {}) }, body: body === undefined ? undefined : JSON.stringify(body) });
    return { status: res.status, json: await res.json() };
  };
  return { server, base, call };
}

async function readLines(res: Response, count: number): Promise<Record<string, unknown>[]> {
  const out: Record<string, unknown>[] = [];
  const reader = res.body!.getReader();
  let buf = '';
  while (out.length < count) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += new TextDecoder().decode(value);
    let i;
    while ((i = buf.indexOf('\n')) >= 0 && out.length < count) {
      out.push(JSON.parse(buf.slice(0, i)));
      buf = buf.slice(i + 1);
    }
  }
  await reader.cancel();
  return out;
}

test('a device makes a cave, posts entries, and a stream catches up then goes live', async () => {
  const { server, base, call } = await start();
  try {
    const laptop = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    const tablet = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(2) })).json.device.id;
    assert.equal((await call(ADMIN, 'GET', '/api/caves')).json.error, 'bad-device');
    assert.equal((await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0001', wrapped: [{ device: laptop, wrapped: 'W1' }] }, laptop)).status, 200);
    assert.equal((await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0001', wrapped: [] }, laptop)).status, 409);
    assert.deepEqual((await call(ADMIN, 'GET', '/api/caves/cave-0001/missing', undefined, laptop)).json.devices.map((d: { id: string }) => d.id), [tablet]);
    await call(ADMIN, 'POST', '/api/caves/cave-0001/keys', { version: 1, wrapped: [{ device: tablet, wrapped: 'W2' }] }, laptop);
    assert.equal((await call(ADMIN, 'GET', '/api/caves', undefined, tablet)).json.caves[0].wrapped, 'W2');

    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0001/entries', { version: 1, locked: 'AAAA' }, laptop)).json.number, 1);
    const res = await fetch(base + '/api/caves/cave-0001/stream?after=0', { headers: { 'x-test-user': ADMIN, 'X-Cave-Device': tablet } });
    assert.equal(res.headers.get('content-type'), 'application/x-ndjson');
    const linesP = readLines(res, 3);
    await new Promise((r) => setTimeout(r, 50));
    assert.equal(listeners('cave-0001'), 1);
    await call(ADMIN, 'POST', '/api/caves/cave-0001/entries', { version: 1, locked: 'BBBB' }, laptop);
    const lines = await linesP;
    assert.deepEqual(lines.map((l) => [l.t, l.number, l.locked]), [['entry', 1, 'AAAA'], ['live', undefined, undefined], ['entry', 2, 'BBBB']]);
    assert.equal(lines[2].authorDevice, laptop);
  } finally {
    server.close();
  }
});

test('snapshots, and someone else is refused', async () => {
  const { server, call } = await start();
  try {
    const d = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0002', wrapped: [{ device: d, wrapped: 'W' }] }, d);
    await call(ADMIN, 'PUT', '/api/caves/cave-0002/snapshot', { upTo: 4, version: 1, locked: 'SNAP' }, d);
    assert.deepEqual((await call(ADMIN, 'GET', '/api/caves/cave-0002/snapshot', undefined, d)).json.snapshot, { upTo: 4, keyVersion: 1, locked: 'SNAP' });
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0002/entries', { version: 1, locked: 'not base64!' }, d)).status, 400);
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0002/entries', { version: 1, locked: 'AAAA' }, 'someone-elses-device')).status, 400);
  } finally {
    server.close();
  }
});
```

- [ ] **Step 5: Run** `cd apps/server && node --test` → `# fail 0`

- [ ] **Step 6: Commit**

```bash
git add apps/server/src/streams.ts apps/server/src/rooms-api.ts apps/server/src/api.ts apps/server/tests/rooms-api.test.ts
git commit -m "Server: rooms API for the Private room, with a live stream of locked entries"
```

---

### Task 4: The cave's room id inside the cave

**Goal:** A cave carries its room id as a Setting (`room-id`), so every device and every copy of the file knows which room it belongs to.

**Files:**
- Modify: `crates/cave-draw/src/lib.rs` (next to `drawing_scale` / `set_drawing_scale`, around line 426)
- Modify: `crates/cave-wasm/src/lib.rs` (next to `drawing_scale` / `set_drawing_scale`, around line 558)
- Modify: `apps/web/src/lib/engine.ts` (imports list and the object, next to `drawingScale`), `apps/web/src/lib/readonly.ts` (classify `setRoomId` as a write, `roomId` as a read)

**Acceptance Criteria:**
- [ ] `cave_draw::room_id(doc)` returns `""` for a cave with none, and the id after `set_room_id`.
- [ ] `set_room_id` refuses an id that is not 8-64 of `[A-Za-z0-9_-]` and returns no actions when the id is unchanged.
- [ ] The id survives a native save/load (it is an ordinary entity).
- [ ] `engine.roomId()` / `engine.setRoomId(id)` exist; the readonly classification test passes.

**Verify:** `cargo test -p cave-draw room_id` → `test result: ok`; `cd apps/web && npm test` → all suites ok

**Steps:**

- [ ] **Step 1: Read the existing pair first.** Open `crates/cave-draw/src/lib.rs` at `pub fn drawing_scale` and `pub fn set_drawing_scale` and copy their shape exactly (they use `setting_id("drawing-scale")`, `Field::SettingValue`, `Action::Create`/`Action::Set`). Write the test in the same file's test module:

```rust
#[test]
fn room_id_round_trips() {
    let mut doc = Document::new(cave_id::Id::from_name("actor:test"));
    assert_eq!(room_id(&doc), "");
    let actions = set_room_id(&doc, "cave_abc12345").unwrap();
    doc.edit(actions).unwrap();
    assert_eq!(room_id(&doc), "cave_abc12345");
    assert!(set_room_id(&doc, "cave_abc12345").unwrap().is_empty());
    assert!(set_room_id(&doc, "bad id!").is_err());
    assert!(set_room_id(&doc, "short").is_err());
}
```

If `Document::new` takes a different argument in this crate, use whatever the `drawing_scale` tests in the same file use to make a document.

- [ ] **Step 2: Run, expect failure** — `cargo test -p cave-draw room_id` → `cannot find function room_id`

- [ ] **Step 3: Implement** beside `drawing_scale`:

```rust
/// The id of the cave's room on the CaveCAD server, or `""` when it has none yet.
pub fn room_id(doc: &Document) -> String {
    doc.entity(setting_id("room-id"))
        .map(|e| e.text(Field::SettingValue).trim().to_string())
        .unwrap_or_default()
}

/// Actions that set the cave's room id (8-64 of letters, digits, `_` and `-`); none when it is already that.
pub fn set_room_id(doc: &Document, id: &str) -> Result<Vec<Action>, DrawError> {
    let ok = (8..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !ok {
        return Err(DrawError::BadStroke);
    }
    if room_id(doc) == id {
        return Ok(Vec::new());
    }
    // Same shape as set_drawing_scale: create the setting entity if missing, then set its value.
    let key = setting_id("room-id");
    let mut actions = Vec::new();
    if doc.entity(key).is_none() {
        actions.push(Action::Create { id: key, kind: Kind::Setting });
    }
    actions.push(Action::Set { id: key, field: Field::SettingValue, value: Value::Text(id.to_string()) });
    Ok(actions)
}
```

Match the `Action::Create` / `Action::Set` field names and the `DrawError` variant to what `set_drawing_scale` actually uses (copy its `actions.push(...)` lines and change only the key and value). If `set_drawing_scale` also sets a setting-name field on create, do the same with `"room-id"`.

- [ ] **Step 4: wasm bindings** in `crates/cave-wasm/src/lib.rs`, copying `drawing_scale` / `set_drawing_scale`:

```rust
/// The open cave's room id on the CaveCAD server, or "" when it has none.
#[wasm_bindgen]
pub fn room_id() -> String {
    DOC.with(|d| d.borrow().as_ref().map(cave_draw::room_id).unwrap_or_default())
}

/// Sets the open cave's room id. One edit in the settings history. Returns `ok` or `error: ...`.
#[wasm_bindgen]
pub fn set_room_id(id: &str) -> String {
    let result = DOC.with(|d| {
        let mut slot = d.borrow_mut();
        let doc = slot.as_mut().ok_or("no cave is open")?;
        let actions = cave_draw::set_room_id(doc, id).map_err(|e| format!("{e:?}"))?;
        if actions.is_empty() {
            return Ok(());
        }
        let edit = doc.edit(actions).map_err(|e| format!("{e:?}"))?;
        record_edit("settings", &edit);
        Ok::<(), String>(())
    });
    match result {
        Ok(()) => "ok".into(),
        Err(e) => format!("error: {e}"),
    }
}
```

Match the tail (how `set_drawing_scale` turns its result into a string) to the real function.

- [ ] **Step 5: engine.ts** — add `room_id, set_room_id` to the import list from `'../wasm/cave_wasm.js'`, and beside `drawingScale` in the `engine` object:

```ts
  /** The open cave's room id on the server ('' when it has none). */
  roomId: (): string => room_id(),
  /** Sets the open cave's room id; `ok` or `error: ...`. */
  setRoomId: (id: string): string => set_room_id(id),
```

In `apps/web/src/lib/readonly.ts`, add `'setRoomId'` to `WRITES` and `'roomId'` to the reads list.

- [ ] **Step 6: Run** — `cargo test -p cave-draw room_id` → ok; `cd apps/web && npm test` → all ok (the wasm rebuilds on test via the existing scripts; if a test needs the wasm built, run `npx vite build` once first with rustup on PATH); `python3 tools/make_code_index.py`.

- [ ] **Step 7: Commit**

```bash
git add crates/cave-draw/src/lib.rs crates/cave-wasm/src/lib.rs apps/web/src/lib/engine.ts apps/web/src/lib/readonly.ts docs/CODE-INDEX.md
git commit -m "Cave room id: a setting inside the cave naming its room on the server"
```

---

### Task 5: Locks and keys on the device

**Goal:** A pure WebCrypto module makes device key pairs and cave keys, wraps a cave key for a device's public key, and locks and unlocks entries; nothing in it touches the network or storage.

**Files:**
- Create: `apps/web/src/lib/room-crypto.ts`, `apps/web/tests/roomcrypto.check.ts`
- Modify: `apps/web/package.json` (`test:roomcrypto`, appended to `test`)

**Acceptance Criteria:**
- [ ] `makeDeviceKeys()` gives a non-extractable X25519 private key and its public key as base64url (43 chars).
- [ ] `wrapCaveKey(caveKey, publicKeyB64)` → base64url text; only `unwrapCaveKey(text, privateKey)` of the matching device opens it; another device's private key throws.
- [ ] `lock(caveKey, text)` / `unlock(caveKey, locked)` round-trip UTF-8; a changed byte throws; each lock uses a fresh 12-byte IV (two locks of the same text differ).
- [ ] `exportCaveKeyForLink` is NOT in step 1 (links are step 2).

**Verify:** `cd apps/web && npm run test:roomcrypto` → `roomcrypto: all ok`

**Steps:**

- [ ] **Step 1: Write the failing test** `apps/web/tests/roomcrypto.check.ts`

```ts
// Locks and keys for rooms: device key pairs, wrapped cave keys, locked entries.
import { lock, makeCaveKey, makeDeviceKeys, unlock, unwrapCaveKey, wrapCaveKey } from '../src/lib/room-crypto.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const throws = async (f: () => Promise<unknown>) => { try { await f(); return false; } catch { return true; } };

const laptop = await makeDeviceKeys();
const tablet = await makeDeviceKeys();
ok(/^[A-Za-z0-9_-]{43}$/.test(laptop.publicKey), 'public key is 43 base64url chars');
ok(laptop.privateKey.extractable === false, 'private key cannot be exported');

const cave = await makeCaveKey();
const forTablet = await wrapCaveKey(cave, tablet.publicKey);
const opened = await unwrapCaveKey(forTablet, tablet.privateKey);
const sealed = await lock(opened, 'station A1 to A2: 12.4 m');
ok((await unlock(cave, sealed)) === 'station A1 to A2: 12.4 m', 'the tablet opens what it was given and locks for everyone with the key');
ok(await throws(() => unwrapCaveKey(forTablet, laptop.privateKey)), 'another device cannot open it');

const a = await lock(cave, 'same text');
const b = await lock(cave, 'same text');
ok(a !== b, 'each lock uses a fresh IV');
const flipped = a.slice(0, 20) + (a[20] === 'A' ? 'B' : 'A') + a.slice(21);
ok(await throws(() => unlock(cave, flipped)), 'a changed entry is refused');
ok((await unlock(cave, await lock(cave, 'ünïcödé ✓'))) === 'ünïcödé ✓', 'UTF-8 round trip');

console.log(bad ? `roomcrypto: ${bad} failed` : 'roomcrypto: all ok');
process.exit(bad ? 1 : 0);
```

- [ ] **Step 2: Run, expect failure** — `node --experimental-strip-types tests/roomcrypto.check.ts` → `Cannot find module`

- [ ] **Step 3: Write `apps/web/src/lib/room-crypto.ts`**

```ts
// Locks and keys for rooms, all on this device (WebCrypto). A device has an X25519 key pair; its private half
// never leaves it. A cave has an AES-256-GCM key; it is handed to a device wrapped with that device's public key
// (an ephemeral X25519 exchange, HKDF, AES-GCM), so the server, which stores the wrapped copies, cannot open them.
// Entries are locked with the cave key and a fresh 12-byte IV. Text forms are base64url.

const enc = new TextEncoder();
const dec = new TextDecoder();
const b64 = (b: ArrayBuffer | Uint8Array) => btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const unb64 = (s: string) => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0));
const INFO = enc.encode('cavecad cave key v1');

export type DeviceKeys = { privateKey: CryptoKey; publicKey: string };

/** A new device key pair: the private key cannot be exported; the public key as base64url. */
export async function makeDeviceKeys(): Promise<DeviceKeys> {
  const pair = (await crypto.subtle.generateKey({ name: 'X25519' }, false, ['deriveBits'])) as CryptoKeyPair;
  return { privateKey: pair.privateKey, publicKey: b64(await crypto.subtle.exportKey('raw', pair.publicKey)) };
}

/** A new cave lock key (extractable, so it can be wrapped for other devices). */
export async function makeCaveKey(): Promise<CryptoKey> {
  return crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, true, ['encrypt', 'decrypt']);
}

async function sharedKey(privateKey: CryptoKey, publicRaw: Uint8Array, salt: Uint8Array): Promise<CryptoKey> {
  const pub = await crypto.subtle.importKey('raw', publicRaw, { name: 'X25519' }, false, []);
  const bits = await crypto.subtle.deriveBits({ name: 'X25519', public: pub }, privateKey, 256);
  const hk = await crypto.subtle.importKey('raw', bits, 'HKDF', false, ['deriveKey']);
  return crypto.subtle.deriveKey({ name: 'HKDF', hash: 'SHA-256', salt, info: INFO }, hk, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
}

/** Wraps a cave key for the device whose public key is `publicKeyB64`. Format: ephemeral public (32) | iv (12) | sealed key. */
export async function wrapCaveKey(caveKey: CryptoKey, publicKeyB64: string): Promise<string> {
  const eph = (await crypto.subtle.generateKey({ name: 'X25519' }, true, ['deriveBits'])) as CryptoKeyPair;
  const ephPub = new Uint8Array(await crypto.subtle.exportKey('raw', eph.publicKey));
  const k = await sharedKey(eph.privateKey, unb64(publicKeyB64), ephPub);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const raw = new Uint8Array(await crypto.subtle.exportKey('raw', caveKey));
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, k, raw));
  const out = new Uint8Array(32 + 12 + sealed.length);
  out.set(ephPub, 0);
  out.set(iv, 32);
  out.set(sealed, 44);
  return b64(out);
}

/** Opens a cave key wrapped for this device. Throws when it was wrapped for another device or changed. */
export async function unwrapCaveKey(wrapped: string, privateKey: CryptoKey): Promise<CryptoKey> {
  const bytes = unb64(wrapped);
  const ephPub = bytes.slice(0, 32);
  const k = await sharedKey(privateKey, ephPub, ephPub);
  const raw = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: bytes.slice(32, 44) }, k, bytes.slice(44));
  return crypto.subtle.importKey('raw', raw, { name: 'AES-GCM', length: 256 }, true, ['encrypt', 'decrypt']);
}

/** Locks text with a cave key. Format: iv (12) | ciphertext, base64url. */
export async function lock(caveKey: CryptoKey, text: string): Promise<string> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, caveKey, enc.encode(text)));
  const out = new Uint8Array(12 + sealed.length);
  out.set(iv, 0);
  out.set(sealed, 12);
  return b64(out);
}

/** Unlocks text locked with `lock`. Throws when the key is wrong or the entry was changed. */
export async function unlock(caveKey: CryptoKey, locked: string): Promise<string> {
  const bytes = unb64(locked);
  return dec.decode(await crypto.subtle.decrypt({ name: 'AES-GCM', iv: bytes.slice(0, 12) }, caveKey, bytes.slice(12)));
}
```

Large entries: `String.fromCharCode(...bytes)` overflows the stack above ~100 KB. Replace `b64` with a chunked version before snapshots use it:

```ts
const b64 = (b: ArrayBuffer | Uint8Array) => {
  const u = new Uint8Array(b);
  let s = '';
  for (let i = 0; i < u.length; i += 0x8000) s += String.fromCharCode(...u.subarray(i, i + 0x8000));
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};
```

(use this chunked version from the start) and add a test line: `ok((await unlock(cave, await lock(cave, 'x'.repeat(3_000_000)))).length === 3_000_000, 'a 3 MB entry round-trips');`

- [ ] **Step 4: Script** — in `apps/web/package.json` add `"test:roomcrypto": "node --experimental-strip-types tests/roomcrypto.check.ts",` and append ` && npm run test:roomcrypto` to `test`.

- [ ] **Step 5: Run** → `roomcrypto: all ok`

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/lib/room-crypto.ts apps/web/tests/roomcrypto.check.ts apps/web/package.json
git commit -m "Room locks: device key pairs, wrapped cave keys and locked entries (WebCrypto)"
```

---

### Task 6: The sync client (pure)

**Goal:** `room-client.ts` keeps one cave in step with its Private room through the server, against an injected engine and fetch, so node can run two devices against the real server.

**Files:**
- Create: `apps/web/src/lib/room-client.ts`, `apps/web/tests/roomclient.check.ts`
- Modify: `apps/web/package.json` (`test:roomclient`, appended to `test`)

**Acceptance Criteria:**
- [ ] `RoomClient.open()` on a cave with no room id makes a cave key, creates the cave on the server wrapped for all of this member's registered devices, sets the room id in the cave, and uploads a locked snapshot.
- [ ] On a cave with a room id and a wrapped key for this device: catches up from the last applied number (snapshot first if it is ahead), then applies live entries; skips entries whose `authorDevice` is this device.
- [ ] Local edits (engine outbox) are locked and posted every `flushMs`; while offline they are kept in order and sent on reconnect.
- [ ] A device with no wrapped key yet reports status `waiting-key` and retries every 30 s; any of the member's devices that has the key hands it to devices missing it when it opens the cave.
- [ ] Status is one of `connecting`, `synced`, `offline`, `waiting-key`, `error`, delivered to an `onStatus` callback.
- [ ] Two clients (laptop, tablet) on the real server in one process: an edit on one reaches the other within 2 s, both ways; a client stopped while the other edits catches up when restarted.

**Verify:** `cd apps/web && npm run test:roomclient` → `roomclient: all ok`

**Steps:**

- [ ] **Step 1: Write `apps/web/src/lib/room-client.ts`**

```ts
// Keeps one open cave in step with its room on the CaveCAD server (step 1: the Private room, the owner's own
// devices). Edits are locked here and posted; the server numbers them and passes them to the cave's other open
// streams. Pure: the engine, fetch and the device's keys are passed in, so node tests run it against the real server.
import { lock, makeCaveKey, unlock, unwrapCaveKey, wrapCaveKey } from './room-crypto.ts';

export type RoomStatus = 'connecting' | 'synced' | 'offline' | 'waiting-key' | 'error';
/** What the client needs from the cave engine (engine.ts provides it in the app; tests fake it). */
export type CaveEngine = { roomId(): string; setRoomId(id: string): string; outbox(): string; apply(ops: string): number; snapshot(): string; load(ops: string): string };
export type RoomClientOptions = {
  base: string;
  fetch: typeof fetch;
  engine: CaveEngine;
  device: { id: string; privateKey: CryptoKey };
  /** Extra headers (the desktop bearer); the device header is added here. */
  headers?: () => Record<string, string>;
  /** Last applied entry number for a cave, kept by the caller (IndexedDB in the app). */
  loadLast: (cave: string) => Promise<number>;
  saveLast: (cave: string, n: number) => Promise<void>;
  onStatus: (s: RoomStatus) => void;
  flushMs?: number;
  snapshotEvery?: number;
};

const newCaveId = () => 'cave_' + Array.from(crypto.getRandomValues(new Uint8Array(12)), (b) => b.toString(16).padStart(2, '0')).join('');

export class RoomClient {
  private o: RoomClientOptions;
  private cave = '';
  private key: CryptoKey | null = null;
  private version = 1;
  private last = 0;
  private sinceSnapshot = 0;
  private pending: string[] = [];
  private stopped = false;
  private abort: AbortController | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(o: RoomClientOptions) {
    this.o = o;
  }

  private headers(): Record<string, string> {
    return { 'Content-Type': 'application/json', 'X-Cave-Device': this.o.device.id, ...(this.o.headers?.() ?? {}) };
  }
  private async call<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await this.o.fetch(this.o.base + path, { method, headers: this.headers(), body: body === undefined ? undefined : JSON.stringify(body), credentials: 'same-origin' });
    const out = (await res.json()) as T & { error?: string };
    if (!res.ok) throw new Error(out.error ?? 'error');
    return out;
  }

  /** Starts keeping the open cave in step. Safe to call once per opened cave; call `stop()` before opening another. */
  async open(): Promise<void> {
    this.o.onStatus('connecting');
    try {
      this.cave = this.o.engine.roomId();
      if (!this.cave) await this.createRoom();
      else if (!(await this.loadKey())) return this.waitForKey();
      await this.handKeysToMyOtherDevices();
      this.last = await this.o.loadLast(this.cave);
      this.timer = setInterval(() => void this.flush(), this.o.flushMs ?? 300);
      void this.listen();
    } catch {
      this.o.onStatus('offline');
      if (!this.stopped) setTimeout(() => void this.open(), 15_000);
    }
  }

  /** Stops syncing (the cave is closing). */
  stop(): void {
    this.stopped = true;
    this.abort?.abort();
    if (this.timer) clearInterval(this.timer);
  }

  private async createRoom(): Promise<void> {
    const id = newCaveId();
    const key = await makeCaveKey();
    const { devices } = await this.call<{ devices: { id: string; publicKey: string }[] }>('GET', '/api/devices');
    const wrapped = await Promise.all(devices.map(async (d) => ({ device: d.id, wrapped: await wrapCaveKey(key, d.publicKey) })));
    await this.call('POST', '/api/caves', { id, wrapped });
    const r = this.o.engine.setRoomId(id);
    if (r !== 'ok') throw new Error(r);
    this.cave = id;
    this.key = key;
    this.version = 1;
    this.o.engine.outbox(); // the snapshot below carries everything made so far
    await this.call('PUT', `/api/caves/${id}/snapshot`, { upTo: 0, version: 1, locked: await lock(key, this.o.engine.snapshot()) });
  }

  private async loadKey(): Promise<boolean> {
    const { caves } = await this.call<{ caves: { id: string; keyVersion: number; wrapped: string | null }[] }>('GET', '/api/caves');
    const mine = caves.find((c) => c.id === this.cave);
    if (!mine || !mine.wrapped) return false;
    this.key = await unwrapCaveKey(mine.wrapped, this.o.device.privateKey);
    this.version = mine.keyVersion;
    return true;
  }

  private waitForKey(): void {
    this.o.onStatus('waiting-key');
    if (!this.stopped) setTimeout(() => void this.open(), 30_000);
  }

  private async handKeysToMyOtherDevices(): Promise<void> {
    const { devices } = await this.call<{ devices: { id: string; publicKey: string }[] }>('GET', `/api/caves/${this.cave}/missing`);
    if (!devices.length) return;
    const wrapped = await Promise.all(devices.map(async (d) => ({ device: d.id, wrapped: await wrapCaveKey(this.key!, d.publicKey) })));
    await this.call('POST', `/api/caves/${this.cave}/keys`, { version: this.version, wrapped });
  }

  private async catchUpFromSnapshot(): Promise<void> {
    const { snapshot } = await this.call<{ snapshot: { upTo: number; keyVersion: number; locked: string } | null }>('GET', `/api/caves/${this.cave}/snapshot`);
    if (snapshot && snapshot.upTo > this.last) {
      this.o.engine.apply(await unlock(this.key!, snapshot.locked));
      this.last = snapshot.upTo;
      await this.o.saveLast(this.cave, this.last);
    }
  }

  private async listen(): Promise<void> {
    while (!this.stopped) {
      try {
        await this.catchUpFromSnapshot();
        this.abort = new AbortController();
        const res = await this.o.fetch(`${this.o.base}/api/caves/${this.cave}/stream?after=${this.last}`, { headers: this.headers(), credentials: 'same-origin', signal: this.abort.signal });
        if (!res.ok || !res.body) throw new Error('stream');
        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buf = '';
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          buf += decoder.decode(value, { stream: true });
          let i;
          while ((i = buf.indexOf('\n')) >= 0) {
            const line = JSON.parse(buf.slice(0, i)) as { t: string; number?: number; locked?: string; authorDevice?: string };
            buf = buf.slice(i + 1);
            await this.onLine(line);
          }
        }
      } catch {
        if (this.stopped) return;
      }
      this.o.onStatus('offline');
      await new Promise((r) => setTimeout(r, 3_000));
    }
  }

  private async onLine(line: { t: string; number?: number; locked?: string; authorDevice?: string }): Promise<void> {
    if (line.t === 'live') {
      this.o.onStatus(this.pending.length ? 'offline' : 'synced');
      void this.flush();
      return;
    }
    if (line.t !== 'entry' || !line.number || line.number <= this.last) return;
    if (line.authorDevice !== this.o.device.id) this.o.engine.apply(await unlock(this.key!, line.locked!));
    this.last = line.number;
    this.sinceSnapshot++;
    await this.o.saveLast(this.cave, this.last);
    if (this.sinceSnapshot >= (this.o.snapshotEvery ?? 500)) {
      this.sinceSnapshot = 0;
      void this.call('PUT', `/api/caves/${this.cave}/snapshot`, { upTo: this.last, version: this.version, locked: await lock(this.key!, this.o.engine.snapshot()) }).catch(() => {});
    }
  }

  /** Locks and posts local edits; keeps them in order while offline. */
  async flush(): Promise<void> {
    if (!this.key) return;
    const ops = this.o.engine.outbox();
    if (ops && ops !== '[]') this.pending.push(await lock(this.key, ops));
    while (this.pending.length) {
      try {
        await this.call('POST', `/api/caves/${this.cave}/entries`, { version: this.version, locked: this.pending[0] });
        this.pending.shift();
      } catch {
        this.o.onStatus('offline');
        return;
      }
    }
  }
}
```

The client needs `GET /api/devices` (the member's devices with public keys). Add to `rooms.ts`:

```ts
/** The member's devices, with public keys (to wrap a new cave's key for each). */
export function myDevices(db: Db, memberId: string): { id: string; publicKey: string }[] {
  return (db.prepare('SELECT id, public_key FROM devices WHERE member_id = ? ORDER BY created_at').all(memberId) as { id: string; public_key: string }[]).map((r) => ({ id: r.id, publicKey: r.public_key }));
}
```

and to `rooms-api.ts`, before the `if (!device)` line: `if (path === '/api/devices' && method === 'GET') return send(res, 200, { devices: R.myDevices(db, memberId) });`. Add a server test line in `rooms-api.test.ts`: after registering two devices, `GET /api/devices` lists both public keys.

- [ ] **Step 2: Write the two-device test** `apps/web/tests/roomclient.check.ts`

```ts
// Two devices of one member keep a cave in step through the real server code, in this process.
import { createServer } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { openDb } from '../../server/src/db.ts';
import { handleApi } from '../../server/src/api.ts';
import { makeDeviceKeys } from '../src/lib/room-crypto.ts';
import { RoomClient, type CaveEngine, type RoomStatus } from '../src/lib/room-client.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const until = async (f: () => boolean, ms = 3000) => { const t = Date.now(); while (!f() && Date.now() - t < ms) await new Promise((r) => setTimeout(r, 20)); return f(); };

/** A fake cave: a set of op strings; outbox gives what was added locally since last time. */
function fakeEngine(): CaveEngine & { add(op: string): void; ops: Set<string> } {
  let room = '';
  const ops = new Set<string>();
  let out: string[] = [];
  return {
    ops,
    add(op) { ops.add(op); out.push(op); },
    roomId: () => room,
    setRoomId: (id) => ((room = id), 'ok'),
    outbox: () => { const o = JSON.stringify(out); out = []; return o; },
    apply: (s) => { const a = JSON.parse(s) as string[]; a.forEach((x) => ops.add(x)); return a.length; },
    snapshot: () => JSON.stringify([...ops]),
    load: (s) => { ops.clear(); (JSON.parse(s) as string[]).forEach((x) => ops.add(x)); return 'ok'; },
  };
}

const db = openDb(':memory:');
const server = createServer((req, res) => void handleApi({
  db, adminEmail: 'nathan@example.com', passKey: generateKeyPairSync('ed25519').privateKey, publicUrl: 'http://127.0.0.1', mailer: { url: '', key: '' },
  whoIs: async () => ({ email: 'nathan@example.com', name: 'Nathan', provider: null, subject: '' }),
}, req, res));
await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;

async function device() {
  const keys = await makeDeviceKeys();
  const res = await fetch(base + '/api/devices', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ publicKey: keys.publicKey }) });
  return { id: (await res.json()).device.id as string, privateKey: keys.privateKey };
}
function client(engine: CaveEngine, dev: { id: string; privateKey: CryptoKey }, status: RoomStatus[]) {
  const last = new Map<string, number>();
  return new RoomClient({ base, fetch, engine, device: dev, flushMs: 50, loadLast: async (c) => last.get(c) ?? 0, saveLast: async (c, n) => void last.set(c, n), onStatus: (s) => status.push(s) });
}

const laptopDev = await device();
const tabletDev = await device();
const laptop = fakeEngine();
laptop.add('trip 1');
const ls: RoomStatus[] = [];
const lc = client(laptop, laptopDev, ls);
await lc.open();
ok(laptop.roomId().startsWith('cave_'), 'the laptop made the room and set its id in the cave');
ok(await until(() => ls.includes('synced')), 'laptop is synced');

const tablet = fakeEngine();
tablet.setRoomId(laptop.roomId()); // the same cave file opened on the tablet
const ts: RoomStatus[] = [];
const tc = client(tablet, tabletDev, ts);
await tc.open();
ok(await until(() => tablet.ops.has('trip 1')), 'the tablet gets the cave from the snapshot');

laptop.add('shot A1-A2');
ok(await until(() => tablet.ops.has('shot A1-A2')), 'a laptop edit reaches the tablet');
tablet.add('wall sketch');
ok(await until(() => laptop.ops.has('wall sketch')), 'a tablet edit reaches the laptop');

tc.stop();
laptop.add('while the tablet was away');
await new Promise((r) => setTimeout(r, 200));
const tc2 = client(tablet, tabletDev, []);
await tc2.open();
ok(await until(() => tablet.ops.has('while the tablet was away')), 'the tablet catches up when it comes back');

const stored = db.prepare('SELECT locked FROM history').all() as { locked: string }[];
ok(stored.length > 0 && stored.every((r) => !r.locked.includes('shot') && !r.locked.includes('trip')), 'the server stores nothing readable');

lc.stop();
tc2.stop();
server.close();
console.log(bad ? `roomclient: ${bad} failed` : 'roomclient: all ok');
process.exit(bad ? 1 : 0);
```

Note the tablet was registered before the laptop made the room, so it received a wrapped key at creation. Add one more case after the catch-up check: register a third device (`phone`) only now, open a client on it (expect status `waiting-key`), then reopen the laptop client (its `open()` hands keys to devices missing them), then reopen the phone client and expect it to reach the cave's ops.

- [ ] **Step 3: Script** — `"test:roomclient": "node --experimental-strip-types tests/roomclient.check.ts",` and append to `test`.

- [ ] **Step 4: Run** → `roomclient: all ok`; `cd apps/server && node --test` → `# fail 0`

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/room-client.ts apps/web/tests/roomclient.check.ts apps/web/package.json apps/server/src/rooms.ts apps/server/src/rooms-api.ts apps/server/tests/rooms-api.test.ts
git commit -m "Room client: keeps a cave in step with its Private room; two devices tested against the real server"
```

---

### Task 7: The device's key and the app wiring

**Goal:** In the app, each signed-in device has its key pair kept in IndexedDB and registered with the server, and every opened cave is kept in step through its Private room with a small status indicator.

**Files:**
- Create: `apps/web/src/lib/device-key.ts`, `apps/web/src/lib/room.svelte.ts`, `apps/web/src/components/RoomStatus.svelte`
- Modify: `apps/web/src/App.svelte` (start rooms after the account is ready; place `<RoomStatus />` beside the user icon), `apps/web/src/lib/cave.svelte.ts` or wherever a cave is opened/closed (call `openRoom()` / `closeRoom()`)

**Acceptance Criteria:**
- [ ] `deviceKey()` returns the same key pair across reloads (private key stored as a non-extractable `CryptoKey` in IndexedDB `cavecad-device`), registering the public key with `POST /api/devices` once and keeping the device id.
- [ ] Opening a cave (shelf, file, import, demo excluded) starts a `RoomClient` for it; opening another cave stops the previous one.
- [ ] Demo caves and classroom student copies never get a room (no `setRoomId` on them).
- [ ] `RoomStatus.svelte` shows a small text label: `Synced`, `Offline: your edits will join when you reconnect`, `Waiting for your other device to share this cave`, or nothing when no cave is open; it uses the app's existing CSS variables.
- [ ] The live-session (peer-to-peer) code keeps working unchanged in this step.
- [ ] Last applied entry numbers are kept per cave in IndexedDB.

**Verify:** `cd apps/web && npm test && npm run check` → all suites ok, no new errors. Then in the browser (dev server with the local API server, see Task 8): open a cave, the label reaches `Synced`; reload, it stays `Synced` with the same room id.

**Steps:**

- [ ] **Step 1: Find where caves open.** Read `apps/web/src/lib/caves.ts`, `files.ts` and `shelf.svelte.ts` and list every place that calls `engine.syncLoad`, `engine.openNative`, `engine.openDxf`, `engine.importSurvey`, `engine.importFolder` or `engine.newCave` to replace the open cave. Each of those (except `openDemo` and the classroom wipe in `classroom.svelte.ts`) must call `openRoom()` after the cave is in place. Prefer one choke point: if `cave.opened` (in `cave.svelte.ts`) is incremented on every open, add an `$effect` in `room.svelte.ts` reacting to it instead of touching each caller.

- [ ] **Step 2: Write `apps/web/src/lib/device-key.ts`**

```ts
// This device's room key pair. Made once, the private half kept as a non-extractable CryptoKey in IndexedDB
// (it never leaves the device), the public half registered with the server, which gives back a device id.
import { makeDeviceKeys } from './room-crypto';

type Stored = { privateKey: CryptoKey; publicKey: string; deviceId: string };
const DB = 'cavecad-device';

function idb<T>(mode: IDBTransactionMode, f: (s: IDBObjectStore) => IDBRequest<T>): Promise<T | undefined> {
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
      try {
        const q = f(r.result.transaction('kv', mode).objectStore('kv'));
        q.onsuccess = () => (resolve(q.result), r.result.close());
        q.onerror = () => (resolve(undefined), r.result.close());
      } catch {
        resolve(undefined);
        r.result.close();
      }
    };
  });
}

let cached: Promise<{ id: string; privateKey: CryptoKey } | null> | null = null;

/** This device's id and private key, registering it the first time. Null when the server can't be reached yet. */
export function deviceKey(base: string, headers: () => Record<string, string>): Promise<{ id: string; privateKey: CryptoKey } | null> {
  return (cached ??= (async () => {
    let s = await idb<Stored>('readonly', (st) => st.get('keys'));
    if (!s) {
      const k = await makeDeviceKeys();
      s = { privateKey: k.privateKey, publicKey: k.publicKey, deviceId: '' };
    }
    if (!s.deviceId) {
      try {
        const res = await fetch(base + '/api/devices', { method: 'POST', headers: { 'Content-Type': 'application/json', ...headers() }, body: JSON.stringify({ publicKey: s.publicKey }), credentials: 'same-origin' });
        if (!res.ok) throw new Error();
        s.deviceId = (await res.json()).device.id;
      } catch {
        await idb('readwrite', (st) => st.put(s, 'keys'));
        cached = null;
        return null;
      }
    }
    await idb('readwrite', (st) => st.put(s, 'keys'));
    return { id: s.deviceId, privateKey: s.privateKey };
  })());
}

/** Last applied entry number per cave. */
export const lastNumber = {
  load: async (cave: string) => (await idb<number>('readonly', (st) => st.get('last:' + cave))) ?? 0,
  save: async (cave: string, n: number) => void (await idb('readwrite', (st) => st.put(n, 'last:' + cave))),
};
```

- [ ] **Step 3: Write `apps/web/src/lib/room.svelte.ts`**

```ts
// Keeps the open cave in step with its Private room on the server (rooms step 1). One RoomClient per open cave.
import { engine } from './engine';
import { account } from './account.svelte';
import { desktopBearer } from './desktop-signin';
import { deviceKey, lastNumber } from './device-key';
import { RoomClient, type RoomStatus } from './room-client';

export const room = $state({ status: '' as RoomStatus | '' });

const BASE = (import.meta.env.VITE_ACCOUNT_BASE as string | undefined) ?? '';
const headers = (): Record<string, string> => {
  const t = desktopBearer();
  return t ? { Authorization: 'Bearer ' + t } : {};
};
let current: RoomClient | null = null;

/** Starts syncing the cave now open. Call after any cave (not a demo, not a class copy) is put in place. */
export async function openRoom(): Promise<void> {
  closeRoom();
  if (!account.member) return;
  const dev = await deviceKey(BASE, headers);
  if (!dev) {
    room.status = 'offline';
    return;
  }
  current = new RoomClient({
    base: BASE,
    fetch: (...a) => fetch(...a),
    engine: { roomId: engine.roomId, setRoomId: engine.setRoomId, outbox: engine.syncOutbox, apply: engine.syncApply, snapshot: engine.syncSnapshot, load: engine.syncLoad },
    device: dev,
    headers,
    loadLast: lastNumber.load,
    saveLast: lastNumber.save,
    onStatus: (s) => (room.status = s),
  });
  await current.open();
}

/** Stops syncing (the cave is closing or another is opening). */
export function closeRoom(): void {
  current?.stop();
  current = null;
  room.status = '';
}
```

Important: the peer-to-peer session also drains `engine.syncOutbox`. While a peer-to-peer session is live (`p2p.role !== 'none'` or `live.status !== 'off'`), call `closeRoom()` and do not open one, so the two never compete for the same outbox. Add that check at the top of `openRoom()` and reopen the room when the session ends (where `leaveP2p` / `leave` finish).

- [ ] **Step 4: Write `apps/web/src/components/RoomStatus.svelte`**

```svelte
<script lang="ts">
  // A small label beside the user icon saying whether the open cave is in step with the server.
  import { room } from '../lib/room.svelte';
  const TEXT: Record<string, string> = {
    connecting: 'Connecting…',
    synced: 'Synced',
    offline: 'Offline: your edits will join when you reconnect',
    'waiting-key': 'Waiting for your other device to share this cave',
    error: "Couldn't reach the server",
  };
</script>

{#if room.status}
  <span class="room-status" class:quiet={room.status === 'synced'} role="status">{TEXT[room.status]}</span>
{/if}

<style>
  .room-status { font-size: 12px; color: var(--muted); padding: 0 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 320px; }
  .room-status:not(.quiet) { color: var(--text); }
</style>
```

- [ ] **Step 5: Wire it** — in `App.svelte`, render `<RoomStatus />` immediately before `<UserMenu />` in the tab bar; call `openRoom()` from the choke point found in Step 1 (only when `account.phase === 'ready' || account.phase === 'offline'`).

- [ ] **Step 6: Run** `cd apps/web && npm test && npm run check` → all ok, no new errors.

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/lib/device-key.ts apps/web/src/lib/room.svelte.ts apps/web/src/components/RoomStatus.svelte apps/web/src/App.svelte apps/web/src/lib/*.ts
git commit -m "Rooms in the app: this device's key, the open cave kept in step through its Private room, status label"
```

---

### Task 8: Check it in a browser, two windows

**Goal:** The real app keeps one cave in step between two browser windows signed in as the same member, through a locally running server.

**Files:**
- Modify: `~/Documents/Claude/.claude/launch.json` (add `rooms-server` on :8788 and `rooms-web` on :5185 for this worktree, like `accounts-server`/`accounts-web`)
- Modify: `docs/DESIGN.md` (dated section "2026-10-25 — Rooms step 1: the Private room")

**Acceptance Criteria:**
- [ ] Two windows (the second a private window, so it has its own IndexedDB and device key) open the same cave from the same file: an edit in one appears in the other within 2 s.
- [ ] Stopping the server shows `Offline: …`; an edit made then appears in the other window after the server restarts.
- [ ] The SQLite `history` table contains only base64 text (spot-check with `sqlite3 dev.sqlite 'select substr(locked,1,40) from history limit 3'`).
- [ ] DESIGN.md records what was built, what was checked, and what is next (step 2 plan).

**Verify:** screenshots of both windows showing the same edit and `Synced`; the sqlite query output.

**Steps:**

- [ ] **Step 1: Launch entries** — copy the `accounts-server` and `accounts-web` entries with names `rooms-server` / `rooms-web`, paths under `cavecad-rs-rooms`, server `PORT=8788`, web `--port 5185`, and in the web dev server's proxy point `/api` at `http://localhost:8788` (set `VITE_API_PROXY` or edit `vite.config.ts` to read the port from an env var `CAVECAD_API_PORT`, default 8787, and set it to 8788 in the launch entry). `PUBLIC_URL=http://localhost:5185`, `DEV_IDENTITY=nathan@example.com`, `DB_PATH=dev.sqlite`, `PASS_KEY_FILE=pass-key.json` (make it with `node tools/make-pass-key.mjs apps/server/pass-key.json` if missing).

- [ ] **Step 2: Two windows** — start both, open http://localhost:5185 in the browser pane, open a demo cave and **Save as** to make it a real cave (demos get no room), confirm the label reaches `Synced`; export it (`.cave` file) and open the same file in a second tab of a private window (or a second browser), confirm `Synced`, draw a line in one, see it in the other.

- [ ] **Step 3: Offline** — stop `rooms-server`, see the label change, draw in window A, start the server, see it arrive in window B.

- [ ] **Step 4: Inspect storage** — run the sqlite query; every value must look like random base64.

- [ ] **Step 5: DESIGN.md** — append a short dated section in the file's style: what step 1 is, the routes, the crypto (X25519 wrap, AES-GCM entries), the stream format, what was checked (tests + this browser run), and that peer-to-peer still runs alongside until step 6.

- [ ] **Step 6: Commit**

```bash
git add docs/DESIGN.md apps/web/vite.config.ts
git commit -m "Rooms step 1: checked in two browser windows; design record"
```

---

## Not in this plan (later plans, in the spec's build order)

2. Extra rooms, seats, links, waiting room. 3. Presence through the server. 4. Removal, lock changes, Everyone with access, handing over ownership. 5. Newcomers through room links waiting for admin approval. 6. Retiring peer-to-peer from the screens. Deploying step 1 to CT 209 follows `deploy/README.md` "Updating the web app" and a server restart, with Nathan's go-ahead.

## Self-review notes

- Spec step 1 covered: device key pairs (Task 5, 7), cave lock keys wrapped per device (5, 6), Private room per cave (2), numbered locked history and snapshots (2, 3), catch-up by number and live delivery (3, 6), own devices kept in step (6, 8), offline edits merge on reconnect (6, 8), server never reads (2, 6, 8 checks).
- Deviation from the spec, to record in its As-built section later: live delivery is a streamed HTTP response (newline-delimited JSON over fetch), not a WebSocket, so the server stays dependency-free and the desktop bearer works without a WebSocket header workaround. Presence in step 3 may revisit this if latency needs it.
- The cave's room id lives inside the cave (a Setting); "Save as copy" will need to clear it so a copy gets its own room (note for step 2).
