// Exchanges the desktop app's sign-in code with TinyAuth (filled in by Task 11).
export function makeDesktopToken(_c: { issuer: string; clientId: string; clientSecret: string }) {
  return async (_body: Record<string, string>) => ({ status: 404, body: { error: 'not-found' } });
}
