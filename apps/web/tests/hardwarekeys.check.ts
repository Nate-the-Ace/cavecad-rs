import {
  makeHwKeys, parseHwEvent, parseBindings, parseCustom, validAction, DEFAULT_BINDINGS, CUSTOM_LIMIT, HW_BOUNCE_MS, HW_CANCEL_MS, HW_KEYS, HW_ACTIONS,
  type HwActions, type HwBindings,
} from '../src/lib/hardware-keys.ts';
let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

function rig(active = true, bindings: HwBindings = DEFAULT_BINDINGS) {
  const calls: string[] = [];
  let t = 1000;
  let b = bindings;
  const a: HwActions = {
    active: () => active,
    laser: (on) => calls.push(`laser:${on}`),
    fire: () => calls.push('fire'),
    acceptNext: () => calls.push('acceptNext'),
    undo: () => calls.push('undo'),
    next: () => calls.push('next'),
    previous: () => calls.push('previous'),
  };
  const key = makeHwKeys(a, () => b, () => t);
  return { calls, key, tick: (ms: number) => { t += ms; }, setActive: (v: boolean) => { active = v; }, bind: (x: Partial<HwBindings>) => { b = { ...b, ...x }; } };
}

// ---- events ----
ok(parseHwEvent({ key: 'ptt', phase: 'down' })?.key === 'ptt', 'a known key parses');
ok(parseHwEvent({ key: 'prog2', phase: 'up' })?.key === 'prog2', 'every key name parses');
ok(parseHwEvent({ key: 'volume', phase: 'down' }) === undefined, 'an unknown key name is ignored');
ok(parseHwEvent({ key: 'ptt', phase: 'hold' }) === undefined, 'an unknown phase is ignored');
ok(parseHwEvent(null) === undefined && parseHwEvent('ptt') === undefined && parseHwEvent({}) === undefined, 'junk is ignored');
{
  const e = parseHwEvent({ key: 'unknown', code: 131, scan: 149, phase: 'down' });
  ok(e?.key === 'unknown' && (e as { code: number }).code === 131 && (e as { scan: number }).scan === 149, 'a learned key carries its codes');
  ok(parseHwEvent({ key: 'unknown', code: 'x', phase: 'down' }) === undefined, 'a learned key needs a numeric code');
}

// ---- bindings ----
ok(HW_KEYS.every((k) => k.id in DEFAULT_BINDINGS) && Object.values(DEFAULT_BINDINGS).every((v) => HW_ACTIONS.some((a) => a.id === v)), 'the defaults cover every key with a real action');
ok(JSON.stringify(parseBindings(null)) === JSON.stringify(DEFAULT_BINDINGS) && JSON.stringify(parseBindings('not json')) === JSON.stringify(DEFAULT_BINDINGS), 'no or bad saved bindings are the defaults');
{
  const b = parseBindings(JSON.stringify({ ptt: 'shot', prog2: 'undo', sos: 'fly', zzz: 'undo' }));
  ok(b.ptt === 'shot' && b.prog2 === 'undo', 'saved bindings are kept');
  ok(b.sos === DEFAULT_BINDINGS.sos && !('zzz' in b), 'an unknown action or key is dropped');
}

// ---- custom keys ----
ok(validAction('com.sonim.intent.action.PTT_KEY_DOWN') && validAction('com.zello.ptt.down'), 'broadcast names are valid');
ok(!validAction('') && !validAction('a b') && !validAction('x;alert(1)') && !validAction("a'b") && !validAction(5) && !validAction('ab'), 'anything else is not');
{
  const c = parseCustom(JSON.stringify({
    actions: [
      { action: 'com.acme.PTT_DOWN', key: 'ptt', phase: 'down' },
      { action: 'com.acme.PTT_DOWN', key: 'sos', phase: 'down' }, // a repeat
      { action: 'bad action', key: 'ptt', phase: 'down' },
      { action: 'com.acme.X', key: 'nope', phase: 'down' },
      { action: 'com.acme.Y', key: 'prog1', phase: 'sideways' },
    ],
    keycodes: [{ code: 131, key: 'prog1' }, { code: 131, key: 'prog2' }, { code: -3, key: 'prog1' }, { code: 1.5, key: 'prog1' }, { code: 7, key: 'x' }],
  }));
  ok(c.actions.length === 1 && c.actions[0].key === 'ptt', 'only valid, new broadcast actions are kept');
  ok(c.keycodes.length === 1 && c.keycodes[0].key === 'prog1', 'only valid, new key codes are kept');
  const many = parseCustom(JSON.stringify({ actions: Array.from({ length: 60 }, (_, i) => ({ action: `com.acme.A${i}`, key: 'ptt', phase: 'down' })), keycodes: Array.from({ length: 60 }, (_, i) => ({ code: i + 1, key: 'ptt' })) }));
  ok(many.actions.length === CUSTOM_LIMIT && many.keycodes.length === CUSTOM_LIMIT, 'each kind stops at the limit');
  ok(parseCustom('garbage').actions.length === 0 && parseCustom(null).keycodes.length === 0, 'bad saved text is nothing custom');
}

// ---- trigger ----
{
  const r = rig();
  r.key('ptt', 'down'); r.tick(400); r.key('ptt', 'up');
  ok(r.calls.join() === 'laser:true,fire', 'trigger: down aims, a quick release fires');
  const c = rig();
  c.key('ptt', 'down'); c.tick(HW_CANCEL_MS); c.key('ptt', 'up');
  ok(c.calls.join() === 'laser:true,laser:false', 'trigger: held for the cancel time cancels');
  const s = rig();
  s.key('ptt', 'down'); s.tick(HW_CANCEL_MS - 1); s.key('ptt', 'up');
  ok(s.calls.join() === 'laser:true,fire', 'trigger: just under the cancel time still fires');
  const u = rig();
  u.key('ptt', 'up');
  ok(u.calls.length === 0, 'a release with no press does nothing');
  const e = rig();
  e.key('ptt', 'down'); e.setActive(false); e.tick(300); e.key('ptt', 'up');
  ok(e.calls.join() === 'laser:true,laser:false', 'the survey ending while held puts the laser out, no shot');
}
// any key can be the trigger, and two keys keep their own state
{
  const r = rig(true, { ...DEFAULT_BINDINGS, prog2: 'trigger' });
  r.key('prog2', 'down'); r.tick(300); r.key('prog2', 'up');
  ok(r.calls.join() === 'laser:true,fire', 'a rebound key works as the trigger');
}

// ---- the other actions ----
{
  const r = rig(true, { ...DEFAULT_BINDINGS, camera: 'shot', focus: 'aim', prog3: 'laser', prog2: 'next', ptt: 'previous' });
  r.key('focus', 'down'); r.tick(500); r.key('camera', 'down'); r.tick(500); r.key('camera', 'up'); r.key('focus', 'up');
  ok(r.calls.join() === 'laser:true,fire,laser:false', 'aim holds the laser on while the key is held; shot fires at once');
  const t = rig(true, { ...DEFAULT_BINDINGS, prog3: 'laser' });
  t.key('prog3', 'down'); t.tick(500); t.key('prog3', 'up'); t.tick(500); t.key('prog3', 'down');
  ok(t.calls.join() === 'laser:true,laser:false', 'laser: a press switches it on, the next press off, and a release does nothing');
}
{
  const r = rig(true, { ...DEFAULT_BINDINGS, prog2: 'next', prog3: 'previous' });
  r.key('prog1', 'down'); r.tick(500); r.key('sos', 'down'); r.tick(500); r.key('prog2', 'down'); r.tick(500); r.key('prog3', 'down');
  ok(r.calls.join() === 'acceptNext,undo,next,previous', 'accept-next, undo, next and previous');
  const n = rig();
  n.key('prog2', 'down'); n.key('prog3', 'down');
  ok(n.calls.length === 0, 'a key set to nothing does nothing');
}
// rebinding takes effect at once
{
  const r = rig();
  r.key('prog1', 'down'); r.tick(500); r.bind({ prog1: 'undo' }); r.key('prog1', 'down');
  ok(r.calls.join() === 'acceptNext,undo', 'changing a binding changes the next press');
}
// bounce, per key
{
  const r = rig();
  r.key('prog1', 'down'); r.tick(HW_BOUNCE_MS - 1); r.key('prog1', 'down'); r.tick(HW_BOUNCE_MS); r.key('prog1', 'down');
  ok(r.calls.join() === 'acceptNext,acceptNext', 'a repeat inside the bounce time is dropped, a later press counts');
  const s = rig();
  s.key('prog1', 'down'); s.key('sos', 'down');
  ok(s.calls.join() === 'acceptNext,undo', 'bounce is per key');
}
// not surveying
{
  const r = rig(false);
  r.key('ptt', 'down'); r.key('sos', 'down'); r.key('prog1', 'down'); r.key('camera', 'down');
  ok(r.calls.length === 0, 'with no survey live the keys do nothing');
}

console.log(bad ? 'FAILED' : 'hardware key checks ok'); process.exit(bad ? 1 : 0);
