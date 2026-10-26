// Small HTTP helpers shared by the account API and the rooms API: JSON answers and JSON bodies with a size limit.
import type { IncomingMessage, ServerResponse } from 'node:http';

/** Answers with JSON, never cached. */
export function send(res: ServerResponse, status: number, body: unknown): void {
  res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
  res.end(JSON.stringify(body));
}

/** A body that is too big or not a JSON object; answered 400. */
export class BadBody extends SyntaxError {}

/** Answers 400 for a body readJson refused; a too-big one also closes the connection, so its rest is never read. */
export function sendBadBody(res: ServerResponse, e: unknown): void {
  if (e instanceof BadBody && e.message === 'too-big') res.setHeader('Connection', 'close');
  send(res, 400, { error: 'bad-request' });
}

/** Reads a JSON object body of at most `max` bytes (64 KB by default); one declared bigger is refused unread. */
export async function readJson(req: IncomingMessage, max = 64 * 1024): Promise<Record<string, unknown>> {
  if (Number(req.headers['content-length'] ?? 0) > max) throw new BadBody('too-big');
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of req) {
    const b = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    size += b.length;
    if (size > max) throw new BadBody('too-big');
    chunks.push(b);
  }
  const s = Buffer.concat(chunks).toString('utf8');
  if (!s) return {};
  const v: unknown = JSON.parse(s);
  if (typeof v !== 'object' || v === null || Array.isArray(v)) throw new BadBody('not-an-object');
  return v as Record<string, unknown>;
}
