import { favoriteItems, isCollapsed, parseCollapsed, parseFavorites, serializeFavorites, toggleCollapsed, toggleKey } from '../src/lib/symbol-favorites.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const eq = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

ok(eq(toggleKey([], 'a'), ['a']), 'toggling adds');
ok(eq(toggleKey(['a', 'b'], 'a'), ['b']), 'toggling again removes');
ok(eq(toggleKey(toggleKey(toggleKey([], 'c'), 'a'), 'b'), ['c', 'a', 'b']), 'order is the order of favouriting');
const before = ['a'];
toggleKey(before, 'b');
ok(eq(before, ['a']), 'the input list is not changed');
ok(eq(toggleKey(['gone', 'a'], 'a'), ['gone']), 'unknown keys survive toggling others');

const all = [{ key: 'a' }, { key: 'b' }, { key: 'c' }];
ok(eq(favoriteItems(all, ['c', 'zz', 'a']), [{ key: 'c' }, { key: 'a' }]), 'display follows favorite order and ignores unknown keys');

ok(eq(parseFavorites(serializeFavorites(['x', 'y'])), ['x', 'y']), 'round trip');
ok(eq(parseFavorites('not json{'), []), 'corrupt text gives empty');
ok(eq(parseFavorites('{"a":1}'), []), 'wrong shape gives empty');
ok(eq(parseFavorites(null), []) && eq(parseFavorites(''), []), 'missing gives empty');
ok(eq(parseFavorites('["a",3,"a","",null,"b"]'), ['a', 'b']), 'non-strings, blanks and duplicates are dropped');

const names = ['Favorites', 'Biology', 'Water'];
ok(parseCollapsed(null) === null && parseCollapsed('junk{') === null && parseCollapsed('{}') === null, 'untouched or corrupt collapsed state is null');
ok(eq(parseCollapsed('["Water",3]'), ['Water']), 'collapsed names parse, non-strings dropped');
ok(!isCollapsed(null, 'Biology', false, false), 'desktop default: everything expanded');
ok(isCollapsed(null, 'Biology', true, false) && !isCollapsed(null, 'Favorites', true, false), 'phone default: categories collapsed, Favorites open');
ok(isCollapsed(['Water'], 'Water', true, false) && !isCollapsed(['Water'], 'Biology', true, false), 'stored state is honoured on the phone');
ok(!isCollapsed(['Water'], 'Water', true, true) && !isCollapsed(null, 'Biology', true, true), 'searching expands everything');
ok(eq(toggleCollapsed(null, 'Biology', names, true), ['Water']), 'first phone toggle starts from the defaults (expanding Biology leaves Water collapsed)');
ok(eq(toggleCollapsed(null, 'Water', names, false), ['Water']), 'first desktop toggle collapses just that section');
ok(eq(toggleCollapsed(['Water'], 'Water', names, false), []), 'toggling again expands');

if (bad) { console.log(bad, 'failed'); process.exit(1); }
console.log('symbol favorites ok');
