import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer, request } from 'node:http';
import { readJson, send, sendBadBody } from '../src/http.ts';

async function start() {
  const server = createServer(async (req, res) => {
    try {
      send(res, 200, await readJson(req, 1000));
    } catch (e) {
      sendBadBody(res, e);
    }
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  return { server, port: (server.address() as { port: number }).port };
}

test('a body declared too big is refused before it is read, and the connection closes', async () => {
  const { server, port } = await start();
  try {
    const answer = await new Promise<{ status: number; connection: string | undefined; body: string }>((resolve, reject) => {
      const req = request({ host: '127.0.0.1', port, method: 'POST', path: '/', headers: { 'Content-Type': 'application/json', 'Content-Length': 50_000_000 } }, (res) => {
        let body = '';
        res.on('data', (c) => (body += c));
        res.on('end', () => {
          resolve({ status: res.statusCode ?? 0, connection: res.headers.connection, body });
          req.destroy();
        });
      });
      req.on('error', reject);
      req.write('{"a":'); // most of the promised body never comes
    });
    assert.equal(answer.status, 400);
    assert.equal(answer.connection, 'close');
    assert.deepEqual(JSON.parse(answer.body), { error: 'bad-request' });
  } finally {
    server.close();
    server.closeAllConnections();
  }
});

test('small bodies still read; bad ones are 400 without closing', async () => {
  const { server, port } = await start();
  try {
    const base = `http://127.0.0.1:${port}/`;
    const ok = await fetch(base, { method: 'POST', body: '{"a":1}' });
    assert.deepEqual(await ok.json(), { a: 1 });
    const bad = await fetch(base, { method: 'POST', body: '[1]' });
    assert.equal(bad.status, 400);
    assert.notEqual(bad.headers.get('connection'), 'close');
  } finally {
    server.close();
    server.closeAllConnections();
  }
});
