import { makeCatalog } from '../src/handbook/catalog.ts';
let bad = 0;
const check = (ok: boolean, what: string) => { if (!ok) { bad++; console.log('FAIL', what); } };

const cat = makeCatalog(
  [{ id: 'a', group: 'Tasks', title: 'Bringing data in' }, { id: 'map', group: 'Panels', title: 'Map', panel: 'map' }, { id: 'splays', group: 'Panels', title: 'Splays' }],
  { a: '# Bringing data in\n\nSplay shots come in too.', map: '# Map\n\nThe plan.', splays: '# Splays\n\nShots to walls.' },
);
check(cat.pages.map((p) => p.id).join() === 'a,map,splays', 'keeps index order');
check(cat.forPanel('map')?.id === 'map', 'page for a panel');
check(cat.byId('nope') === undefined, 'unknown id');
check(cat.search('splay').map((p) => p.id).join() === 'splays,a', 'title hits first');
check(cat.search('  ').length === 0, 'empty query');
check(cat.groups().map((g) => g.name).join() === 'Tasks,Panels', 'groups in first-appearance order');

console.log(bad ? 'FAILED' : 'handbook ok');
process.exit(bad ? 1 : 0);
