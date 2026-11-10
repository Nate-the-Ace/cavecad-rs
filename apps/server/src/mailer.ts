// Sends an invite email through the Apps Script in tools/invite-mailer. A failure never stops the invite:
// the admin is told and can copy the link instead.
export type MailerConfig = { url: string; key: string; fetch?: typeof fetch; timeoutMs?: number };

/** True when the email went. */
export async function sendInvite(c: MailerConfig, p: { to: string; inviter: string; link: string }): Promise<boolean> {
  if (!c.url || !c.key) return false;
  try {
    const res = await (c.fetch ?? fetch)(c.url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ key: c.key, ...p }), signal: AbortSignal.timeout(c.timeoutMs ?? 10_000) });
    const out = (await res.json()) as { ok?: boolean };
    return out.ok === true;
  } catch {
    return false;
  }
}

/** Emails a sign-in code. True when the email went. */
export async function sendCode(c: MailerConfig, p: { to: string; code: string }): Promise<boolean> {
  if (!c.url || !c.key) return false;
  try {
    const res = await (c.fetch ?? fetch)(c.url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ key: c.key, kind: 'code', ...p }), signal: AbortSignal.timeout(c.timeoutMs ?? 10_000) });
    return ((await res.json()) as { ok?: boolean }).ok === true;
  } catch {
    return false;
  }
}
