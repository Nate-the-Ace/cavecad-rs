// The server's one database file: members, invites, which drives each member uses, and settings.
// Nothing about any cave is ever stored here.
import { DatabaseSync } from 'node:sqlite';

const SCHEMA = `
CREATE TABLE IF NOT EXISTS members (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  colour TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('admin', 'member')),
  provider TEXT,
  invited_by TEXT,
  created_at INTEGER NOT NULL,
  last_seen_at INTEGER,
  welcomed_at INTEGER,
  removed_at INTEGER
);
CREATE TABLE IF NOT EXISTS invites (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('admin', 'member')),
  token_hash TEXT NOT NULL UNIQUE,
  invited_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  used_at INTEGER,
  cancelled_at INTEGER
);
CREATE TABLE IF NOT EXISTS drives (
  id TEXT PRIMARY KEY,
  member_id TEXT NOT NULL REFERENCES members(id),
  provider TEXT NOT NULL CHECK (provider IN ('gdrive', 'onedrive', 'dropbox', 'webdav')),
  account_label TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS devices (
  id TEXT PRIMARY KEY,
  member_id TEXT NOT NULL REFERENCES members(id),
  public_key TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_seen_at INTEGER,
  UNIQUE (member_id, public_key)
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
-- A cave's name and summary for its owners' cave lists, locked with the cave key (the server cannot read it).
CREATE TABLE IF NOT EXISTS cave_labels (
  cave_id TEXT PRIMARY KEY REFERENCES caves(id),
  key_version INTEGER NOT NULL,
  locked TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS rooms_cave ON rooms(cave_id);
CREATE INDEX IF NOT EXISTS cave_owners_member ON cave_owners(member_id);
CREATE INDEX IF NOT EXISTS devices_member ON devices(member_id);
INSERT OR IGNORE INTO settings (key, value) VALUES ('signups', 'invite');
`;

export type Db = DatabaseSync;

/** Opens (and if needed creates) the database at `path`; `':memory:'` for tests. */
export function openDb(path: string): Db {
  const db = new DatabaseSync(path);
  db.exec('PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;');
  db.exec(SCHEMA);
  return db;
}

/** One setting's value, or `undefined`. */
export function getSetting(db: Db, key: string): string | undefined {
  return (db.prepare('SELECT value FROM settings WHERE key = ?').get(key) as { value: string } | undefined)?.value;
}

/** Sets one setting. */
export function setSetting(db: Db, key: string, value: string): void {
  db.prepare('INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value').run(key, value);
}
