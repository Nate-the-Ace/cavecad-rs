import { Registry, type Plugin } from '../src/lib/plugin.ts';
import { SETTINGS_SECTIONS, settingsSectionsFor, type SettingsSection } from '../src/lib/settings-sections.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

interface Stub extends Plugin { hello: string }
const r = new Registry<Stub>('stubs', 1);
ok(r.register({ id: 'a', apiVersion: 1, hello: 'A' }) === true, 'a current plugin registers');
ok(r.register({ id: 'b', apiVersion: 1, hello: 'B' }) === true, 'a second one registers');
ok(r.list().map((p) => p.id).join() === 'a,b', 'list keeps registration order');
ok(r.get('b')?.hello === 'B', 'get finds by id');
ok(r.get('zzz') === undefined, 'get of an unknown id is undefined');
ok(r.register({ id: 'old', apiVersion: 2, hello: 'X' }) === false, 'a plugin for another api version is refused');
ok(r.get('old') === undefined, 'a refused plugin is not kept');
ok(/old/.test(r.refusals()[0] ?? '') && /stubs/.test(r.refusals()[0] ?? ''), 'the refusal names the plugin and the registry');
let threw = false;
try { r.register({ id: 'a', apiVersion: 1, hello: 'again' }); } catch { threw = true; }
ok(threw, 'a duplicate id throws');

// settings sections say which layout shows them: the phone's Settings tab, the dock's user window, or both
const comp = (() => null) as unknown as SettingsSection['component'];
const sec = (id: string, order: number, layouts: SettingsSection['layouts']) => SETTINGS_SECTIONS.register({ id, apiVersion: 1, label: id, order, component: comp, layouts });
sec('p2', 20, ['phone']);
sec('d1', 15, ['dock']);
sec('p1', 10, ['phone']);
sec('both', 30, ['phone', 'dock']);
ok(settingsSectionsFor('phone').map((s) => s.id).join() === 'p1,p2,both', 'phone gets its sections in order, not the dock ones');
ok(settingsSectionsFor('dock').map((s) => s.id).join() === 'd1,both', 'dock gets its sections in order, not the phone ones');
console.log(bad ? 'FAILED' : 'registry ok');
process.exit(bad ? 1 : 0);
