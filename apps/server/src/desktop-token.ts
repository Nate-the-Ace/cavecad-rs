// Exchanges the desktop app's sign-in code (or a refresh token) with TinyAuth's OIDC provider. Done here, not
// in the app, so the client secret stays on the server and the app's webview needs no CORS from TinyAuth.
export const DESKTOP_REDIRECT = 'cavecad://auth';

type Discovery = { authorization_endpoint?: string; token_endpoint?: string };

/** Reads the issuer's discovery document once (a failure is not kept, so the next call tries again). */
function discovery(issuer: string, f: typeof fetch): () => Promise<Discovery> {
  let kept: Discovery | null = null;
  return async () => {
    if (kept) return kept;
    const res = await f(issuer.replace(/\/$/, '') + '/.well-known/openid-configuration', { signal: AbortSignal.timeout(5000) });
    if (res.ok === false) throw new Error('discovery-failed');
    kept = (await res.json()) as Discovery;
    return kept;
  };
}

/** Builds the `/api/desktop-config` answer: where the desktop app sends the browser to sign in. The app asks
 *  the CaveCAD server, never TinyAuth, so its webview needs no CORS from TinyAuth. Null when TinyAuth can't be read. */
export function makeDesktopConfig(c: { issuer: string; clientId: string; fetch?: typeof fetch }) {
  const read = discovery(c.issuer, c.fetch ?? fetch);
  return async (): Promise<{ authorizationEndpoint: string; clientId: string } | null> => {
    try {
      const d = await read();
      return d.authorization_endpoint ? { authorizationEndpoint: d.authorization_endpoint, clientId: c.clientId } : null;
    } catch {
      return null;
    }
  };
}

/** Builds the `/api/desktop-token` handler. */
export function makeDesktopToken(c: { issuer: string; clientId: string; clientSecret: string; fetch?: typeof fetch }) {
  const f = c.fetch ?? fetch;
  const read = discovery(c.issuer, f);
  return async (body: Record<string, string>): Promise<{ status: number; body: unknown }> => {
    const form = new URLSearchParams({ client_id: c.clientId, client_secret: c.clientSecret });
    if (body.code && body.verifier) {
      form.set('grant_type', 'authorization_code');
      form.set('code', body.code);
      form.set('code_verifier', body.verifier);
      form.set('redirect_uri', DESKTOP_REDIRECT);
    } else if (body.refresh) {
      form.set('grant_type', 'refresh_token');
      form.set('refresh_token', body.refresh);
    } else return { status: 400, body: { error: 'bad-request' } };
    try {
      const endpoint = (await read()).token_endpoint;
      if (!endpoint) return { status: 400, body: { error: 'sign-in-failed' } };
      const res = await f(endpoint, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: form.toString(), signal: AbortSignal.timeout(10_000) });
      const out = (await res.json()) as { id_token?: string; refresh_token?: string; expires_in?: number };
      if (!res.ok || !out.id_token) return { status: 400, body: { error: 'sign-in-failed' } };
      return { status: 200, body: { id_token: out.id_token, refresh_token: out.refresh_token ?? null, expires_in: out.expires_in ?? 0 } };
    } catch {
      return { status: 400, body: { error: 'sign-in-failed' } };
    }
  };
}
