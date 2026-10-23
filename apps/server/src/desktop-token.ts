// Exchanges the desktop app's sign-in code (or a refresh token) with TinyAuth's OIDC provider. Done here, not
// in the app, so the client secret stays on the server and the app's webview needs no CORS from TinyAuth.
export const DESKTOP_REDIRECT = 'cavecad://auth';

/** Builds the `/api/desktop-token` handler. */
export function makeDesktopToken(c: { issuer: string; clientId: string; clientSecret: string; fetch?: typeof fetch }) {
  const f = c.fetch ?? fetch;
  let endpoint = '';
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
      if (!endpoint) endpoint = ((await (await f(c.issuer.replace(/\/$/, '') + '/.well-known/openid-configuration')).json()) as { token_endpoint: string }).token_endpoint;
      const res = await f(endpoint, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: form.toString() });
      const out = (await res.json()) as { id_token?: string; refresh_token?: string; expires_in?: number };
      if (!res.ok || !out.id_token) return { status: 400, body: { error: 'sign-in-failed' } };
      return { status: 200, body: { id_token: out.id_token, refresh_token: out.refresh_token ?? null, expires_in: out.expires_in ?? 0 } };
    } catch {
      return { status: 400, body: { error: 'sign-in-failed' } };
    }
  };
}
