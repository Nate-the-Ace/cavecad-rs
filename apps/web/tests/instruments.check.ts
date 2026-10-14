import {
  BRIC_CONTROL, BRIC_CONTROL_SERVICE, BRIC_ERRORS, BRIC_LAST_TIME, BRIC_MEASURE_SERVICE, BRIC_META, BRIC_PRIMARY, BricCommand, commandBytes, decodeErrors, decodeMeta, decodePrimary, describeBricErrors, encodeShot, lastTimeBytes,
} from '../src/lib/instrument/bric.ts';
import { SAP5_NOTIFY, SAP5_SERVICE, SAP5_WRITE, decodeDistoXPacket, encodeDistoXPacket } from '../src/lib/instrument/sap5.ts';
import { hasNordicUart, identify, NAMES } from '../src/lib/instrument/identify.ts';
import { BRIC_SETTLE_MS, BricLink, Sap5Link, type Timers } from '../src/lib/instrument/link.ts';
import { CAVE_BLE_SERVICE, hex } from '../src/lib/instrument/caveble.ts';
import { X1_SERVICE } from '../src/lib/instrument/cavway.ts';
import type { LinkEvents, Reading } from '../src/lib/instrument/types.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number | null | undefined, b: number, e = 1e-6) => typeof a === 'number' && Math.abs(a - b) <= e;
const bytes = (...n: number[]) => Uint8Array.from(n);

// ---- BRIC: uuids ----
ok(BRIC_MEASURE_SERVICE === '000058d0-0000-1000-8000-00805f9b34fb' && BRIC_PRIMARY === '000058d1-0000-1000-8000-00805f9b34fb' && BRIC_META.startsWith('000058d2') && BRIC_ERRORS.startsWith('000058d3') && BRIC_LAST_TIME.startsWith('000058d4'), 'the BRIC measurement uuids are the 16-bit 58d0 to 58d4 on the standard base');
ok(BRIC_CONTROL_SERVICE.startsWith('000058e0') && BRIC_CONTROL.startsWith('000058e1'), 'and the control ones 58e0 and 58e1');
ok(Array.from(commandBytes(BricCommand.SHOT)).join() === '115,104,111,116' && Array.from(commandBytes(BricCommand.POWER_OFF)).length === 9 && commandBytes(BricCommand.CLEAR_MEMORY).length === 12, 'commands are ASCII words: "shot" is 73 68 6f 74, "power off" 9 bytes, "clear memory" 12');

// ---- BRIC: indications written out by hand from the layout ----
// primary: 2026-10-14 12:30:05.25, 12.5 m, 90.0 deg, -10.5 deg
const prim = bytes(0xea, 0x07, 0x0a, 0x0e, 0x0c, 0x1e, 0x05, 0x19, 0x00, 0x00, 0x48, 0x41, 0x00, 0x00, 0xb4, 0x42, 0x00, 0x00, 0x28, 0xc1);
const p = decodePrimary(prim);
ok(p !== null && p.time.year === 2026 && p.time.month === 10 && p.time.day === 14 && p.time.hour === 12 && p.time.minute === 30 && p.time.second === 5 && p.time.centisecond === 25, 'the primary carries a time: year, month, day, hour, minute, second, hundredths');
ok(p?.distance === 12.5 && p?.azimuth === 90 && p?.inclination === -10.5, 'and the distance (m), azimuth and inclination as little-endian floats');
ok(decodePrimary(prim.subarray(0, 19)) === null && decodePrimary(bytes()) === null, 'a short primary is not one');
const nan = Uint8Array.from(prim);
nan.set([0x00, 0x00, 0xc0, 0x7f], 8);
ok(decodePrimary(nan) === null, 'a NaN distance is not a shot');
// meta: index 5, dip 60, roll 0, temperature 20, 8 samples, type 0
const meta = bytes(0x05, 0, 0, 0, 0x00, 0x00, 0x70, 0x42, 0, 0, 0, 0, 0x00, 0x00, 0xa0, 0x41, 0x08, 0x00, 0x00);
const m = decodeMeta(meta);
ok(m?.index === 5 && m.dip === 60 && m.roll === 0 && m.temperature === 20 && m.samples === 8 && m.type === 0, 'the meta carries index, dip, roll, temperature, samples and type');
ok(decodeMeta(meta.subarray(0, 18)) === null, 'a short meta is not one');
// errors: first code 7 (target moved), second none
const errs = decodeErrors(bytes(7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));
ok(errs?.[0].code === 7 && errs?.[1].code === 0, 'the errors are two entries of a code and two numbers');
ok(describeBricErrors(errs) === 'the target moved', 'a code is described in words');
ok(describeBricErrors(decodeErrors(bytes(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))) === null, 'no error is no message');
ok(describeBricErrors(decodeErrors(bytes(8, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0))) === 'laser signal too weak; the two accelerometers disagree', 'two errors are both said');
ok(decodeErrors(bytes(7, 0, 0, 0, 0, 0, 0, 0, 0))?.[1].code === 0, 'a short errors indication (one entry) still gives the first');
ok(decodeErrors(bytes(1, 2, 3)) === null, 'too short to hold one is not errors');
ok(describeBricErrors([{ code: 99, a: 0, b: 0 }, { code: 0, a: 0, b: 0 }]) === 'error 99', 'a code nobody has described is still reported');
const t = lastTimeBytes({ year: 2026, month: 10, day: 14, hour: 12, minute: 30, second: 5, centisecond: 25 });
ok(t.length === 20 && hex(t.subarray(0, 8)) === 'ea 07 0a 0e 0c 1e 05 19' && t.subarray(8).every((b) => b === 0), 'the last-time write is the time then zeros to 20 bytes');
const built = encodeShot({ distance: 12.5, azimuth: 90, inclination: -10.5, index: 5, time: { minute: 30, second: 5, centisecond: 25, hour: 12 } });
ok(hex(built.primary) === hex(prim) && hex(built.meta) === hex(meta), 'the test encoder builds exactly the hand-written bytes');

// ---- BRIC: the link ----
function fakeTimers() {
  const waiting: { fn: () => void; id: number }[] = [];
  let n = 0;
  const timers: Timers = {
    schedule: (fn) => (waiting.push({ fn, id: ++n }), n),
    cancel: (h) => { const i = waiting.findIndex((w) => w.id === h); if (i >= 0) waiting.splice(i, 1); },
  };
  return { timers, waiting, fire: () => waiting.splice(0).forEach((w) => w.fn()) };
}
function bricRig() {
  const readings: Reading[] = [];
  const frames: string[] = [];
  const ev: LinkEvents = { status() {}, frame: (h) => frames.push(h), reading: (r) => readings.push(r) };
  const clock = fakeTimers();
  return { readings, frames, clock, link: new BricLink(ev, 'bric', clock.timers) };
}
{
  const r = bricRig();
  const s1 = encodeShot({ distance: 7.25, azimuth: 123.5, inclination: 4.5, index: 1, roll: 12, time: { second: 1 } });
  r.link.onFrame('primary', s1.primary);
  ok(r.readings.length === 0, 'the primary alone is not yet a shot: its meta and errors are still to come');
  r.link.onFrame('meta', s1.meta);
  r.link.onFrame('errors', s1.errors);
  ok(r.readings.length === 1 && r.readings[0].distance === 7.25 && r.readings[0].azimuth === 123.5 && r.readings[0].inclination === 4.5 && r.readings[0].roll === 12, 'the errors complete the shot: one reading, with the roll from the meta');
  ok(r.readings[0].source === 'bric' && r.readings[0].error === null && r.readings[0].mark === 'none', 'its source, no error, no mark');
  r.link.onFrame('primary', s1.primary);
  r.link.onFrame('meta', s1.meta);
  r.link.onFrame('errors', s1.errors);
  ok(r.readings.length === 1, 'the same primary again is a repeat, not a second shot');
  ok(r.clock.waiting.length === 0, 'and nothing is left waiting on a timer');
}
{
  const r = bricRig();
  const bad1 = encodeShot({ distance: 3, azimuth: 10, inclination: 0, index: 1, time: { second: 1 }, errors: [[7, 0, 0]] });
  r.link.onFrame('primary', bad1.primary);
  r.link.onFrame('meta', bad1.meta);
  r.link.onFrame('errors', bad1.errors);
  ok(r.readings[0].error === 'the target moved', 'an error the instrument reports goes with the reading, in words');
}
{
  const r = bricRig();
  const a = encodeShot({ distance: 4, azimuth: 40, inclination: 1, index: 1, time: { second: 1 } });
  r.link.onFrame('primary', a.primary);
  ok(r.clock.waiting.length === 1, 'a primary starts a wait for the rest');
  r.clock.fire();
  ok(r.readings.length === 1 && r.readings[0].roll === null, 'if the rest never comes the shot is written after the wait, with no roll');
  const b = encodeShot({ distance: 5, azimuth: 50, inclination: 2, index: 2, time: { second: 2 } });
  const c = encodeShot({ distance: 6, azimuth: 60, inclination: 3, index: 3, time: { second: 3 } });
  r.link.onFrame('primary', b.primary);
  r.link.onFrame('primary', c.primary);
  ok(r.readings.length === 2 && r.readings[1].distance === 5, 'a new primary shows the one before it was left behind: that one is written');
  r.link.flush();
  ok(r.readings.length === 3 && r.readings[2].distance === 6, 'and flush writes the last');
  r.link.flush();
  ok(r.readings.length === 3, 'flushing with nothing waiting writes nothing');
}
{
  const r = bricRig();
  const z = encodeShot({ distance: 0, azimuth: 0, inclination: 0, index: 1, time: { second: 1 } });
  r.link.onFrame('primary', z.primary);
  r.link.onFrame('meta', z.meta);
  r.link.onFrame('errors', z.errors);
  ok(r.readings.length === 0 && r.frames.some((f) => /no length/.test(f)), 'a shot with no length is logged and left out');
  const one = encodeShot({ distance: 3, azimuth: 0, inclination: 0, index: 2, time: { second: 2 } });
  const four = encodeShot({ distance: 3, azimuth: 1, inclination: 0, index: 5, time: { second: 3 } });
  for (const s of [one, four]) { r.link.onFrame('primary', s.primary); r.link.onFrame('meta', s.meta); r.link.onFrame('errors', s.errors); }
  ok(r.readings.length === 2 && r.frames.some((f) => /missed 2 shot/.test(f)), 'a jump in the running index (2 to 5) says two shots were missed');
  r.link.onFrame('primary', bytes(1, 2, 3));
  ok(r.frames.some((f) => /not a BRIC primary/.test(f)), 'junk is logged and ignored');
  ok(BRIC_SETTLE_MS === 400, 'the wait for the rest of a shot is 400 ms');
}

// ---- SAP5: the DistoX 8-byte packet ----
// 5.000 m (0x1388), azimuth 0x4000 (90 deg), clino 0xF8E4 (-1820: -9.9976 deg), roll 0x40 (90 deg), seq bit 1
const d = decodeDistoXPacket(bytes(0x81, 0x88, 0x13, 0x00, 0x40, 0xe4, 0xf8, 0x40));
ok(d !== null && d.seq === 1 && d.distance === 5 && d.azimuth === 90 && near(d.inclination, -9.9976, 0.001) && d.roll === 90, 'a hand-written SAP5 packet: 5 m, 90 deg, about -10 deg, roll 90, sequence bit 1');
ok(decodeDistoXPacket(bytes(0x01, 0x88, 0x13, 0x00, 0x40, 0xe4, 0xf8, 0x40))?.seq === 0, 'sequence bit 0');
ok(decodeDistoXPacket(bytes(1, 2, 3)) === null && decodeDistoXPacket(new Uint8Array(9)) === null, 'a packet of another length is not one');
ok(decodeDistoXPacket(bytes(0x02, 0x88, 0x13, 0, 0, 0, 0, 0)) === null, 'another packet type (calibration data) is not a shot');
ok(near(decodeDistoXPacket(bytes(0x01, 0x70, 0x11, 0, 0, 0, 0, 0))?.distance, 4.464, 1e-9), 'with the high bit clear 0x1170 is 4.464 m');
ok(near(decodeDistoXPacket(bytes(0x41, 0x70, 0x11, 0, 0, 0, 0, 0))?.distance, 70, 1e-9), 'with bit 16 set (0x40 in the first byte) it is 70.000 m: 65.536 + 4.464');
// The SAP5 firmware fault: bit 16 set once past 32.768 m. 33.000 m = 0x80E8.
const faulty = bytes(0x41, 0xe8, 0x80, 0, 0, 0, 0, 0);
ok(near(decodeDistoXPacket(faulty)?.distance, 33, 1e-9), 'the SAP5 fault (bit 16 set at 33 m) is corrected: 33.000 m');
ok(near(decodeDistoXPacket(faulty, { fixBit16: false })?.distance, 98.536, 1e-9), 'and without the correction it would read 98.5 m');
ok(near(decodeDistoXPacket(bytes(0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00))?.inclination, 90, 1e-9) && near(decodeDistoXPacket(bytes(0x01, 0, 0, 0, 0, 0x00, 0xc0, 0))?.inclination, -90, 1e-9), 'straight up is 90 and straight down -90 exactly');
const back = decodeDistoXPacket(encodeDistoXPacket({ distance: 12.345, azimuth: 271.5, inclination: -33.25, roll: 45, seq: 1 }));
ok(near(back?.distance, 12.345, 1e-9) && near(back?.azimuth, 271.5, 0.01) && near(back?.inclination, -33.25, 0.01) && near(back?.roll, 45, 1.5) && back?.seq === 1, 'encode then decode returns what went in');
ok(SAP5_SERVICE === X1_SERVICE && SAP5_NOTIFY.startsWith('6e400002') && SAP5_WRITE.startsWith('6e400003'), 'the SAP5 shares the Cavway X1 service but notifies on 0002 where the X1 uses 0003');

{
  const readings: Reading[] = [];
  const frames: string[] = [];
  const link = new Sap5Link({ status() {}, frame: (h) => frames.push(h), reading: (r) => readings.push(r) });
  const a = encodeDistoXPacket({ distance: 5, azimuth: 90, inclination: 10, seq: 0 });
  const b = encodeDistoXPacket({ distance: 5, azimuth: 90, inclination: 10, seq: 1 });
  link.onFrame(a);
  link.onFrame(a);
  link.onFrame(b);
  ok(readings.length === 2, 'a repeated packet is one shot; the same numbers with the other sequence bit are a new one');
  ok(readings[0].distance === 5 && readings[0].azimuth === 90 && readings[0].source === 'sap5' && readings[0].mark === 'none', 'the reading is tidied and says where it came from');
  link.onFrame(bytes(1, 2, 3));
  ok(readings.length === 2 && frames.some((f) => /not a DistoX/.test(f)), 'junk is logged and ignored');
  link.reset();
  link.onFrame(a);
  ok(readings.length === 3, 'after a reconnect the same packet counts again');
}

// ---- telling instruments apart ----
const id = (n: string | undefined, s: string[] = []) => identify(n, s);
ok(id('CavwayX1-0123')?.kind === 'x1' && id('CavwayX1-0123')?.how === 'name', 'CavwayX1- is the Cavway X1');
ok(id('SAP6_ab12')?.kind === 'sap6' && id('SAP_ab12')?.kind === 'sap6', 'SAP6_ and SAP_ are the SAP6');
ok(id('SAP5_ab12')?.kind === 'sap5' && id('Shetland_ab12')?.kind === 'sap5', 'SAP5_ and Shetland_ are the SAP5');
ok(id('BRIC4_01')?.kind === 'bric' && id('BRIC5-01')?.kind === 'bric' && id('BRIC5_01')?.kind === 'bric' && id('BRIC-01')?.kind === 'bric', 'every BRIC name is a BRIC');
ok(id('BRIC4_01')?.label === 'BRIC4' && id('BRIC5_01')?.label === 'BRIC5', 'and says which');
ok(id('DistoXBLE-1')?.kind === 'unsupported', 'a DistoX BLE is recognised but not supported yet');
ok(new Set(NAMES.map((n) => n.prefix)).size === NAMES.length, 'no name is listed twice');
ok(NAMES.every((a) => NAMES.every((b) => a === b || !b.prefix.startsWith(a.prefix))), 'and no name prefix hides another');
ok(id('Some Phone') === null && id(undefined) === null && id('') === null, 'a name nobody knows, or none, is not guessed from the name');
ok(id('Unnamed', [CAVE_BLE_SERVICE])?.kind === 'sap6' && id('Unnamed', [CAVE_BLE_SERVICE])?.how === 'service', 'but the CaveBLE service alone means a SAP6');
ok(id(undefined, [BRIC_MEASURE_SERVICE])?.kind === 'bric', 'and the 58d0 service alone means a BRIC');
ok(id(undefined, [X1_SERVICE]) === null && hasNordicUart([X1_SERVICE]) && !hasNordicUart([CAVE_BLE_SERVICE]), 'Nordic UART alone settles nothing (X1, SAP5 and others share it): the packets must tell');
ok(id('SAP5_x', [CAVE_BLE_SERVICE])?.kind === 'sap5', 'the name wins over the services');
ok(id('  CavwayX1-9 ')?.kind === 'x1', 'stray spaces in a name do not matter');

if (bad) process.exit(1);
console.log('instruments ok');
