// Starts the CaveCAD server: sign-in, the web app, downloads and the account API on one port. Settings come
// from the environment (see .env.example); nothing secret is in the code.
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { createPrivateKey } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { openDb } from './db.ts';
import { handleApi, serveApp } from './api.ts';
import { makeWhoIs } from './identity.ts';
import { handleSignin } from './signin.ts';
import { handleDownloads } from './downloads.ts';
import { mayGetIn } from './members.ts';
import { sendCode } from './mailer.ts';

const env = process.env;
const need = (k: string) => env[k] || (() => { throw new Error(`${k} is not set`); })();

const publicUrl = need('PUBLIC_URL');
// Signs the emailed codes and Google's sign-in state: a short one could be guessed.
const codeSecret = need('CODE_SECRET');
if (codeSecret.length < 32) throw new Error('CODE_SECRET must be at least 32 characters (make one: node -e "console.log(require(\'crypto\').randomBytes(32).toString(\'hex\'))")');
const devIdentity = env.DEV_IDENTITY || undefined;
if (devIdentity) {
  // DEV_IDENTITY signs in everyone on this machine, so it must never run on a public address.
  const host = (() => { try { return new URL(publicUrl).hostname; } catch { return ''; } })();
  if (host !== 'localhost' && host !== '127.0.0.1') throw new Error(`DEV_IDENTITY needs a localhost PUBLIC_URL, not ${publicUrl}`);
}
// TAILNET_IDENTITY=1: reached only through `tailscale serve` on this machine. The server must then listen on
// 127.0.0.1, or anyone who can reach the port could claim any email.
const tailnet = env.TAILNET_IDENTITY === '1';
if (tailnet) {
  if (devIdentity) throw new Error('DEV_IDENTITY and TAILNET_IDENTITY cannot run together');
  if (env.HOST !== '127.0.0.1') throw new Error('TAILNET_IDENTITY needs HOST=127.0.0.1');
}

const db = openDb(env.DB_PATH ?? 'cavecad.sqlite');
const passKey = createPrivateKey({ key: JSON.parse(readFileSync(need('PASS_KEY_FILE'), 'utf8')), format: 'jwk' });
const adminEmail = need('ADMIN_EMAIL');
const mailer = { url: env.MAILER_URL ?? '', key: env.MAILER_KEY ?? '' };
const secure = publicUrl.startsWith('https:');
const whoIs = makeWhoIs({ db, tailnet, devIdentity });
const deps = {
  db, passKey, adminEmail, publicUrl, mailer, tailnet, whoIs,
  scansDir: resolve(env.SCANS_DIR ?? join(dirname(resolve(env.DB_PATH ?? 'cavecad.sqlite')), 'scans')),
};
const signin = {
  db, publicUrl, secure,
  codes: { db, secret: codeSecret, send: (to: string, code: string) => sendCode(mailer, { to, code }), mayGetIn: (e: string) => mayGetIn(db, e, { adminEmail, now: Date.now() }) },
  google: env.GOOGLE_CLIENT_ID ? { clientId: env.GOOGLE_CLIENT_ID, clientSecret: need('GOOGLE_CLIENT_SECRET'), redirectUri: publicUrl.replace(/\/$/, '') + '/auth/google/callback', stateSecret: codeSecret } : null,
};
const downloads = { db, whoIs, adminEmail, dir: env.DOWNLOADS_DIR ?? '' };
const dist = resolve(env.DIST_DIR ?? '../web/dist');

createServer((req, res) => {
  const path = (req.url ?? '/').split('?')[0];
  const p = (async () => {
    if (path === '/signin' || path.startsWith('/signin/') || path.startsWith('/auth/') || path === '/signout') {
      if (await handleSignin(signin, req, res)) return;
    }
    if (path.startsWith('/downloads/') || path === '/api/downloads') return handleDownloads(downloads, req, res);
    if (path.startsWith('/api/')) return handleApi(deps, req, res);
    return serveApp(deps, dist, req, res);
  })();
  p.catch((e) => {
    console.error(e);
    if (!res.headersSent) res.writeHead(500, { 'Content-Type': 'application/json' }).end('{"error":"server"}');
  });
}).listen(Number(env.PORT ?? 8787), env.HOST ?? '0.0.0.0', () => console.log(`CaveCAD server on ${env.HOST ?? '0.0.0.0'}:${env.PORT ?? 8787}`));
