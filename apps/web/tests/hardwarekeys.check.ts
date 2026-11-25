import { makeHwKeys, parseHwEvent, HW_BOUNCE_MS, HW_CANCEL_MS, type HwActions } from '../src/lib/hardware-keys.ts';
let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

function rig(active = true) {
  const calls: string[] = [];
  let t = 1000;
  const a: HwActions = {
    active: () => active,
    laser: (on) => calls.push(`laser:${on}`),
    fire: () => calls.push('fire'),
    acceptNext: () => calls.push('acceptNext'),
    undo: () => calls.push('undo'),
  };
  const key = makeHwKeys(a, () => t);
  return { calls, key, tick: (ms: number) => { t += ms; }, setActive: (v: boolean) => { active = v; } };
}

// parsing
ok(parseHwEvent({ key: 'ptt', phase: 'down' })?.key === 'ptt', 'a good event parses');
ok(parseHwEvent({ key: 'camera', phase: 'down' }) === undefined, 'camera and focus are not hardware keys here');
ok(parseHwEvent({ key: 'ptt', phase: 'hold' }) === undefined, 'an unknown phase is ignored');
ok(parseHwEvent(null) === undefined && parseHwEvent('ptt') === undefined && parseHwEvent({}) === undefined, 'junk is ignored');

// PTT is a trigger: down aims, a quick release fires
{
  const r = rig();
  r.key('ptt', 'down'); r.tick(400); r.key('ptt', 'up');
  ok(r.calls.join() === 'laser:true,fire', 'PTT down aims, a quick release fires');
}
// held to the cancel time: laser off, no shot
{
  const r = rig();
  r.key('ptt', 'down'); r.tick(HW_CANCEL_MS); r.key('ptt', 'up');
  ok(r.calls.join() === 'laser:true,laser:false', 'PTT held for the cancel time cancels');
  const s = rig();
  s.key('ptt', 'down'); s.tick(HW_CANCEL_MS - 1); s.key('ptt', 'up');
  ok(s.calls.join() === 'laser:true,fire', 'just under the cancel time still fires');
}
// a release with no press seen does nothing
{
  const r = rig();
  r.key('ptt', 'up');
  ok(r.calls.length === 0, 'a PTT release with no press does nothing');
}
// the survey ending while PTT is held: laser off, no shot
{
  const r = rig();
  r.key('ptt', 'down'); r.setActive(false); r.tick(300); r.key('ptt', 'up');
  ok(r.calls.join() === 'laser:true,laser:false', 'PTT released after the survey ended switches the laser off');
}
// Yellow and SOS
{
  const r = rig();
  r.key('yellow', 'down'); r.tick(500); r.key('sos', 'down'); r.key('yellow', 'up'); r.key('sos', 'up');
  ok(r.calls.join() === 'acceptNext,undo', 'Yellow accepts and moves on, SOS undoes, releases do nothing');
}
// bounce
{
  const r = rig();
  r.key('yellow', 'down'); r.tick(HW_BOUNCE_MS - 1); r.key('yellow', 'down'); r.tick(HW_BOUNCE_MS); r.key('yellow', 'down');
  ok(r.calls.join() === 'acceptNext,acceptNext', 'a repeat inside the bounce time is dropped, a later press counts');
  const s = rig();
  s.key('yellow', 'down'); s.key('sos', 'down');
  ok(s.calls.join() === 'acceptNext,undo', 'bounce is per key');
}
// not surveying
{
  const r = rig(false);
  r.key('ptt', 'down'); r.key('sos', 'down'); r.key('yellow', 'down'); r.key('ptt', 'up');
  ok(r.calls.length === 0, 'with no survey live the keys do nothing');
}

console.log(bad ? 'FAILED' : 'hardware key checks ok'); process.exit(bad ? 1 : 0);
