// The handshake that a class link starts, as two small loops over a mailbox. Neither knows anything about
// WebRTC: the page gives each the two things it can do (make an offer / take an answer, and answer an offer),
// so the whole exchange is checked here with a memory mailbox and pretend connections.
//
//   STUDENT  leaves `hello:ME`, waits for `offer:ME`, answers it, leaves `answer:ME`.
//   TEACHER  sees each new hello, makes an offer for that student alone, leaves `offer:ME`, and when
//            `answer:ME` turns up hands it to that offer.
//
// The class link is only the session id, so every student gets the same link; the student's own id
// (`ME`) is what tells their notes from everyone else's.

import type { Mailbox } from './signal';

export const POLL_MS = 800;
/** The host looks for new hellos more often: it is one caller, and a student should not wait on it. */
export const HOST_POLL_MS = 600;
const STUDENT_PATIENCE_MS = 90_000;
/** Mailbox errors in a row that the teacher's loop puts up with before saying so. */
const TEACHER_PATIENCE = 5;

const nap = (ms: number, signal?: AbortSignal) =>
  new Promise<void>((resolve) => {
    const t = setTimeout(resolve, ms);
    signal?.addEventListener('abort', () => (clearTimeout(t), resolve()), { once: true });
  });

/** A fresh id for one student's attempt. */
export const attemptId = (): string => Math.random().toString(36).slice(2, 12).padEnd(10, 'x');

export interface HostHooks {
  /** Makes an offer for one student: the invite's id and the code to hand over. Null if it cannot. */
  offer(): Promise<{ id: string; code: string } | null>;
  /** Gives a student's answer to the offer that was made for them. A sentence if it did not work, else null. */
  reply(id: string, code: string): Promise<string | null>;
  /** The mailbox could not be reached (say so once; the loop keeps trying). */
  trouble?(message: string): void;
  /** A student was handled (for tests and the roster). */
  seen?(attempt: string, what: 'offered' | 'connected' | 'refused'): void;
}

/** The teacher's side: watches the mailbox for students and walks each through the handshake. Returns the way to stop.
 *  Hellos and answers are watched by two loops of their own: each mailbox call takes a second or so, and one loop doing
 *  both would make an answer wait for the whole round of hellos before it was even looked for. */
export function hostLoop(box: Mailbox, session: string, hooks: HostHooks, pollMs = HOST_POLL_MS): () => void {
  const abort = new AbortController();
  const offered = new Map<string, string>(); // attempt id -> invite id
  const done = new Set<string>();
  /** Tidying the mailbox is not on anyone's way: it is done in the background and a failure does not matter. */
  const tidy = (p: Promise<void>) => void p.catch(() => undefined);

  async function hellos() {
    // New hellos: an offer for each, all at once (a class arrives together).
    const fresh = (await box.list(session, 'hello:')).map((n) => n.name.slice('hello:'.length)).filter((id) => !offered.has(id));
    for (const id of fresh) offered.set(id, ''); // claimed now, so the next pass does not make a second offer
    await Promise.all(
      fresh.map(async (id) => {
        const made = await hooks.offer();
        if (!made) return void offered.delete(id);
        offered.set(id, made.id);
        await box.put(session, `offer:${id}`, made.code);
        tidy(box.del(session, `hello:${id}`));
        hooks.seen?.(id, 'offered');
      })
    );
  }

  async function answers() {
    for (const note of await box.list(session, 'answer:')) {
      const id = note.name.slice('answer:'.length);
      const invite = offered.get(id);
      if (!invite || done.has(id)) continue;
      done.add(id);
      const problem = await hooks.reply(invite, note.value);
      hooks.seen?.(id, problem ? 'refused' : 'connected');
      tidy(box.del(session, `answer:${id}`));
      tidy(box.del(session, `offer:${id}`));
    }
  }

  let told = false; // one report for both loops
  function watch(pass: () => Promise<void>) {
    let bad = 0;
    (async () => {
      while (!abort.signal.aborted) {
        try {
          await pass();
          bad = 0;
          told = false;
        } catch (e) {
          if (++bad >= TEACHER_PATIENCE && !told) {
            told = true;
            hooks.trouble?.(e instanceof Error ? e.message : String(e));
          }
        }
        await nap(pollMs, abort.signal);
      }
    })();
  }
  watch(hellos);
  watch(answers);
  return () => abort.abort();
}

export interface JoinHooks {
  /** Answers the teacher's offer: the answer code to send back, or a sentence saying why it cannot be used. */
  answer(offer: string): Promise<{ code: string } | { error: string }>;
}

/** The student's side. Resolves to null when the answer has been left for the teacher (the connection then opens by
 *  itself), or to a sentence saying what went wrong. */
export async function joinLoop(box: Mailbox, session: string, hooks: JoinHooks, opts: { pollMs?: number; patienceMs?: number; id?: string; signal?: AbortSignal } = {}): Promise<string | null> {
  const id = opts.id ?? attemptId();
  const pollMs = opts.pollMs ?? POLL_MS;
  const until = Date.now() + (opts.patienceMs ?? STUDENT_PATIENCE_MS);
  try {
    await box.put(session, `hello:${id}`, '1');
    let offer: string | null = null;
    while (offer === null) {
      if (opts.signal?.aborted) return 'Cancelled.';
      if (Date.now() > until) return 'The teacher’s page did not answer. Check that the class is running and that you opened the newest link.';
      offer = await box.get(session, `offer:${id}`);
      if (offer === null) await nap(pollMs, opts.signal);
    }
    const made = await hooks.answer(offer);
    if ('error' in made) return made.error;
    await box.put(session, `answer:${id}`, made.code);
    return null;
  } catch (e) {
    return e instanceof Error ? e.message : String(e);
  }
}
