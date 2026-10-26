import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openDb, getSetting } from '../src/db.ts';

test('creates the tables', () => {
  const db = openDb(':memory:');
  const names = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").all().map((r) => r.name);
  assert.deepEqual(names, ['cave_gone', 'cave_labels', 'cave_owners', 'caves', 'devices', 'drives', 'history', 'invites', 'members', 'room_guests', 'rooms', 'settings', 'snapshots', 'wrapped_keys']);
});

test('sign-ups start invite only', () => {
  assert.equal(getSetting(openDb(':memory:'), 'signups'), 'invite');
});

test('opening the same file twice keeps one default', () => {
  const file = join(mkdtempSync(join(tmpdir(), 'cave-db-')), 'a.sqlite');
  openDb(file).close();
  const db = openDb(file);
  assert.equal(db.prepare("SELECT COUNT(*) AS n FROM settings WHERE key = 'signups'").get().n, 1);
});

test('indexes for the rooms lookups', () => {
  const db = openDb(':memory:');
  const idx = db.prepare("SELECT tbl_name, sql FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL").all().map((r) => `${r.tbl_name}:${r.sql}`);
  for (const [table, col] of [['rooms', 'cave_id'], ['cave_owners', 'member_id'], ['devices', 'member_id']]) {
    assert.ok(idx.some((s) => s.startsWith(`${table}:`) && s.includes(`(${col})`)), `${table}(${col})`);
  }
});

test('an existing database without notice_version on members gains it at 0 on start', async () => {
  const { DatabaseSync } = await import('node:sqlite');
  const file = join(mkdtempSync(join(tmpdir(), 'cave-db-')), 'old.sqlite');
  const old = new DatabaseSync(file);
  old.exec("CREATE TABLE members (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, display_name TEXT NOT NULL, colour TEXT NOT NULL, role TEXT NOT NULL, provider TEXT, invited_by TEXT, created_at INTEGER NOT NULL, last_seen_at INTEGER, welcomed_at INTEGER, removed_at INTEGER); INSERT INTO members (id, email, display_name, colour, role, created_at, welcomed_at) VALUES ('m1', 'a@b.c', 'A', '#000000', 'member', 1, 1);");
  old.close();
  const db = openDb(file);
  assert.equal(db.prepare("SELECT notice_version FROM members WHERE id = 'm1'").get().notice_version, 0);
  db.close();
  openDb(file).close(); // and a second start leaves it be
});
