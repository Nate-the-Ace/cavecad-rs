// Rooms, step 1: each cave's Private room. The server keeps devices' public keys, each cave's lock key
// wrapped for each device (which it cannot open), and the cave's history as numbered locked entries. It never
// reads an entry: blobs are base64 text stored as given. In step 1 only a cave's owner can reach it.
import { randomUUID } from 'node:crypto';
import type { Db } from './db.ts';

export type Device = { id: string; memberId: string; publicKey: string };
export type Wrapped = { device: string; wrapped: string };
/**
 * One of the member's caves for a device: its wrapped key for that device (null when it has none yet), its locked
 * label (name and summary, null until a device sets one), and whether another of the member's devices still has no key.
 */
export type MyCave = {
  id: string;
  keyVersion: number;
  wrapped: string | null;
  privateRoom: string;
  createdAt: number;
  label: { keyVersion: number; locked: string } | null;
  needsKeys: boolean;
  storage: 'locked' | 'open';
};
export type Entry = { number: number; keyVersion: number; locked: string; authorMember: string; authorDevice: string; receivedAt: number };

const fail = (code: string): never => {
  throw new Error(code);
};

/**
 * Registers a device's public key for a member (the same key gives the same device back). Keys are unique per
 * member only: another member holding the same key gets a device of their own and blocks no one.
 */
export function registerDevice(db: Db, memberId: string, publicKey: string, now: number): Device {
  // An X25519 public key: exactly 32 bytes, as 43 base64url characters.
  if (!/^[A-Za-z0-9_-]{43}$/.test(publicKey) || Buffer.from(publicKey, 'base64url').length !== 32) fail('bad-key');
  const found = db.prepare('SELECT id FROM devices WHERE member_id = ? AND public_key = ?').get(memberId, publicKey) as { id: string } | undefined;
  if (found) {
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
/** Checks the device is one of the member's own (`bad-device` if not) and marks it seen. */
export function checkDevice(db: Db, memberId: string, deviceId: string, now: number): void {
  requireOwnDevice(db, memberId, deviceId);
  db.prepare('UPDATE devices SET last_seen_at = ? WHERE id = ?').run(now, deviceId);
}
export type Storage = 'locked' | 'open';
/** The cave's storage (owner only, `not-found` otherwise). Open caves hold plain JSON text and have no keys. */
export function caveStorage(db: Db, memberId: string, caveId: string): Storage {
  requireOwner(db, memberId, caveId);
  return (db.prepare('SELECT storage FROM caves WHERE id = ?').get(caveId) as { storage: Storage }).storage;
}
/** The highest entry number the cave has had: its history's last, or the snapshot's when conversion emptied the history. */
const topNumber = (db: Db, caveId: string): number =>
  (
    db
      .prepare('SELECT MAX(COALESCE((SELECT MAX(number) FROM history WHERE cave_id = ?), 0), COALESCE((SELECT up_to FROM snapshots WHERE cave_id = ?), 0)) AS n')
      .get(caveId, caveId) as { n: number }
  ).n;
const keyVersion = (db: Db, caveId: string) => (db.prepare('SELECT key_version FROM caves WHERE id = ?').get(caveId) as { key_version: number }).key_version;

/** Makes a cave with its owner and Private room, and stores its lock key (version 1) wrapped for the given devices. */
export function createCave(db: Db, memberId: string, caveId: string, wrapped: Wrapped[], now: number, storage: Storage = 'locked'): void {
  if (!/^[A-Za-z0-9_-]{8,64}$/.test(caveId)) fail('bad-id');
  if (storage === 'open' && wrapped.length > 0) fail('bad-request');
  if (db.prepare('SELECT 1 FROM caves WHERE id = ?').get(caveId)) fail('exists');
  for (const w of wrapped) requireOwnDevice(db, memberId, w.device);
  db.exec('BEGIN');
  try {
    db.prepare('INSERT INTO caves (id, key_version, created_at, storage) VALUES (?, ?, ?, ?)').run(caveId, storage === 'open' ? 0 : 1, now, storage);
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
      `SELECT c.id, c.key_version, c.storage, c.created_at, w.wrapped, r.id AS room, l.key_version AS label_version, l.locked AS label,
         c.storage = 'locked' AND EXISTS (SELECT 1 FROM devices d WHERE d.member_id = ? AND NOT EXISTS
           (SELECT 1 FROM wrapped_keys k WHERE k.cave_id = c.id AND k.key_version = c.key_version AND k.device_id = d.id)) AS needs_keys
       FROM caves c
       JOIN cave_owners o ON o.cave_id = c.id AND o.member_id = ?
       JOIN rooms r ON r.cave_id = c.id AND r.kind = 'private'
       LEFT JOIN wrapped_keys w ON w.cave_id = c.id AND w.key_version = c.key_version AND w.device_id = ?
       LEFT JOIN cave_labels l ON l.cave_id = c.id
       ORDER BY c.created_at`,
    )
    .all(memberId, memberId, deviceId) as {
    id: string;
    key_version: number;
    storage: Storage;
    created_at: number;
    wrapped: string | null;
    room: string;
    label_version: number | null;
    label: string | null;
    needs_keys: number;
  }[];
  return rows.map((r) => ({
    id: r.id,
    keyVersion: r.key_version,
    wrapped: r.wrapped,
    privateRoom: r.room,
    createdAt: r.created_at,
    label: r.label === null || r.label_version === null ? null : { keyVersion: r.label_version, locked: r.label },
    needsKeys: !!r.needs_keys,
    storage: r.storage,
  }));
}

/** Keeps the cave's locked label (its name and summary for cave lists), locked under the current key version. */
export function putLabel(db: Db, memberId: string, caveId: string, version: number, locked: string, now: number): void {
  const open = caveStorage(db, memberId, caveId) === 'open';
  if (open) version = 0;
  else if (version !== keyVersion(db, caveId)) fail('bad-request');
  db.prepare(
    `INSERT INTO cave_labels (cave_id, key_version, locked, updated_at) VALUES (?, ?, ?, ?)
     ON CONFLICT(cave_id) DO UPDATE SET key_version = excluded.key_version, locked = excluded.locked, updated_at = excluded.updated_at`,
  ).run(caveId, version, locked, now);
}

/** The member's devices, with public keys (to wrap a new cave's key for each) and when each was added and last seen. */
export function myDevices(db: Db, memberId: string): { id: string; publicKey: string; createdAt: number; lastSeen: number | null }[] {
  return (
    db.prepare('SELECT id, public_key, created_at, last_seen_at FROM devices WHERE member_id = ? ORDER BY created_at').all(memberId) as {
      id: string;
      public_key: string;
      created_at: number;
      last_seen_at: number | null;
    }[]
  ).map((r) => ({ id: r.id, publicKey: r.public_key, createdAt: r.created_at, lastSeen: r.last_seen_at }));
}

/** The owner's devices that have no wrapped key for the cave's current version. */
export function devicesMissingKey(db: Db, memberId: string, caveId: string): { id: string; publicKey: string }[] {
  if (caveStorage(db, memberId, caveId) === 'open') return [];
  return db
    .prepare(
      `SELECT d.id, d.public_key FROM devices d WHERE d.member_id = ? AND NOT EXISTS
       (SELECT 1 FROM wrapped_keys w WHERE w.cave_id = ? AND w.key_version = ? AND w.device_id = d.id) ORDER BY d.created_at`,
    )
    .all(memberId, caveId, keyVersion(db, caveId))
    .map((r) => ({ id: (r as { id: string }).id, publicKey: (r as { public_key: string }).public_key }));
}

/** Stores wrapped copies of the cave's current key version for the owner's own devices. */
export function putWrappedKeys(db: Db, memberId: string, caveId: string, version: number, wrapped: Wrapped[]): void {
  if (caveStorage(db, memberId, caveId) === 'open') fail('bad-request');
  if (version !== keyVersion(db, caveId)) fail('bad-request');
  for (const w of wrapped) requireOwnDevice(db, memberId, w.device);
  for (const w of wrapped) db.prepare('INSERT OR REPLACE INTO wrapped_keys (cave_id, key_version, device_id, wrapped) VALUES (?, ?, ?, ?)').run(caveId, version, w.device, w.wrapped);
}

/** Adds a locked entry to the cave's history; gives back its number. */
export function appendEntry(db: Db, memberId: string, deviceId: string, caveId: string, version: number, locked: string, now: number): number {
  const open = caveStorage(db, memberId, caveId) === 'open';
  requireOwnDevice(db, memberId, deviceId);
  if (open) version = 0;
  else if (version < 1 || version > keyVersion(db, caveId)) fail('bad-request');
  const room = (db.prepare("SELECT id FROM rooms WHERE cave_id = ? AND kind = 'private'").get(caveId) as { id: string }).id;
  // Numbered in the same statement that stores it, so two writers can never take the same number.
  const row = db
    .prepare(
      `INSERT INTO history (cave_id, number, key_version, locked, author_member, author_device, room_id, received_at)
       SELECT ?, MAX(COALESCE(MAX(number), 0), COALESCE((SELECT up_to FROM snapshots WHERE cave_id = ?), 0)) + 1, ?, ?, ?, ?, ?, ? FROM history WHERE cave_id = ? RETURNING number`,
    )
    .get(caveId, caveId, version, locked, memberId, deviceId, room, now, caveId) as { number: number };
  return row.number;
}

/** Entries after `after`, oldest first, at most `limit`, stopping once their locked text reaches `maxBytes` (always at least one). */
export function entriesAfter(db: Db, memberId: string, caveId: string, after: number, limit: number, maxBytes = Infinity): Entry[] {
  requireOwner(db, memberId, caveId);
  const out: Entry[] = [];
  let bytes = 0;
  const rows = db.prepare('SELECT number, key_version, locked, author_member, author_device, received_at FROM history WHERE cave_id = ? AND number > ? ORDER BY number LIMIT ?').iterate(caveId, after, limit);
  for (const row of rows) {
    const r = row as { number: number; key_version: number; locked: string; author_member: string; author_device: string; received_at: number };
    out.push({ number: r.number, keyVersion: r.key_version, locked: r.locked, authorMember: r.author_member, authorDevice: r.author_device, receivedAt: r.received_at });
    bytes += r.locked.length;
    if (bytes >= maxBytes) break;
  }
  return out;
}

/**
 * Keeps a locked whole-cave snapshot covering entries up to `upTo`. It never goes backwards: one covering no more
 * than the kept snapshot is ignored. `upTo` past the cave's last entry is refused (0 is fine for an empty cave).
 */
export function putSnapshot(db: Db, memberId: string, caveId: string, upTo: number, version: number, locked: string, now: number): void {
  requireOwner(db, memberId, caveId);
  if (upTo > topNumber(db, caveId)) fail('bad-request');
  db.prepare(
    `INSERT INTO snapshots (cave_id, up_to, key_version, locked, created_at) VALUES (?, ?, ?, ?, ?)
     ON CONFLICT(cave_id) DO UPDATE SET up_to = excluded.up_to, key_version = excluded.key_version, locked = excluded.locked, created_at = excluded.created_at
     WHERE excluded.up_to > snapshots.up_to`,
  ).run(caveId, upTo, version, locked, now);
}

/**
 * The latest snapshot, or null. With `after` (a reconnecting client's last entry number), null too when the
 * snapshot covers no more than that, so a client that is already past it does not download it again.
 */
export function latestSnapshot(db: Db, memberId: string, caveId: string, after?: number): { upTo: number; keyVersion: number; locked: string } | null {
  requireOwner(db, memberId, caveId);
  const r = db.prepare('SELECT up_to, key_version, locked FROM snapshots WHERE cave_id = ?').get(caveId) as { up_to: number; key_version: number; locked: string } | undefined;
  if (!r || (after !== undefined && r.up_to <= after)) return null;
  return { upTo: r.up_to, keyVersion: r.key_version, locked: r.locked };
}

/**
 * Converts a locked cave to open: `snapshot` (plain JSON text, from the owner's device that holds the key) replaces
 * everything locked. `upTo` must be the cave's last entry number. Numbering carries on after it.
 */
export function openCave(db: Db, memberId: string, caveId: string, upTo: number, snapshot: string, now: number): void {
  if (caveStorage(db, memberId, caveId) !== 'locked') fail('bad-request');
  if (upTo !== topNumber(db, caveId)) fail('bad-request');
  db.exec('BEGIN');
  try {
    db.prepare('DELETE FROM history WHERE cave_id = ?').run(caveId);
    db.prepare('DELETE FROM snapshots WHERE cave_id = ?').run(caveId);
    db.prepare('DELETE FROM wrapped_keys WHERE cave_id = ?').run(caveId);
    db.prepare('DELETE FROM cave_labels WHERE cave_id = ?').run(caveId);
    db.prepare('INSERT INTO snapshots (cave_id, up_to, key_version, locked, created_at) VALUES (?, ?, 0, ?, ?)').run(caveId, upTo, snapshot, now);
    db.prepare("UPDATE caves SET storage = 'open', key_version = 0 WHERE id = ?").run(caveId);
    db.exec('COMMIT');
  } catch (e) {
    db.exec('ROLLBACK');
    throw e;
  }
}
