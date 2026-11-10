import { isOldJoinUrl, isOldLiveLink, withoutOldLink } from '../src/lib/moved-link.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

ok(isOldLiveLink('?live=abc'), 'a live link');
ok(isOldLiveLink('?class=abc&invite=x'), 'a class link');
ok(isOldLiveLink('?join=room&invite=x'), 'a join link');
ok(!isOldLiveLink('') && !isOldLiveLink('?x=1') && !isOldLiveLink('?live='), 'other addresses are not');
ok(isOldJoinUrl('cavecad://join?live=abc'), 'a desktop join link');
ok(isOldJoinUrl('cavecad://join/?s=abc'), 'a desktop join link with a slash');
ok(!isOldJoinUrl('cavecad://signin?code=1') && !isOldJoinUrl('https://x.example/?y=1') && !isOldJoinUrl('not a url'), 'other links are not');
ok(isOldJoinUrl('https://x.example/app/?live=abc') === false, 'a web address is handled by isOldLiveLink, not here');
ok(withoutOldLink('/app/', '?live=abc&mb=e&mk=k', '#top') === '/app/#top', withoutOldLink('/app/', '?live=abc&mb=e&mk=k', '#top'));
ok(withoutOldLink('/', '?join=r&invite=x&keep=1', '') === '/?keep=1', 'other query keys stay');

if (bad) process.exit(1);
console.log('moved link ok');
