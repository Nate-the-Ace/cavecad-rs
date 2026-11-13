import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
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

test('desktop updates: members ask by version, and fetch the file the answer names', async () => {
  const db = openDb(':memory:');
  const dir = mkdtempSync(join(tmpdir(), 'dl-'));
  writeFileSync(join(dir, 'CaveCAD_0.1.7_universal.dmg'), 'DMG');
  writeFileSync(join(dir, 'CaveCAD.app.tar.gz'), 'TGZ');
  writeFileSync(join(dir, 'downloads.json'), JSON.stringify({ version: '0.1.7', builtAt: '2026-11-09', files: { mac: 'CaveCAD_0.1.7_universal.dmg' } }));
  writeFileSync(
    join(dir, 'update.json'),
    JSON.stringify({ version: '0.1.7', notes: 'n', pubDate: '2026-11-09T00:00:00Z', platforms: { 'darwin-aarch64': { file: 'CaveCAD.app.tar.gz', signature: 'SIG' } } }),
  );
  const d = { db, adminEmail: 'nathan@example.com', dir, whoIs: async (r: { headers: Record<string, unknown> }) => (r.headers['x-test-user'] ? { email: String(r.headers['x-test-user']), name: 'n', provider: null, subject: '' } : null) };
  const server = createServer((req, res) => void handleDownloads(d as never, req, res));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const get = (p: string, user?: string) =>
    fetch(base + p, { headers: { ...(user ? { 'x-test-user': user } : {}), 'x-forwarded-host': 'api.cavecad.com', 'x-forwarded-proto': 'https' }, redirect: 'manual' });
  const me = 'nathan@example.com';
  try {
    assert.equal((await get('/api/desktop-update/darwin/aarch64/0.1.6')).status, 401);
    assert.equal((await get('/api/desktop-update/darwin/aarch64/0.1.6', 'stranger@example.com')).status, 403);
    const r = await get('/api/desktop-update/darwin/aarch64/0.1.6', me);
    assert.equal(r.status, 200);
    const j = (await r.json()) as { version: string; url: string; signature: string };
    assert.equal(j.version, '0.1.7');
    assert.equal(j.url, 'https://api.cavecad.com/downloads/CaveCAD.app.tar.gz');
    assert.equal(j.signature, 'SIG');
    // as new or newer, or a local build (0.0.1 is older, so it is offered)
    assert.equal((await get('/api/desktop-update/darwin/aarch64/0.1.7', me)).status, 204);
    assert.equal((await get('/api/desktop-update/darwin/aarch64/0.1.10', me)).status, 204);
    assert.equal((await get('/api/desktop-update/darwin/aarch64/0.0.1', me)).status, 200);
    // no build for that platform
    assert.equal((await get('/api/desktop-update/linux/x86_64/0.1.6', me)).status, 204);
    // the update file is served like an installer
    assert.equal(await (await get('/downloads/CaveCAD.app.tar.gz', me)).text(), 'TGZ');
  } finally {
    server.close();
  }
});

test('versions compare as numbers', async () => {
  const { compareVersions } = await import('../src/downloads.ts');
  assert.ok(compareVersions('0.1.9', '0.1.10') < 0);
  assert.ok(compareVersions('0.1.10', '0.1.9') > 0);
  assert.equal(compareVersions('0.1.7', '0.1.7'), 0);
  assert.ok(compareVersions('0.0.1', '0.1.0') < 0);
});

test('test builds: admins see and fetch them, and the dev channel offers the newer build; members never do', async () => {
  const db = openDb(':memory:');
  const root = mkdtempSync(join(tmpdir(), 'dl-'));
  const dir = join(root, 'downloads');
  const devDir = join(root, 'downloads-dev');
  mkdirSync(dir);
  mkdirSync(devDir);
  writeFileSync(join(dir, 'CaveCAD_0.1.7_universal.dmg'), 'DMG');
  writeFileSync(join(dir, 'CaveCAD.app.tar.gz'), 'TGZ');
  writeFileSync(join(dir, 'downloads.json'), JSON.stringify({ version: '0.1.7', builtAt: 'x', files: { mac: 'CaveCAD_0.1.7_universal.dmg' } }));
  writeFileSync(join(dir, 'update.json'), JSON.stringify({ version: '0.1.7', platforms: { 'darwin-aarch64': { file: 'CaveCAD.app.tar.gz', signature: 'SIG' } } }));
  writeFileSync(join(devDir, 'CaveCAD_0.1.9_universal.dmg'), 'DEVDMG');
  writeFileSync(join(devDir, 'CaveCAD.app.tar.gz'), 'DEVTGZ');
  writeFileSync(join(devDir, 'downloads.json'), JSON.stringify({ version: '0.1.9', builtAt: 'x', files: { mac: 'CaveCAD_0.1.9_universal.dmg' } }));
  writeFileSync(join(devDir, 'update.json'), JSON.stringify({ version: '0.1.9', platforms: { 'darwin-aarch64': { file: 'CaveCAD.app.tar.gz', signature: 'DEVSIG' } } }));
  db.prepare("INSERT INTO members (id, email, display_name, colour, role, created_at) VALUES ('a', 'boss@example.com', 'b', '#000', 'admin', 0), ('m', 'pal@example.com', 'p', '#000', 'member', 0)").run();
  const d = { db, adminEmail: 'boss@example.com', dir, whoIs: async (r: { headers: Record<string, unknown> }) => (r.headers['x-test-user'] ? { email: String(r.headers['x-test-user']), name: 'n', provider: null, subject: '' } : null) };
  const server = createServer((req, res) => void handleDownloads(d as never, req, res));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const get = (p: string, user: string) => fetch(base + p, { headers: { 'x-test-user': user, 'x-forwarded-host': 'api.cavecad.com' } });
  const [boss, pal] = ['boss@example.com', 'pal@example.com'];
  try {
    // the list: admins get the test build beside the regular one
    assert.equal(((await (await get('/api/downloads', boss)).json()) as { dev?: { version: string } }).dev?.version, '0.1.9');
    assert.equal(((await (await get('/api/downloads', pal)).json()) as { dev?: unknown }).dev, undefined);
    // the files
    assert.equal(await (await get('/downloads/dev/CaveCAD_0.1.9_universal.dmg', boss)).text(), 'DEVDMG');
    assert.equal((await get('/downloads/dev/CaveCAD_0.1.9_universal.dmg', pal)).status, 404);
    assert.equal((await get('/downloads/dev/..%2Fdownloads%2Fdownloads.json', boss)).status, 404);
    assert.equal((await get('/downloads/CaveCAD_0.1.9_universal.dmg', boss)).status, 404);
    // updates: the dev channel offers the newer test build to admins only
    const devAsk = '/api/desktop-update/darwin/aarch64/0.1.6?channel=dev';
    const j = (await (await get(devAsk, boss)).json()) as { version: string; url: string; signature: string };
    assert.deepEqual([j.version, j.url, j.signature], ['0.1.9', 'https://api.cavecad.com/downloads/dev/CaveCAD.app.tar.gz', 'DEVSIG']);
    assert.equal(((await (await get(devAsk, pal)).json()) as { version: string }).version, '0.1.7');
    assert.equal(((await (await get('/api/desktop-update/darwin/aarch64/0.1.6', boss)).json()) as { version: string }).version, '0.1.7');
    assert.equal((await get('/api/desktop-update/darwin/aarch64/0.1.9?channel=dev', boss)).status, 204);
    // a regular release newer than the test build wins on the dev channel too
    writeFileSync(join(dir, 'update.json'), JSON.stringify({ version: '0.1.12', platforms: { 'darwin-aarch64': { file: 'CaveCAD.app.tar.gz', signature: 'SIG' } } }));
    const k = (await (await get('/api/desktop-update/darwin/aarch64/0.1.9?channel=dev', boss)).json()) as { version: string; url: string };
    assert.deepEqual([k.version, k.url], ['0.1.12', 'https://api.cavecad.com/downloads/CaveCAD.app.tar.gz']);
  } finally {
    server.close();
  }
});
