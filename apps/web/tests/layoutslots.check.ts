import { LAYOUT_SLOTS, cleanName, emptySlots, fitsPanels, panelIds, parseSlots, slotName, withSlot, withoutSlot, type SavedLayout } from '../src/lib/layout-slots.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

const dock = (...ids: string[]) => ({ grid: {}, panels: Object.fromEntries(ids.map((id) => [id, { id }])), activeGroup: 'g1' });
const saved = (name: string, ids: string[] = ['map'], mapLock = false): SavedLayout => ({ name, layout: dock(...ids), mapLock });

ok(LAYOUT_SLOTS === 6, 'there are six slots');
ok(emptySlots().length === 6 && emptySlots().every((s) => s === null), 'they start empty');

const one = withSlot(emptySlots(), 2, saved('Cave'));
ok(one.length === 6 && one[2]?.name === 'Cave' && one.filter((s) => s).length === 1, 'a layout goes into its slot and nowhere else');
ok(withSlot(one, 2, saved('Again'))[2]?.name === 'Again', 'saving into a full slot replaces it');
ok(withSlot(one, 6, saved('x')) === one && withSlot(one, -1, saved('x')) === one && withSlot(one, 1.5, saved('x')) === one, 'a slot that does not exist changes nothing');
ok(one[2]?.name === 'Cave', 'and saving never changes the slots it was given (a copy comes back)');
ok(withoutSlot(one, 2)[2] === null && withoutSlot(one, 9) === one, 'a slot can be emptied');

ok(slotName(one, 2) === 'Cave' && slotName(one, 0) === 'Layout 1' && slotName(one, 5) === 'Layout 6', 'an unnamed slot is "Layout" and its number from one');
ok(cleanName('  Sketch   in the cave ', 0) === 'Sketch in the cave', 'a name is trimmed and its spaces tidied');
ok(cleanName('   ', 3) === 'Layout 4' && cleanName('', 0) === 'Layout 1', 'an empty name is the default');
ok(cleanName('x'.repeat(80), 0).length === 30, 'a long name is cut to 30 characters');

// What comes back from storage is not trusted.
const kept = JSON.parse(JSON.stringify(one));
ok(JSON.stringify(parseSlots(kept)) === JSON.stringify(one), 'what was kept reads back the same');
ok(parseSlots(null).length === 6 && parseSlots('nonsense').length === 6 && parseSlots({}).every((s) => s === null), 'junk is six empty slots');
ok(parseSlots([saved('A'), saved('B')]).length === 6 && parseSlots([saved('A')])[1] === null, 'a short list is padded to six');
ok(parseSlots(Array.from({ length: 9 }, () => saved('x'))).length === 6, 'a long list is cut to six');
const broken = parseSlots([{ name: 'no layout', mapLock: false }, { name: 7, layout: dock('map'), mapLock: false }, { name: 'ok', layout: dock('map'), mapLock: 'yes' }, { name: 'good', layout: dock('map'), mapLock: true }]);
ok(broken[0] === null && broken[1] === null && broken[2] === null && broken[3]?.name === 'good', 'a slot that is not well formed is an empty one; the good ones survive');

// Panels.
ok(panelIds(dock('map', 'survey')).join() === 'map,survey', 'the panels of a layout are listed');
ok(panelIds(null).length === 0 && panelIds({}).length === 0 && panelIds({ panels: [] }).length === 0, 'a layout with none, or not a layout, lists none');
const has = (id: string) => ['map', 'profile', 'survey'].includes(id);
ok(fitsPanels(dock('map', 'survey'), has), 'a layout of panels this workspace has fits');
ok(!fitsPanels(dock('map', 'tools'), has), 'one naming a panel it no longer has does not (a layout from an older version)');
ok(!fitsPanels(dock(), has) && !fitsPanels(null, has), 'an empty layout does not fit');

if (bad) process.exit(1);
console.log('layoutslots ok');
