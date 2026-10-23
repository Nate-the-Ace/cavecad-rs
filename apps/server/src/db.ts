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
