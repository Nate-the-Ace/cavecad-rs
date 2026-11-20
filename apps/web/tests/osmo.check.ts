// The Osmo message encoder: DJI's whole-message checksum (seed 0x3692) rebuilds DJI Mimo's own messages to the gimbal
// byte for byte (PacketLogger capture, 2026-11-17), and a speed message is still exactly the one proven to move it.
import { duml, speedMessage, DumlParser, clampAxis, rotateByQuat, type Quat } from '../src/lib/instrument/osmo.ts';
const hex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
let fails = 0;
const ok = (c: boolean, m: string) => {
  if (!c) {
    fails++;
    console.error('FAIL', m);
  }
};
// [seq, set, cmd, payload, attributes, Mimo's bytes]
const mimo: [number, number, number, number[], number, string][] = [
  [0xbd9a, 0x04, 0x4c, [0x02, 0x00], 0x40, '550f04a202049abd40044c02009887'], // mode 2
  [0xc4b1, 0x04, 0x4c, [0x01, 0x00], 0x40, '550f04a20204b1c440044c01008358'], // mode 1
  [0xbd56, 0x04, 0x10, [0x0a], 0x40, '550e0466020456bd4004100aeb00'],
  [0xc475, 0x04, 0x54, [0x01, 0xdc, 0x05, 0, 0, 0, 0, 0x80, 0, 0, 0], 0x40, '55180420020475c440045401dc050000000080000000b71f'],
];
for (const [seq, set, cmd, p, attr, want] of mimo) {
  const got = hex(duml(seq, set, cmd, p, attr));
  ok(got === want, `${set.toString(16)}/${cmd.toString(16)}: ${got} vs Mimo ${want}`);
}
// proven on the gimbal (Swift probe, 2026-11-16): seq 1, yaw 30°/s
ok(hex(speedMessage(1, 30, 0, 0)) === '551504a90204010000040c2c0100000000800223de', `speed: ${hex(speedMessage(1, 30, 0, 0))}`);
// the parser takes Mimo's messages back apart, also when they arrive split across notifications and with junk between
const p = new DumlParser();
const stream = [...duml(7, 0x04, 0x05, Array.from({ length: 30 }, (_, i) => i), 0, 0x02), 0x00, 0x13, ...duml(8, 0x00, 0x00, [0x11], 0x20, 0x02)];
const got = [...p.feed(Uint8Array.from(stream.slice(0, 9))), ...p.feed(Uint8Array.from(stream.slice(9)))];
ok(got.length === 2 && got[0].set === 4 && got[0].cmd === 5 && got[0].payload.length === 30 && got[1].attr === 0x20 && got[1].seq === 8, `parser: ${JSON.stringify(got.map((g) => [g.set, g.cmd, g.attr, g.seq]))}`);
// attitude from the bench test (2026-11-17): at rest the clamp top points straight up; after roll +30 (gimbal said
// joint roll 80.6 -> 108.3) it leans ~27 degrees; after yaw +45 the clamp's side axis has turned ~41 degrees
const near = (a: number, b: number, t: number) => Math.abs(a - b) < t;
const rest = clampAxis([0.7067, -0.7064, 0.0354, -0.016]);
ok(near(rest.inc, 88.5, 1), `rest: ${JSON.stringify(rest)}`);
const rolled = clampAxis([0.8044, -0.5005, -0.1525, 0.2815]);
ok(near(rolled.inc, 63, 1), `rolled: ${JSON.stringify(rolled)}`);
const side = (q: Quat) => { const [n, e] = rotateByQuat(q, [1, 0, 0]); return (Math.atan2(e, n) * 180) / Math.PI; };
ok(near(side([0.6653, -0.6751, -0.2156, 0.235]) - side([0.7067, -0.7064, 0.0354, -0.016]), 41.4, 1.5), 'yaw turn');
if (fails) {
  console.error(`${fails} osmo check(s) failed`);
  process.exit(1);
}
console.log('osmo encoder: ok');
