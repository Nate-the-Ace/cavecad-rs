import {
  X1Command, X1Memory, X1_NOTIFY, X1_SERVICE, X1_USB, X1_WRITE, ackFrame, command, decodePacket, describeErrors, encodePacket, frame, parseErrors, parseReply, queryFrame,
} from '../src/lib/instrument/cavway.ts';
import { hex } from '../src/lib/instrument/caveble.ts';
import { X1SimulatedInstrument } from '../src/lib/instrument/x1sim.ts';
import type { LinkEvents, Reading } from '../src/lib/instrument/types.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number | null | undefined, b: number, e = 0.01) => a !== null && a !== undefined && Math.abs(a - b) <= e;

ok(X1_SERVICE === '6e400001-b5a3-f393-e0a9-e50e24dcca9e' && X1_WRITE.startsWith('6e400002') && X1_NOTIFY.startsWith('6e400003'), 'the Nordic UART uuids match the vendor PDF');
ok(X1_USB.vendorId === 0x1a86 && X1_USB.productId === 0x55d3, 'the USB ids match the vendor PDF');

// A packet written out by hand from the PDF's layout (not built with encodePacket), 64 bytes:
// type 01; flag 0x05 (bit 7 clear = leg, bits 1-3 = 2 = backsight, bit 0 set = not calibration);
// distance 12.345 m = 12345 mm = 0x003039 stored as b2=00 (high), b3=39 (low), b4=30 (middle);
// azimuth 0x4000 (about 90 degrees); inclination 0xF8E4 as signed = -1820 (about -10 degrees); roll 0;
// time 1000000 = 40 42 0f 00; no error (0xff); quality 0x0123; sensor 2 absG 0x0102, absM 0x0304, dip 0x0200.
const hand = new Uint8Array(64);
hand.set([0x01, 0x05, 0x00, 0x39, 0x30, 0x00, 0x40, 0xe4, 0xf8, 0x00, 0x00]);
hand.set([0x11, 0x00, 0x22, 0x00, 0x00, 0x08], 11); // sensor 1: absG 0x0011, absM 0x0022, dip 0x0800
hand.set([0x40, 0x42, 0x0f, 0x00], 17);
hand.fill(0xff, 45, 54);
hand.set([0x23, 0x01], 56);
hand.set([0x02, 0x01, 0x04, 0x03, 0x00, 0x02], 58);
const p = decodePacket(hand);
ok(p !== null && p.type === 1 && p.leg && !p.calibration && p.mark === 'backsight', 'flag 0x05: a leg shot marked backsight');
ok(near(p?.distance, 12.345, 1e-9), 'distance is millimetres in the order high, low, middle');
ok(near(p?.azimuth, 90, 0.01) && near(p?.inclination, -10, 0.01) && p?.roll === 0, 'azimuth and signed inclination in degrees');
ok(p?.time === 1000000 && p?.quality === 0x0123 && p?.errors.length === 0, 'time, laser quality, and no error');
ok(p?.sensors.length === 2 && p.sensors[0].absG === 0x11 && p.sensors[0].absM === 0x22 && p.sensors[1].absG === 0x0102 && p.sensors[1].absM === 0x0304, 'both sensors\' magnitudes');
ok(near(p?.sensors[0].dip, 0x0800 * 360 / 65535, 1e-6), 'a dip is a signed angle');

// The short form (33 bytes): everything through sensor 1, nothing after.
const short = decodePacket(hand.slice(0, 33));
ok(short !== null && near(short.distance, 12.345, 1e-9) && short.mark === 'backsight' && short.sensors.length === 1, 'a 33-byte packet decodes');
ok(short?.time === 1000000 && short?.quality === null && short?.errors.length === 0, 'and what is missing from it is null');
ok(decodePacket(hand.slice(0, 32)) === null, '32 bytes is too short');
ok(decodePacket(hand.subarray(0, 40))?.time === 1000000, 'anything from 33 bytes up is accepted');

// Distance past 65.5 m needs the high byte (b2). 70.000 m = 70000 mm = 0x011170.
const far = Uint8Array.from(hand);
far.set([0x01, 0x70, 0x11], 2);
ok(near(decodePacket(far)?.distance, 70, 1e-9), 'a 70 m shot reads 70 m: the high byte counts for 16 bits, not 8');

// Angles: 0xffff is 360; the sign of inclination.
const wide = Uint8Array.from(hand);
wide.set([0xff, 0xff, 0x00, 0x80], 5);
ok(near(decodePacket(wide)?.azimuth, 360, 1e-9) && (decodePacket(wide)?.inclination ?? 0) < -179, '0xffff is 360 degrees; 0x8000 is the most negative inclination');

// Straight up and straight down: 0x4000 and 0xC000 read as exactly 90 and -90, not 90.0014 (which would be refused as past vertical).
const incl = (raw: number) => { const q = Uint8Array.from(hand); q[7] = raw & 0xff; q[8] = (raw >> 8) & 0xff; return decodePacket(q)?.inclination; };
ok(incl(0x4000) === 90 && incl(0xc000) === -90, 'a vertical shot reads 90 and -90 exactly');
ok(near(incl(0x3fff), 89.9959, 0.001) && near(incl(0x4001), 90, 0.0001) === true, 'one step either side of vertical is within the slack');
ok((incl(0x4010) ?? 0) > 90.05 && (incl(0xbff0) ?? 0) < -90.05, 'but a reading well past vertical is not snapped: it stays out of range for the checks to refuse');

// Flags and what they say.
const withFlag = (flag: number, type = 1) => decodePacket(Uint8Array.from(hand, (v, i) => (i === 0 ? type : i === 1 ? flag : v)));
ok(withFlag(0xff)?.leg === false && withFlag(0xff)?.mark === 'none', 'an ordinary shot (0xff): not a leg, no mark');
ok(withFlag(0x7f)?.leg === true && withFlag(0x7f)?.mark === 'none', 'bit 7 clear marks a leg');
ok(withFlag(0xff & ~0x0e)?.mark === 'feature' && withFlag(0x03)?.mark === 'ridge' && withFlag(0x05)?.mark === 'backsight' && withFlag(0x07)?.mark === 'generic', 'marks: feature, ridge, backsight, generic');
ok(withFlag(0x09)?.mark === 'none' && withFlag(0x0f)?.mark === 'none', 'the other values are no mark');
ok(withFlag(0xff, 2)?.calibration === true && withFlag(0xff, 1)?.calibration === false, 'type 2 is a calibration packet');
ok(withFlag(0xff, 3) === null && withFlag(0xff, 0) === null, 'an unknown packet type is not a shot');
ok(decodePacket(Uint8Array.from(hand, (v, i) => (i === 0 ? 0xff : v))) === null, 'an empty memory slot (first byte 0xff) is not a shot');

// Build, then decode: many values.
let all = true;
for (let i = 0; i < 200; i++) {
  const d = 0.3 + i * 0.37;
  const az = (i * 7.31) % 360;
  const inc = -89 + (i * 0.91) % 178;
  const q = decodePacket(encodePacket({ distance: d, azimuth: az, inclination: inc, leg: i % 3 === 0, mark: i % 5 === 0 ? 'ridge' : 'none', time: 1_700_000_000 + i }));
  if (!q || !near(q.distance, d, 0.0006) || !near(q.azimuth, az, 0.006) || !near(q.inclination, inc, 0.006) || q.leg !== (i % 3 === 0) || q.mark !== (i % 5 === 0 ? 'ridge' : 'none') || q.time !== 1_700_000_000 + i) all = false;
}
ok(all, 'two hundred encoded shots decode to what went in');
ok(decodePacket(encodePacket({ distance: 1, azimuth: 1, inclination: 1, type: 2 }))?.calibration === true, 'a calibration packet round trips');
ok(encodePacket({ distance: 1, azimuth: 1, inclination: 1, short: true }).length === 33, 'the short form is 33 bytes');

// Accuracy error block: absG and angle error (bits 7 and 4 clear), values 1.05 / 0.95 and 1820 raw.
const errs = parseErrors(Uint8Array.from([0x6f, 0x04, 0x29, 0x1c, 0x25, 0x1c, 0x07, 0x00, 0x00]));
ok(errs.length === 2 && errs[0].kind === 'absG' && near(errs[0].a, 1.05, 1e-9) && near(errs[0].b, 0.95, 1e-9), 'an absG error carries both sensors\' figures');
ok(errs[1].kind === 'angle' && near(errs[1].a, 1820 * 360 / 65535, 1e-6) && errs[1].b === null, 'an angle error is one unsigned angle');
ok(parseErrors(Uint8Array.from([0xff, 1, 2, 3, 4, 5, 6, 7, 8])).length === 0, '0xff first byte: no error');
ok(parseErrors(Uint8Array.from([])).length === 0, 'an empty block is no error');
ok(describeErrors(errs) === 'accuracy error: absG, angle' && describeErrors([]) === null, 'errors are described in one line');
const withErrors = decodePacket(encodePacket({ distance: 5, azimuth: 5, inclination: 5, errors: [0x6f, 0x04, 0x29, 0x1c, 0x25, 0x1c, 0x07, 0x00, 0x00] }));
ok(withErrors?.errors.length === 2, 'a packet carrying an error block decodes it');

// What is written to the X1.
ok(hex(command(X1Command.TRIGGER)) === '64 61 74 61 3a 01 38 0d 0a', 'laser trigger: data: 01 38 CRLF');
ok(hex(command(X1Command.LASER_ON)) === '64 61 74 61 3a 01 36 0d 0a' && X1Command.LASER_OFF === 0x37 && X1Command.POWER_OFF === 0x34, 'the other commands');
ok(hex(ackFrame(0x05)) === '64 61 74 61 3a 01 55 0d 0a' && hex(ackFrame(0x80)) === '64 61 74 61 3a 01 d5 0d 0a' && hex(ackFrame(0xff)) === '64 61 74 61 3a 01 ff 0d 0a', 'the ack is the flag byte OR 0x55, framed');
ok(hex(queryFrame(X1Memory.FIRMWARE, 4)) === '64 61 74 61 3a 04 3d 00 e0 04 0d 0a', 'a memory query: 3d, address low and high, count');
ok(hex(frame([1, 2, 3])) === '64 61 74 61 3a 03 01 02 03 0d 0a', 'framing carries the payload length');
let threw = 0;
for (const f of [() => queryFrame(0x10000, 4), () => queryFrame(0, 0), () => queryFrame(0, 125), () => queryFrame(-1, 4)]) try { f(); } catch { threw++; }
ok(threw === 4, 'a query outside the address space or past 124 bytes is refused');
const reply = parseReply([0x3d, 0x00, 0xe0, 0x04, 1, 1, 4, 0]);
ok(reply?.command === 0x3d && reply.address === 0xe000 && reply.data.join() === '1,1,4,0', 'a memory reply carries its address and bytes');
ok(parseReply([0x3d, 0, 0, 4, 1, 2]) === null && parseReply([0x01, 0, 0, 0]) === null && parseReply([]) === null, 'a short reply, or one that is not a reply, is not parsed');

// The link, through the simulator: acks, repeats, calibration, garbage, short packets.
function collect(opts?: ConstructorParameters<typeof X1SimulatedInstrument>[1]) {
  const readings: Reading[] = [];
  const frames: string[] = [];
  const events: LinkEvents = { status() {}, frame: (h) => frames.push(h), reading: (r) => readings.push(r) };
  return { readings, frames, events, sim: new X1SimulatedInstrument(events, opts) };
}
const one = collect({ steps: [{ kind: 'splay', distance: 1.5, azimuth: 40, inclination: -3 }, { kind: 'leg', distance: 5, azimuth: 120, inclination: 4 }], policy: 'third' });
await one.sim.connect();
for (let i = 0; i < 4; i++) one.sim.takeShot();
ok(one.readings.length === 4, 'one press, one shot: a splay and the three shots of a leg are four readings');
ok(one.readings[0].leg === false && one.readings[1].leg === false && one.readings[3].leg === true, 'the leg flag is carried through (only the third, under this policy)');
ok(one.readings[0].distance === 1.5 && near(one.readings[0].azimuth, 40, 0.006) && near(one.readings[0].inclination, -3, 0.006), 'values are tidied: 1.5 m exactly, angles to the packet resolution (0.0055 degrees)');
ok(String(one.readings[0].azimuth).length <= 8, 'with no float noise in the digits');
ok(one.readings[0].mark === 'none' && one.readings[1].error === null && one.readings[0].source === 'sim', 'mark, error and source are filled in');
ok(one.sim.received.length === 4 && one.sim.received.every((m) => hex(m).startsWith('64 61 74 61 3a 01 ') && hex(m).endsWith(' 0d 0a')), 'every shot is acknowledged with a framed byte');
ok(hex(one.sim.received[0]) === '64 61 74 61 3a 01 ff 0d 0a', 'an ordinary shot is acknowledged with 0xff');

const rep = collect({ repeatFrames: true, steps: [{ kind: 'splay', distance: 2, azimuth: 10, inclination: 0 }, { kind: 'splay', distance: 3, azimuth: 99, inclination: 0 }] });
await rep.sim.connect();
rep.sim.takeShot();
rep.sim.takeShot();
ok(rep.readings.length === 2 && rep.sim.received.length === 4, 'a packet sent twice is one reading, and both copies are acknowledged');

const sh = collect({ short: true, steps: [{ kind: 'splay', distance: 2.25, azimuth: 77, inclination: 6 }] });
await sh.sim.connect();
sh.sim.takeShot();
ok(sh.readings.length === 1 && sh.readings[0].distance === 2.25 && sh.readings[0].quality === null, 'a 33-byte packet gives the same reading');

// Calibration and junk through the link directly.
const cal = collect();
const { X1Link } = await import('../src/lib/instrument/link.ts');
const acks: Uint8Array[] = [];
const link = new X1Link(cal.events, (b) => acks.push(b), 'test');
link.onFrame(encodePacket({ distance: 0, azimuth: 295, inclination: 8, type: 2 }));
ok(cal.readings.length === 0 && acks.length === 1, 'a calibration shot is acknowledged but not surveyed');
link.onFrame(Uint8Array.from([1, 2, 3]));
link.onFrame(Uint8Array.from([0x3d, 0, 0xe0, 4, 1, 1, 4, 0]));
ok(cal.readings.length === 0 && acks.length === 1 && cal.frames.some((f) => /too short/.test(f)), 'a reply or junk is logged and ignored, never acknowledged');
link.onFrame(encodePacket({ distance: 3, azimuth: 3, inclination: 3, time: 5 }));
link.reset();
link.onFrame(encodePacket({ distance: 3, azimuth: 3, inclination: 3, time: 5 }));
ok(cal.readings.length === 2, 'after a reconnect the same packet counts again');

if (bad) process.exit(1);
console.log('cavway ok');
