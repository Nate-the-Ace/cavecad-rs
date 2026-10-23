import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer, type Server } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openDb } from '../src/db.ts';
import { handleApi, type ApiDeps } from '../src/api.ts';
import { serveStatic } from '../src/static.ts';

const ADMIN = 'nathan@example.com';

async function start(mailOk = true) {
  const db = openDb(':memory:');
  const deps: ApiDeps = {
    db, adminEmail: ADMIN, passKey: generateKeyPairSync('ed25519').privateKey, publicUrl: 'https://cave.test',
    mailer: { url: 'https://mail', key: 'K', fetch: (async () => ({ json: async () => ({ ok: mailOk }) })) as unknown as typeof fetch },
    whoIs: async (r) => { const e = r.headers['x-test-user']; return e ? { email: String(e), name: String(e).split('@')[0], provider: null, subject: '' } : null; },
  };
  const root = mkdtempSync(join(tmpdir(), 'cave-dist-'));
  writeFileSync(join(root, 'index.html'), '<html>app</html>');
  mkdirSync(join(root, 'assets'));
  writeFileSync(join(root, 'assets', 'a.js'), 'x');
  const server: Server = createServer((req, res) => void ((req.url ?? '').startsWith('/api/') ? handleApi(deps, req, res) : serveStatic(root, req, res)));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const call = async (user: string | null, method: string, path: string, body?: unknown, extra: Record<string, string> = {}) => {
    const res = await fetch(base + path, { method, headers: { ...(user ? { 'x-test-user': user } : {}), 'Content-Type': 'application/json', ...extra }, body: body ? JSON.stringify(body) : undefined });
    return { status: res.status, headers: res.headers, json: res.headers.get('content-type')?.includes('json') ? await res.json() : await res.text() };
  };
  return { server, call };
}

test('me, invites, admin rules over HTTP', async () => {
  const { server, call } = await start();
  try {
    assert.equal((await call(null, 'GET', '/api/me')).status, 401);
    const me = await call(ADMIN, 'GET', '/api/me');
    assert.equal(me.json.status, 'member');
    assert.equal(me.json.member.role, 'admin');
    assert.equal(me.json.firstVisit, true);
    assert.match(me.json.pass, /^[\w-]+\.[\w-]+$/);
    assert.deepEqual((await call('rick@example.com', 'GET', '/api/me')).json, { status: 'not-invited', email: 'rick@example.com', reason: 'none' });

    const inv = await call(ADMIN, 'POST', '/api/admin/invites', { email: 'rick@example.com', role: 'member' });
    assert.equal(inv.json.emailed, true);
    const token = inv.json.link.split('?signup=')[1];
    assert.ok(inv.json.link.startsWith('https://cave.test/?signup='));
    const rick = await call('rick@example.com', 'GET', '/api/me', undefined, { 'X-Cave-Signup': token });
    assert.equal(rick.json.status, 'member');

    assert.equal((await call('rick@example.com', 'GET', '/api/admin/members')).status, 403);
    assert.equal((await call(ADMIN, 'DELETE', `/api/admin/members/${me.json.member.id}`)).json.error, 'self');
    assert.equal((await call(ADMIN, 'POST', '/api/admin/invites', { email: 'rick@example.com', role: 'member' })).status, 409);
    const members = (await call(ADMIN, 'GET', '/api/admin/members')).json.members;
    assert.equal(members.length, 2);
    assert.equal(members[1].invitedByName, 'nathan');

    await call(ADMIN, 'PATCH', '/api/me', { welcomed: true, displayName: 'Nathan', colour: '#2fb86a' });
    const again = (await call(ADMIN, 'GET', '/api/me')).json;
    assert.equal(again.firstVisit, false);
    assert.equal(again.member.colour, '#2fb86a');

    const drive = (await call(ADMIN, 'POST', '/api/me/drives', { provider: 'dropbox', accountLabel: 'nathan@example.com' })).json.drive;
    assert.equal((await call(ADMIN, 'GET', '/api/me')).json.drives.length, 1);
    assert.equal((await call('rick@example.com', 'DELETE', `/api/me/drives/${drive.id}`)).status, 404);
    assert.equal((await call(ADMIN, 'DELETE', `/api/me/drives/${drive.id}`)).status, 200);

    assert.equal((await call(ADMIN, 'PUT', '/api/admin/settings/signups', { signups: 'open' })).json.signups, 'open');
    assert.equal((await call('anyone@example.com', 'GET', '/api/me')).json.status, 'member');
  } finally {
    server.close();
  }
});

test('an invite still exists when the email fails', async () => {
  const { server, call } = await start(false);
  try {
    await call(ADMIN, 'GET', '/api/me');
    const inv = await call(ADMIN, 'POST', '/api/admin/invites', { email: 'iris@example.com', role: 'admin' });
    assert.equal(inv.json.emailed, false);
    assert.equal((await call(ADMIN, 'GET', '/api/admin/invites')).json.invites.length, 1);
  } finally {
    server.close();
  }
});

test('static files and cache rules', async () => {
  const { server, call } = await start();
  try {
    const home = await call(null, 'GET', '/');
    assert.equal(home.headers.get('cache-control'), 'no-store');
    assert.equal(home.json, '<html>app</html>');
    assert.equal((await call(null, 'GET', '/some/deep/link')).json, '<html>app</html>');
    assert.equal((await call(null, 'GET', '/assets/a.js')).headers.get('cache-control'), 'public, max-age=31536000, immutable');
    assert.equal((await call(null, 'GET', '/assets/missing.js')).status, 404);
    // fetch would tidy a ".." away before sending, so send the raw path with node:http
    const { get } = await import('node:http');
    const port = (server.address() as { port: number }).port;
    const status = await new Promise<number>((r) => get({ host: '127.0.0.1', port, path: '/assets/..%2f..%2fsecret' }, (res) => { res.resume(); r(res.statusCode ?? 0); }));
    assert.equal(status, 400);
  } finally {
    server.close();
  }
});

test('CORS only for the desktop app', async () => {
  const { server, call } = await start();
  try {
    assert.equal((await call(null, 'OPTIONS', '/api/me', undefined, { Origin: 'tauri://localhost' })).headers.get('access-control-allow-origin'), 'tauri://localhost');
    assert.equal((await call(null, 'GET', '/api/me', undefined, { Origin: 'https://evil.example' })).headers.get('access-control-allow-origin'), null);
  } finally {
    server.close();
  }
});
