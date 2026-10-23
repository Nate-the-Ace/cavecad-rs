// node tools/invite-mailer/test.mjs -- checks Logic.gs.
import { createRequire } from 'node:module';
const L = createRequire(import.meta.url)('./Logic.gs');
let bad = 0;
const ok = (c, m) => { if (!c) { bad++; console.log('FAIL', m); } };
const site = 'https://cave.schonegg.stream';
const good = { key: 'K', expected: 'K', to: 'rick@example.com', link: site + '/?signup=abc', site, countThisHour: 0 };
ok(L.check(good) === null, 'a good request passes');
ok(L.check({ ...good, key: 'x' }) === 'key', 'wrong key');
ok(L.check({ ...good, expected: '' }) === 'key', 'no key set');
ok(L.check({ ...good, to: 'nope' }) === 'address', 'bad address');
ok(L.check({ ...good, link: 'https://evil.example/?signup=abc' }) === 'link', 'foreign link');
ok(L.check({ ...good, countThisHour: 40 }) === 'busy', 'hourly cap');
const m = L.message({ inviter: 'Nathan', to: 'rick@example.com', link: good.link });
ok(m.subject === 'Nathan invited you to CaveCAD', 'subject');
ok(m.body.includes('rick@example.com') && m.body.includes('7 days') && m.body.includes(good.link), 'body');
console.log(bad ? `invite-mailer: ${bad} failed` : 'invite-mailer: all ok');
process.exit(bad ? 1 : 0);
