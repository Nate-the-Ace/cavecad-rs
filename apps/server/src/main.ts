// Starts the CaveCAD server: the web app and the account API on one port. Settings come from the
// environment (see .env.example); nothing secret is in the code.
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { createPrivateKey } from 'node:crypto';
import { resolve } from 'node:path';
import { openDb } from './db.ts';
import { handleApi } from './api.ts';
import { serveStatic } from './static.ts';
import { makeWhoIs, discoveredKeys } from './identity.ts';
import { makeDesktopConfig, makeDesktopToken } from './desktop-token.ts';

const env = process.env;
const need = (k: string) => env[k] || (() => { throw new Error(`${k} is not set`); })();

const devIdentity = env.DEV_IDENTITY || undefined;
if (devIdentity) {
  // DEV_IDENTITY signs in everyone on this machine, so it must never run beside the real gates.
  if (env.EDGE_ADDRESS || env.OIDC_ISSUER) throw new Error('DEV_IDENTITY is for local development only: unset it, or unset EDGE_ADDRESS and OIDC_ISSUER');
  const host = (() => { try { return new URL(need('PUBLIC_URL')).hostname; } catch { return ''; } })();
  if (host !== 'localhost' && host !== '127.0.0.1') throw new Error(`DEV_IDENTITY needs a localhost PUBLIC_URL, not ${env.PUBLIC_URL}`);
}
// TAILNET_IDENTITY=1: reached only through `tailscale serve` on this machine (until there is a public address).
// The server must then listen on 127.0.0.1, or anyone who can reach the port could claim any email.
const tailnet = env.TAILNET_IDENTITY === '1';
if (tailnet) {
  if (devIdentity) throw new Error('DEV_IDENTITY and TAILNET_IDENTITY cannot run together');
  if (env.HOST !== '127.0.0.1') throw new Error('TAILNET_IDENTITY needs HOST=127.0.0.1');
}
const edgeAddress = devIdentity || tailnet ? env.EDGE_ADDRESS ?? '' : need('EDGE_ADDRESS');
const gateSecret = devIdentity || tailnet ? env.GATE_SECRET ?? '' : need('GATE_SECRET');

const db = openDb(env.DB_PATH ?? 'cavecad.sqlite');
const passKey = createPrivateKey({ key: JSON.parse(readFileSync(need('PASS_KEY_FILE'), 'utf8')), format: 'jwk' });
const oidc = env.OIDC_ISSUER ? { issuer: env.OIDC_ISSUER, audience: need('OIDC_CLIENT_ID'), keys: discoveredKeys(env.OIDC_ISSUER) } : undefined;
const deps = {
  db,
  passKey,
  adminEmail: need('ADMIN_EMAIL'),
  publicUrl: need('PUBLIC_URL'),
  mailer: { url: env.MAILER_URL ?? '', key: env.MAILER_KEY ?? '' },
  whoIs: makeWhoIs({ edgeAddress, gateSecret, oidc, tailnet, devIdentity }),
  desktopToken: oidc ? makeDesktopToken({ issuer: oidc.issuer, clientId: oidc.audience, clientSecret: env.OIDC_CLIENT_SECRET ?? '' }) : undefined,
  desktopConfig: oidc ? makeDesktopConfig({ issuer: oidc.issuer, clientId: oidc.audience }) : undefined,
};
const dist = resolve(env.DIST_DIR ?? '../web/dist');

createServer((req, res) => {
  const p = (req.url ?? '/').startsWith('/api/') ? handleApi(deps, req, res) : serveStatic(dist, req, res);
  p.catch((e) => {
    console.error(e);
    if (!res.headersSent) res.writeHead(500, { 'Content-Type': 'application/json' }).end('{"error":"server"}');
  });
}).listen(Number(env.PORT ?? 8787), env.HOST ?? '0.0.0.0', () => console.log(`CaveCAD server on ${env.HOST ?? '0.0.0.0'}:${env.PORT ?? 8787}`));
