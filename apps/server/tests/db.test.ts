import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openDb, getSetting } from '../src/db.ts';

test('creates the four tables', () => {
  const db = openDb(':memory:');
  const names = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").all().map((r) => r.name);
  assert.deepEqual(names, ['drives', 'invites', 'members', 'settings']);
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
