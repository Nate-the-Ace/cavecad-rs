import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openDb } from '../src/db.ts';
import { handleDownloads } from '../src/downloads.ts';

test('downloads are for members', async () => {
  const db = openDb(':memory:');
  const dir = mkdtempSync(join(tmpdir(), 'dl-'));
  writeFileSync(join(dir, 'CaveCAD_1.0.0_universal.dmg'), 'DMG');
  writeFileSync(join(dir, 'secret.txt'), 'no');
  writeFileSync(join(dir, 'downloads.json'), JSON.stringify({ version: '1.0.0', builtAt: '2026-11-08', files: { mac: 'CaveCAD_1.0.0_universal.dmg' } }));
  const d = { db, adminEmail: 'nathan@example.com', dir, whoIs: async (r: { headers: Record<string, unknown> }) => (r.headers['x-test-user'] ? { email: String(r.headers['x-test-user']), name: 'n', provider: null, subject: '' } : null) };
  const server = createServer((req, res) => void handleDownloads(d as never, req, res));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const get = (p: string, user?: string) => fetch(base + p, { headers: user ? { 'x-test-user': user } : {}, redirect: 'manual' });
  try {
    assert.equal((await get('/api/downloads')).status, 401);
    assert.equal(((await (await get('/api/downloads', 'nathan@example.com')).json()) as { version: string }).version, '1.0.0');
    const r = await get('/downloads/CaveCAD_1.0.0_universal.dmg', 'nathan@example.com');
    assert.equal(r.status, 200);
    assert.match(r.headers.get('content-disposition') ?? '', /attachment; filename="CaveCAD_1.0.0_universal.dmg"/);
    assert.equal(await r.text(), 'DMG');
    assert.equal((await get('/downloads/secret.txt', 'nathan@example.com')).status, 404);
    assert.equal((await get('/downloads/..%2Fdownloads.json', 'nathan@example.com')).status, 404);
    const out = await get('/downloads/CaveCAD_1.0.0_universal.dmg');
    assert.equal(out.status, 302);
    assert.equal(out.headers.get('location'), '/signin?return=%2Fdownloads%2FCaveCAD_1.0.0_universal.dmg');
    assert.equal((await get('/api/downloads', 'stranger@example.com')).status, 403);
  } finally {
    server.close();
  }
});

test('no manifest: 404', async () => {
  const db = openDb(':memory:');
  const d = { db, adminEmail: 'nathan@example.com', dir: mkdtempSync(join(tmpdir(), 'dl-')), whoIs: async () => ({ email: 'nathan@example.com', name: 'n', provider: null, subject: '' }) };
  const server = createServer((req, res) => void handleDownloads(d as never, req, res));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  try {
    assert.equal((await fetch(base + '/api/downloads')).status, 404);
    assert.equal((await fetch(base + '/downloads/x.dmg')).status, 404);
  } finally {
    server.close();
  }
});
