import { readJoinLink } from '../src/lib/classroom-log.ts';
import { createRequire } from 'node:module';
import { httpMailbox, memoryMailbox, type Mailbox } from '../src/lib/signal.ts';
import { hostLoop, joinLoop } from '../src/lib/signal-handshake.ts';

const require = createRequire(import.meta.url);
const SignalLogic = require('../../../tools/signal-mailbox/Logic.gs');

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** The Apps Script mailbox, run in this process: the real decisions in Logic.gs behind a fetch. */
function appsScript(key = 'K', options: { failAfter?: number } = {}): typeof fetch {
  const cache = new Map<string, { v: string; until: number }>();
  let calls = 0;
  const env = () => ({
    cache: {
      get: (k: string) => { const e = cache.get(k); return e && e.until > Date.now() ? e.v : null; },
      put: (k: string, v: string, s: number) => void cache.set(k, { v, until: Date.now() + s * 1000 }),
      getAll: (keys: string[]) => Object.fromEntries(keys.flatMap((k) => { const e = cache.get(k); return e && e.until > Date.now() ? [[k, e.v]] : []; })),
      remove: (k: string) => void cache.delete(k),
    },
    expected: key,
    now: new Date(),
    locked: (f: () => void) => f(),
  });
  return (async (input: RequestInfo | URL, init?: RequestInit) => {
    if (options.failAfter !== undefined && ++calls > options.failAfter) throw new Error('offline');
    const u = new URL(String(input));
    const p = u.searchParams;
    const out = SignalLogic.run(env(), { op: p.get('op'), key: p.get('k'), session: p.get('s'), name: p.get('n'), prefix: p.get('p'), body: init?.method === 'POST' ? String(init.body ?? '') : '' });
    return { json: async () => out } as Response;
  }) as typeof fetch;
}

// a handshake with three students arriving at once, over a mailbox
async function handshake(box: Mailbox, label: string) {
  const session = 'classsession1';
  let made = 0;
  const replies: Record<string, string> = {};
  const seen: string[] = [];
  const stop = hostLoop(
    box,
    session,
    {
      offer: async () => { const id = `inv${++made}`; return { id, code: `OFFER-${id}` }; },
      reply: async (id, code) => { replies[id] = code; return null; },
      seen: (a, what) => seen.push(`${what}`),
    },
    10
  );
  const students = ['aaaa1', 'bbbb2', 'cccc3'];
  const results = await Promise.all(
    students.map((id) =>
      joinLoop(box, session, { answer: async (offer) => ({ code: `ANSWER-to-${offer}` }) }, { id, pollMs: 10, patienceMs: 3000 })
    )
  );
  await wait(120);
  stop();
  ok(results.every((r) => r === null), `${label}: every student left an answer (${results})`);
  ok(made === 3, `${label}: one offer made per student, not more (${made})`);
  ok(Object.keys(replies).length === 3 && Object.values(replies).every((c) => /^ANSWER-to-OFFER-inv\d$/.test(c)), `${label}: each answer went to the offer made for that student: ${JSON.stringify(replies)}`);
  ok(new Set(Object.values(replies)).size === 3, `${label}: no two students got the same offer`);
  ok(seen.filter((s) => s === 'connected').length === 3, `${label}: all three handed to their connections`);
  ok((await box.list(session, 'hello:')).length === 0 && (await box.list(session, 'offer:')).length === 0 && (await box.list(session, 'answer:')).length === 0, `${label}: the mailbox is tidied up after`);
}
await handshake(memoryMailbox(), 'memory');
await handshake(httpMailbox('https://script.example/exec', 'K', appsScript('K')), 'apps script contract');

// a wrong key is said in words, and nothing is stored
{
  const box = httpMailbox('https://script.example/exec', 'WRONG', appsScript('K'));
  let said = '';
  await box.put('classsession1', 'hello:abcd1', '1').catch((e: Error) => (said = e.message));
  ok(said.includes('did not accept'), `a wrong key: ${said}`);
  const r = await joinLoop(box, 'classsession1', { answer: async () => ({ error: 'x' }) }, { pollMs: 5, patienceMs: 100 });
  ok(!!r && r.includes('did not accept'), `a student is told why: ${r}`);
}

// a teacher that never comes
{
  const r = await joinLoop(memoryMailbox(), 'classsession1', { answer: async () => ({ code: 'x' }) }, { pollMs: 5, patienceMs: 80 });
  ok(!!r && r.includes('did not answer'), `a student waits, then says so: ${r}`);
}

// an offer that cannot be used is the student's to hear about, and no answer is left
{
  const box = memoryMailbox();
  await box.put('classsession1', 'offer:zzzz9', 'BROKEN');
  const r = await joinLoop(box, 'classsession1', { answer: async () => ({ error: 'That invite could not be used.' }) }, { id: 'zzzz9', pollMs: 5 });
  ok(r === 'That invite could not be used.' && (await box.list('classsession1', 'answer:')).length === 0, `a bad offer: ${r}`);
}

// the teacher's loop keeps going through a mailbox that drops out, and says so once
{
  const box = httpMailbox('https://script.example/exec', 'K', appsScript('K', { failAfter: 0 }));
  const said: string[] = [];
  const stop = hostLoop(box, 'classsession1', { offer: async () => null, reply: async () => null, trouble: (m) => said.push(m) }, 5);
  await wait(150);
  stop();
  ok(said.length === 1 && said[0].includes('Could not reach'), `the teacher is told once: ${said}`);
}

// the teacher can stop, and an offer that fails to be made lets the student be tried again
{
  const box = memoryMailbox();
  let tries = 0;
  const stop = hostLoop(box, 'classsession1', { offer: async () => (++tries < 3 ? null : { id: 'inv', code: 'OFFER' }), reply: async () => null }, 5);
  await box.put('classsession1', 'hello:late1', '1');
  await wait(150);
  stop();
  ok(tries >= 3 && (await box.get('classsession1', 'offer:late1')) === 'OFFER', `an offer that failed is tried again (${tries})`);
  const before = tries;
  await wait(60);
  ok(tries === before, 'a stopped loop makes no more offers');
}

// a pasted join link: a whole address, the part after the ?, or just the session code
{
  const sess = 'cc805973baf84c7baebf';
  const a = readJoinLink(`https://nate-the-ace.github.io/cavecad-rs/?live=${sess}`);
  ok(a?.session === sess && a.mailbox === null, 'a whole address gives its session');
  const b = readJoinLink(`http://localhost:5174/?live=${sess}&mb=${encodeURIComponent('https://x.example/exec')}&mk=KEY`);
  ok(b?.session === sess && b.mailbox?.endpoint === 'https://x.example/exec' && b.mailbox.key === 'KEY', 'a link that carries its mailbox gives that too');
  ok(readJoinLink(`  ?live=${sess} `)?.session === sess, 'just the part after the question mark works, spaces and all');
  ok(readJoinLink(sess)?.session === sess, 'the bare session code works');
  const app = readJoinLink(`cavecad://join?live=${sess}&mb=${encodeURIComponent('https://x.example/exec')}&mk=KEY`);
  ok(app?.session === sess && app.mailbox?.key === 'KEY', 'a link that opens the desktop app (cavecad://) reads the same way');
  ok(readJoinLink('hello there') === null && readJoinLink('') === null && readJoinLink('https://example.com/') === null, 'anything else is not a join link');
}

if (bad) process.exit(1);
console.log('signal ok');
