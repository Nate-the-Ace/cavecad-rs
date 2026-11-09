// CaveCAD's sign-in: a small page (no app in it) with Google and an emailed code, the routes behind it, and
// sign-out. Signing in ends in a web session cookie, or, for the desktop app, a hand-off code it redeems.
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Db } from './db.ts';
import type { Identity } from './members.ts';
import { send, sendBadBody, readJson } from './http.ts';
import { requestCode, verifyCode, type CodeDeps } from './email-code.ts';
import { googleStart, googleFinish, clearOauthCookie, safeReturn, validDesktop, type GoogleConfig, type Desktop } from './google.ts';
import { createSession, revokeSession, sessionCookie, clearSessionCookie, readCookie, SESSION_COOKIE } from './sessions.ts';
import { createHandoff } from './handoff.ts';

export type SigninDeps = { db: Db; publicUrl: string; secure: boolean; codes: CodeDeps; google: GoogleConfig | null; now?: () => number };

/** Where to go once signed in, plus the cookie to set (none for the desktop). */
function finish(d: SigninDeps, who: Identity, returnTo: string, desktop: Desktop, now: number): { next: string; cookie: string | null } {
  if (desktop) {
    const code = createHandoff(d.db, who, desktop.challenge, now);
    return { next: `cavecad://auth?code=${encodeURIComponent(code)}&state=${encodeURIComponent(desktop.state)}`, cookie: null };
  }
  return { next: safeReturn(returnTo), cookie: sessionCookie(createSession(d.db, who, 'web', now), d.secure) };
}

/** JSON posts from this site only (a plain form or another site cannot send them). */
function crossSite(req: IncomingMessage): boolean {
  const type = String(req.headers['content-type'] ?? '').toLowerCase();
  const site = req.headers['sec-fetch-site'];
  return !type.startsWith('application/json') || (site !== undefined && site !== 'same-origin' && site !== 'none');
}

/**
 * The visitor's address, for the code-request limits. The edge (Caddy) appends the address it saw to
 * X-Forwarded-For, so only the last entry is the edge's word; earlier ones are whatever the client sent.
 */
export function clientIp(req: IncomingMessage): string {
  const xff = req.headers['x-forwarded-for'];
  const last = String(Array.isArray(xff) ? xff[xff.length - 1] : xff ?? '').split(',').pop()?.trim();
  return last || (req.socket.remoteAddress ?? '');
}

/** Handles a sign-in route; false when the path is not one. */
export async function handleSignin(d: SigninDeps, req: IncomingMessage, res: ServerResponse): Promise<boolean> {
  const url = new URL(req.url ?? '/', 'http://x');
  const path = url.pathname;
  const method = req.method ?? 'GET';
  const now = (d.now ?? Date.now)();
  try {
    if (path === '/signin' && method === 'GET') {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' }).end(page(url.searchParams, !!d.google));
      return true;
    }
    if ((path === '/signin/code' || path === '/signin/verify' || path === '/signout') && method === 'POST' && crossSite(req)) return send(res, 403, { error: 'cross-site' }), true;
    if (path === '/signin/code' && method === 'POST') {
      const b = await readJson(req);
      const r = await requestCode(d.codes, b.email, clientIp(req), now);
      return send(res, r.ok ? 200 : 400, r.ok ? { ok: true } : { error: r.error }), true;
    }
    if (path === '/signin/verify' && method === 'POST') {
      const b = await readJson(req);
      const r = verifyCode(d.codes, b.email, b.code, now);
      if (!r.ok) return send(res, 400, r.error === 'wrong' ? { error: 'wrong', triesLeft: r.triesLeft } : { error: 'expired' }), true;
      const f = finish(d, { email: r.email, name: r.email.split('@')[0], provider: 'email', subject: 'session' }, String(b.return ?? '/'), validDesktop(b), now);
      if (f.cookie) res.setHeader('Set-Cookie', f.cookie);
      return send(res, 200, { next: f.next }), true;
    }
    if (path === '/auth/google' && method === 'GET') {
      if (!d.google) return res.writeHead(302, { Location: '/signin' }).end(), true;
      const p = Object.fromEntries(url.searchParams);
      const s = googleStart(d.google, { returnTo: p.return ?? '/', desktop: validDesktop(p) }, now);
      res.writeHead(302, { Location: s.location, 'Set-Cookie': s.cookie, 'Cache-Control': 'no-store' }).end();
      return true;
    }
    if (path === '/auth/google/callback' && method === 'GET') {
      if (!d.google) return res.writeHead(302, { Location: '/signin' }).end(), true;
      const r = await googleFinish(d.google, url.searchParams, req.headers.cookie, now);
      if (!r.ok) {
        res.writeHead(302, { Location: `/signin?error=${r.error === 'unverified' ? 'unverified' : 'google'}`, 'Set-Cookie': clearOauthCookie, 'Cache-Control': 'no-store' }).end();
        return true;
      }
      const f = finish(d, r.who, r.returnTo, r.desktop, now);
      res.writeHead(302, { Location: r.desktop ? `/signin?done=desktop&next=${encodeURIComponent(f.next)}` : f.next, 'Set-Cookie': [clearOauthCookie, ...(f.cookie ? [f.cookie] : [])], 'Cache-Control': 'no-store' }).end();
      return true;
    }
    if (path === '/signout') {
      if (method !== 'POST' && method !== 'GET') return false;
      revokeSession(d.db, readCookie(req.headers.cookie, SESSION_COOKIE), now);
      if (method === 'POST') {
        res.setHeader('Set-Cookie', clearSessionCookie(d.secure));
        return send(res, 200, { ok: true }), true;
      }
      res.writeHead(302, { Location: '/signin', 'Set-Cookie': clearSessionCookie(d.secure), 'Cache-Control': 'no-store' }).end();
      return true;
    }
  } catch (e) {
    if (e instanceof SyntaxError) return sendBadBody(res, e), true;
    throw e;
  }
  return false;
}

const esc = (t: string) => t.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
const ERRORS: Record<string, string> = {
  google: "Google sign-in didn't finish. Try again.",
  unverified: "Google hasn't confirmed that email. Use Email me a code instead.",
};
/** JSON for inside a <script>: no '<' survives, so nothing can close the tag. */
const jsonInScript = (v: unknown) => JSON.stringify(v).replace(/</g, '\\u003c');

/** The sign-in page: plain HTML and a little script; the desktop hand-off parameters ride along. */
function page(q: URLSearchParams, google: boolean): string {
  const keep = new URLSearchParams();
  for (const k of ['return', 'desktop', 'challenge', 'state']) if (q.get(k)) keep.set(k, q.get(k)!);
  // Only a cavecad://auth link may be offered here: anything else in ?next= would be a script-injection hole.
  const nextLink = q.get('next') ?? '';
  const done = q.get('done') === 'desktop' && /^cavecad:\/\/auth\?[A-Za-z0-9_%=&.-]+$/.test(nextLink) ? nextLink : '';
  const err = ERRORS[q.get('error') ?? ''] ?? '';
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Sign in to CaveCAD</title><link rel="icon" href="/favicon.svg"><link rel="apple-touch-icon" href="/apple-touch-icon.png">
<style>
:root{color-scheme:light dark;--bg:#f7f6f2;--card:#fff;--text:#1c2733;--muted:#5b6670;--line:#d9d6cc;--accent:#e8590c}
@media (prefers-color-scheme:dark){:root{--bg:#14181d;--card:#1c2228;--text:#e7e9ec;--muted:#9aa4ae;--line:#2c343c}}
*{box-sizing:border-box}body{margin:0;min-height:100vh;display:grid;place-items:center;padding:16px;background:var(--bg);color:var(--text);font:16px/1.5 system-ui,sans-serif}
.card{width:min(400px,100%);background:var(--card);border:1px solid var(--line);border-radius:14px;padding:24px;display:flex;flex-direction:column;gap:12px}
h1{margin:0;font-size:20px}p{margin:0}.muted{color:var(--muted);font-size:14px}.bad{color:#c92a2a;font-size:14px}
input{font:inherit;padding:10px 12px;border:1px solid var(--line);border-radius:8px;background:transparent;color:inherit;width:100%}
button,.btn{font:inherit;padding:10px 12px;border-radius:8px;border:1px solid var(--line);background:transparent;color:inherit;cursor:pointer;text-align:center;text-decoration:none;display:block;width:100%}
.primary{background:var(--accent);border-color:var(--accent);color:#fff}.or{text-align:center;color:var(--muted);font-size:13px}[hidden]{display:none!important}
#code{letter-spacing:.4em;text-align:center;font-size:22px}
</style></head><body><main class="card">
<img src="/favicon.svg" alt="" width="48" height="48">
${done ? `<h1>You're signed in</h1><p>You can go back to CaveCAD now.</p><a class="btn primary" href="${esc(done)}">Open CaveCAD</a><script>location.href=${jsonInScript(done)}</script>` : `
<h1>Sign in to CaveCAD</h1>
${err ? `<p class="bad" role="alert">${esc(err)}</p>` : ''}
${google ? `<a class="btn" href="/auth/google?${esc(keep.toString())}">Continue with Google</a><p class="or">or</p>` : ''}
<form id="f1"><label class="muted" for="email">Your email</label><input id="email" type="email" autocomplete="email" required><button class="primary" style="margin-top:8px">Email me a code</button></form>
<form id="f2" hidden><p id="sentto"></p><input id="code" inputmode="numeric" autocomplete="one-time-code" maxlength="6" pattern="[0-9]{6}" required><button class="primary" style="margin-top:8px">Sign in</button><button type="button" id="again" style="margin-top:8px">Send a new code</button></form>
<p id="msg" class="bad" role="alert"></p>
<p class="muted">CaveCAD is invite only. <a href="/privacy.html">Privacy</a></p>
<script>
const keep=${jsonInScript(Object.fromEntries(keep))};
const $=(id)=>document.getElementById(id);const msg=(t)=>{$('msg').textContent=t||''};
const post=async(p,b)=>{const r=await fetch(p,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(b)});return{ok:r.ok,body:await r.json().catch(()=>({}))}};
async function ask(){msg('');const email=$('email').value.trim();const r=await post('/signin/code',{email});
if(!r.ok){msg('That doesn\\'t look like an email address.');return}
$('f1').hidden=true;$('f2').hidden=false;$('sentto').textContent='If '+email+' can use CaveCAD, a 6-digit code is on its way. It works for 10 minutes.';$('code').focus()}
$('f1').onsubmit=(e)=>{e.preventDefault();ask()};
$('again').onclick=()=>{$('f2').hidden=true;$('f1').hidden=false;ask()};
$('f2').onsubmit=async(e)=>{e.preventDefault();msg('');const r=await post('/signin/verify',{email:$('email').value.trim(),code:$('code').value,...keep});
if(r.ok){if(r.body.next.startsWith('cavecad:')){document.querySelector('main').innerHTML='<h1>You\\'re signed in</h1><p>You can go back to CaveCAD now.</p>'}location.href=r.body.next;return}
msg(r.body.error==='wrong'?'That code isn\\'t right. '+r.body.triesLeft+(r.body.triesLeft===1?' try':' tries')+' left.':'That code has expired. Ask for a new one.')};
</script>`}
</main></body></html>`;
}
