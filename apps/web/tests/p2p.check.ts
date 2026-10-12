import { decodeCode, encodeCode } from '../src/lib/p2p-code.ts';
import { Reassembler, frames } from '../src/lib/p2p-wire.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

const sdp = 'v=0\r\no=- 4611731400430051336 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=group:BUNDLE 0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\nc=IN IP4 0.0.0.0\r\na=candidate:1 1 udp 2113937151 192.168.1.20 54321 typ host\r\na=fingerprint:sha-256 AA:BB:CC\r\n'.repeat(6);
const code = await encodeCode({ kind: 'invite', sdp, sid: 'abc123xyz' });
ok(code.startsWith('CAVE1-') && /^[A-Za-z0-9_-]+$/.test(code.slice(6)), 'a code is one run of safe characters');
ok(code.length < sdp.length, 'it is squeezed: ' + code.length + ' vs ' + sdp.length);
const back = await decodeCode(code);
ok(back?.kind === 'invite' && back.sdp === sdp && back.sid === 'abc123xyz', 'it reads back exactly');
const wrapped = await decodeCode(`Here you go:\n  "${code.slice(0, 30)}\n${code.slice(30)}"  thanks!`);
ok(wrapped?.sid === 'abc123xyz', 'spaces, line breaks and words around it are fine');
ok((await decodeCode('hello there')) === null && (await decodeCode('CAVE1-!!!')) === null && (await decodeCode('CAVE1-AAAA')) === null, 'anything else is refused');
const answer = await decodeCode(await encodeCode({ kind: 'answer', sdp, sid: 'abc123xyz' }));
ok(answer?.kind === 'answer', 'answers are told from invites');

// Frames: a message of any size goes through in pieces and comes out whole.
const big = 'x'.repeat(100_000) + '😀é\n"quoted"';
const fs = frames(big, 7, 14_000);
ok(fs.length === 8 && fs.every((f) => f.length < 15_000 + 100), 'cut to frames: ' + fs.length);
const r = new Reassembler();
let out: string | null = null;
for (const f of fs) out = r.push(f);
ok(out === big, 'and put back together, byte for byte');
const small = frames('{"t":"ops"}', 1);
ok(small.length === 1 && new Reassembler().push(small[0]) === '{"t":"ops"}', 'a small message is one frame');
const mix = new Reassembler();
const a = frames('A'.repeat(30_000), 1, 10_000), b = frames('B'.repeat(20_000), 2, 10_000);
const got: string[] = [];
for (const f of [a[0], b[0], a[1], b[1], a[2]]) { const m = mix.push(f); if (m) got.push(m[0] + m.length); }
ok(got.join() === 'B20000,A30000', 'interleaved messages keep apart: ' + got.join());
ok(new Reassembler().push('not json') === null && new Reassembler().push(JSON.stringify({ c: 1, i: 5, n: 2, d: '' })) === null, 'junk and impossible frames are dropped');
const dropped = new Reassembler();
dropped.push(a[0]);
ok(dropped.push(a[2]) === null, 'a missing frame drops the message, never a guess');

if (bad) process.exit(1);
console.log('p2p ok');
