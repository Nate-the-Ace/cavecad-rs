import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer, type Server } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openDb } from '../src/db.ts';
import { handleApi, serveApp, type ApiDeps } from '../src/api.ts';

const ADMIN = 'nathan@example.com';

async function start(mailOk = true, extra: Partial<ApiDeps> = {}) {
  const db = openDb(':memory:');
  const deps: ApiDeps = {
    db, adminEmail: ADMIN, passKey: generateKeyPairSync('ed25519').privateKey, publicUrl: 'https://cave.test',
    mailer: { url: 'https://mail', key: 'K', fetch: (async () => ({ json: async () => ({ ok: mailOk }) })) as unknown as typeof fetch },
    whoIs: async (r) => { const e = r.headers['x-test-user']; return e ? { email: String(e), name: String(e).split('@')[0], provider: null, subject: '' } : null; },
    ...extra,
  };
  const root = mkdtempSync(join(tmpdir(), 'cave-dist-'));
  writeFileSync(join(root, 'index.html'), '<html>app</html>');
  writeFileSync(join(root, 'privacy.html'), '<html>privacy</html>');
  mkdirSync(join(root, 'assets'));
  writeFileSync(join(root, 'assets', 'a.js'), 'x');
  const server: Server = createServer((req, res) => void ((req.url ?? '').startsWith('/api/') ? handleApi(deps, req, res) : serveApp(deps, root, req, res)));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const call = async (user: string | null, method: string, path: string, body?: unknown, extra: Record<string, string> = {}) => {
    const raw = typeof body === 'string' ? body : body ? JSON.stringify(body) : undefined;
    const res = await fetch(base + path, { method, headers: { ...(user ? { 'x-test-user': user } : {}), 'Content-Type': 'application/json', ...extra }, body: raw });
    return { status: res.status, headers: res.headers, json: res.headers.get('content-type')?.includes('json') ? await res.json() : await res.text() };
  };
  return { server, call, db };
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
    const home = await call(ADMIN, 'GET', '/');
    assert.equal(home.headers.get('cache-control'), 'no-store');
    assert.equal(home.json, '<html>app</html>');
    assert.equal((await call(ADMIN, 'GET', '/some/deep/link')).json, '<html>app</html>');
    assert.equal((await call(ADMIN, 'GET', '/assets/a.js')).headers.get('cache-control'), 'public, max-age=31536000, immutable');
    assert.equal((await call(ADMIN, 'GET', '/assets/missing.js')).status, 404);
    // fetch would tidy a ".." away before sending, so send the raw path with node:http
    const { get } = await import('node:http');
    const port = (server.address() as { port: number }).port;
    const raw = (path: string) => new Promise<number>((r) => get({ host: '127.0.0.1', port, path, headers: { 'x-test-user': ADMIN } }, (res) => { res.resume(); r(res.statusCode ?? 0); }));
    assert.equal(await raw('/assets/..%2f..%2fsecret'), 400);
    assert.equal(await raw('/%E0%A4%A'), 400, 'a broken percent escape');
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

test('cross-site and non-JSON writes are refused', async () => {
  const { server, call } = await start();
  try {
    await call(ADMIN, 'GET', '/api/me');
    const body = { email: 'rick@example.com', role: 'member' };
    const plainText = await call(ADMIN, 'POST', '/api/admin/invites', JSON.stringify(body), { 'Content-Type': 'text/plain' });
    assert.equal(plainText.status, 415);
    assert.deepEqual(plainText.json, { error: 'bad-request' });
    assert.equal((await call(ADMIN, 'GET', '/api/admin/invites')).json.invites.length, 0, 'no invite made');

    const sameSite = await call(ADMIN, 'POST', '/api/admin/invites', body, { 'Sec-Fetch-Site': 'same-site' });
    assert.equal(sameSite.status, 403);
    assert.deepEqual(sameSite.json, { error: 'cross-site' });
    assert.equal((await call(ADMIN, 'POST', '/api/admin/invites', body, { Origin: 'https://play.schonegg.stream' })).status, 403);
    assert.equal((await call(ADMIN, 'POST', '/api/admin/invites', body, { 'Sec-Fetch-Site': 'cross-site', Origin: 'tauri://localhost' })).status, 403, 'desktop origin without a bearer');
    assert.equal((await call(ADMIN, 'GET', '/api/admin/invites')).json.invites.length, 0, 'still no invite');

    assert.equal((await call(ADMIN, 'POST', '/api/admin/invites', body, { 'Sec-Fetch-Site': 'same-origin', Origin: 'https://cave.test' })).status, 200, 'the web app');
    assert.equal((await call(ADMIN, 'POST', '/api/admin/invites', { email: 'iris@example.com', role: 'member' }, { 'Sec-Fetch-Site': 'cross-site', Origin: 'tauri://localhost', Authorization: 'Bearer x' })).status, 200, 'the desktop app');
    assert.equal((await call(ADMIN, 'POST', '/api/admin/invites', { email: 'ann@example.com', role: 'member' })).status, 200, 'no browser headers at all');

    const desk = { 'Sec-Fetch-Site': 'cross-site', Origin: 'tauri://localhost' };
    const token = await call(null, 'POST', '/api/desktop-token', { code: 'c' }, desk);
    assert.notEqual(token.status, 403, 'the desktop app getting its token has no bearer yet');
    assert.equal(token.headers.get('access-control-allow-origin'), 'tauri://localhost');
    assert.equal((await call(null, 'POST', '/api/desktop-token', 'code=c', { ...desk, 'Content-Type': 'text/plain' })).status, 415, 'still JSON only');
    assert.equal((await call(null, 'POST', '/api/desktop-token', { code: 'c' }, { 'Sec-Fetch-Site': 'cross-site', Origin: 'https://evil.example' })).status, 403, 'still no foreign origins');
  } finally {
    server.close();
  }
});

test('desktop config: public, read from discovery, absent without OIDC', async () => {
  const { makeDesktopConfig } = await import('../src/desktop-token.ts');
  let reads = 0;
  const fake = (async (url: string) => {
    reads++;
    assert.equal(url, 'https://auth.test/.well-known/openid-configuration');
    return { ok: true, json: async () => ({ authorization_endpoint: 'https://auth.test/authorize', token_endpoint: 'https://auth.test/token' }) };
  }) as unknown as typeof fetch;
  const { server, call } = await start(true, { desktopConfig: makeDesktopConfig({ issuer: 'https://auth.test/', clientId: 'cavecad-desktop', fetch: fake }) });
  try {
    const desk = { 'Sec-Fetch-Site': 'cross-site', Origin: 'tauri://localhost' };
    const a = await call(null, 'GET', '/api/desktop-config', undefined, desk);
    assert.equal(a.status, 200, 'no sign-in and cross-site from the desktop app');
    assert.deepEqual(a.json, { authorizationEndpoint: 'https://auth.test/authorize', clientId: 'cavecad-desktop' });
    assert.equal(a.headers.get('access-control-allow-origin'), 'tauri://localhost');
    assert.equal(a.headers.get('access-control-allow-credentials'), null, 'never credentialed cross-origin');
    await call(null, 'GET', '/api/desktop-config');
    assert.equal(reads, 1, 'the discovery document is read once');
    assert.equal((await call(null, 'POST', '/api/desktop-config', {})).status, 401, 'only GET is the public route');
  } finally {
    server.close();
  }
  const down = await start(true, { desktopConfig: makeDesktopConfig({ issuer: 'https://auth.test', clientId: 'c', fetch: (async () => { throw new Error('down'); }) as unknown as typeof fetch }) });
  try {
    assert.equal((await down.call(null, 'GET', '/api/desktop-config')).status, 502, 'TinyAuth unreachable');
  } finally {
    down.server.close();
  }
  const none = await start();
  try {
    assert.equal((await none.call(null, 'GET', '/api/desktop-config')).status, 404, 'no OIDC configured');
  } finally {
    none.server.close();
  }
});

test('bad bodies are 400', async () => {
  const { server, call } = await start();
  try {
    await call(ADMIN, 'GET', '/api/me');
    assert.equal((await call(ADMIN, 'POST', '/api/admin/invites', 'null')).status, 400, 'null');
    assert.equal((await call(ADMIN, 'POST', '/api/admin/invites', '[1]')).status, 400, 'an array');
    assert.equal((await call(ADMIN, 'POST', '/api/admin/invites', '{"email":')).status, 400, 'malformed');
    assert.equal((await call(ADMIN, 'PATCH', '/api/me', 'x'.repeat(70 * 1024))).status, 400, 'too big');
  } finally {
    server.close();
  }
});

test('a non-member gets 403 everywhere but GET /api/me', async () => {
  const { server, call } = await start();
  try {
    await call(ADMIN, 'GET', '/api/me');
    assert.equal((await call('rick@example.com', 'GET', '/api/me')).status, 200);
    const patch = await call('rick@example.com', 'PATCH', '/api/me', { displayName: 'Rick' });
    assert.equal(patch.status, 403);
    assert.deepEqual(patch.json, { error: 'not-invited' });
  } finally {
    server.close();
  }
});

test('on the tailnet /api/me says so, for members and for the not-invited', async () => {
  const { server, call } = await start(true, { tailnet: true });
  try {
    assert.equal((await call(ADMIN, 'GET', '/api/me')).json.via, 'tailnet');
    assert.equal((await call('rick@example.com', 'GET', '/api/me')).json.via, 'tailnet');
  } finally {
    server.close();
  }
  const plain = await start();
  try {
    assert.equal((await plain.call(ADMIN, 'GET', '/api/me')).json.via, undefined);
  } finally {
    plain.server.close();
  }
});

test('the app itself is only for members and people with an invite waiting', async () => {
  const { server, call } = await start();
  try {
    assert.equal((await call(ADMIN, 'GET', '/')).json, '<html>app</html>', 'the first admin');
    const stranger = await call('stranger@example.com', 'GET', '/');
    assert.equal(stranger.status, 403);
    assert.match(stranger.json, /invite only/);
    assert.match(stranger.json, /stranger@example\.com/);
    assert.doesNotMatch(stranger.json, /<html>app<\/html>/);
    assert.equal((await call('stranger@example.com', 'GET', '/assets/a.js')).status, 403, 'no app code either');
    assert.equal((await call(null, 'GET', '/')).status, 401, 'not signed in');
    assert.equal((await call(null, 'GET', '/privacy.html')).json, '<html>privacy</html>', 'the privacy page is for anyone');
    assert.equal((await call('<b>x</b>@example.com', 'GET', '/')).json.includes('<b>x</b>'), false, 'the email is escaped');

    await call(ADMIN, 'POST', '/api/admin/invites', { email: 'rick@example.com', role: 'member' });
    assert.equal((await call('rick@example.com', 'GET', '/')).json, '<html>app</html>', 'an invite waiting for that email');
    assert.equal((await call('rick@example.com', 'GET', '/')).status, 200, 'loading the app does not use the invite up');
    assert.equal((await call('rick@example.com', 'GET', '/api/me')).json.status, 'member');

    const members = (await call(ADMIN, 'GET', '/api/admin/members')).json.members as { id: string; email: string }[];
    await call(ADMIN, 'DELETE', `/api/admin/members/${members.find((m) => m.email === 'rick@example.com')!.id}`);
    const removed = await call('rick@example.com', 'GET', '/');
    assert.equal(removed.status, 403);
    assert.match(removed.json, /removed/);

    await call(ADMIN, 'PUT', '/api/admin/settings/signups', { signups: 'open' });
    assert.equal((await call('anyone@example.com', 'GET', '/')).status, 200, 'sign-ups open: anyone');
  } finally {
    server.close();
  }
});

test('the "where your caves live" notice: shown once to members welcomed before it, marked with the welcome for new ones', async () => {
  const { server, call, db } = await start();
  try {
    const first = (await call(ADMIN, 'GET', '/api/me')).json;
    assert.equal(first.firstVisit, true);
    assert.equal(first.noticeSeen, false);
    // A new member: seeing the welcome page marks the notice too.
    await call(ADMIN, 'PATCH', '/api/me', { welcomed: true });
    const after = (await call(ADMIN, 'GET', '/api/me')).json;
    assert.deepEqual([after.firstVisit, after.noticeSeen], [false, true]);

    // A member welcomed before the notice existed (notice_version 0) sees it until they say OK.
    await call(ADMIN, 'PUT', '/api/admin/settings/signups', { signups: 'open' });
    await call('old@example.com', 'GET', '/api/me');
    await call('old@example.com', 'PATCH', '/api/me', { welcomed: true });
    db.prepare("UPDATE members SET notice_version = 0 WHERE email = 'old@example.com'").run();
    const old = (await call('old@example.com', 'GET', '/api/me')).json;
    assert.deepEqual([old.firstVisit, old.noticeSeen], [false, false]);
    assert.equal((await call('old@example.com', 'PATCH', '/api/me', { noticeSeen: true })).status, 200);
    assert.equal((await call('old@example.com', 'GET', '/api/me')).json.noticeSeen, true);
    // Only true marks it; anything else changes nothing.
    db.prepare("UPDATE members SET notice_version = 0 WHERE email = 'old@example.com'").run();
    await call('old@example.com', 'PATCH', '/api/me', { noticeSeen: false });
    assert.equal((await call('old@example.com', 'GET', '/api/me')).json.noticeSeen, false);
  } finally {
    server.close();
  }
});
