// A DJI Osmo Mobile gimbal over Web Bluetooth (in the apps, the native bridge stands in for navigator.bluetooth).
// DJI publishes nothing; the gimbal speaks DJI's DUML messages on service fff0 (write fff5, it talks back on fff4).
// What the Osmo Mobile 7 actually does is measured in tools/cave-osmo/FINDINGS.md: only SPEED commands move it (the
// angle commands 04/14 and 04/0a are ignored), timed turns land within a few per cent, once its handshake is answered it
// reports its attitude (a quaternion) and joint angles once a second, and its clamp holds a strong magnet that upsets an
// instrument's compass. Kept behind this small class so another pan-tilt head can replace it.

const SERVICE = '0000fff0-0000-1000-8000-00805f9b34fb';
const WRITE = '0000fff5-0000-1000-8000-00805f9b34fb';
const NOTIFY = '0000fff4-0000-1000-8000-00805f9b34fb';

// ---- DUML: reflected CRC-8 (poly 0x31, init 0x77) on the 3-byte header; reflected CRC-16 (poly 0x1021, init 0x3692)
// over the whole message before it. Verified against a capture of DJI Mimo (tools/cave-osmo FINDINGS, 2026-11-17).
const CRC8 = Array.from({ length: 256 }, (_, i) => {
  let c = i;
  for (let k = 0; k < 8; k++) c = c & 1 ? (c >> 1) ^ 0x8c : c >> 1;
  return c;
});
const CRC16 = Array.from({ length: 256 }, (_, i) => {
  let c = i;
  for (let k = 0; k < 8; k++) c = c & 1 ? (c >> 1) ^ 0x8408 : c >> 1;
  return c;
});
export const crc8 = (d: number[]) => d.reduce((c, b) => CRC8[(b ^ c) & 0xff], 0x77);
export const crc16 = (d: number[]) => d.reduce((c, b) => (c >> 8) ^ CRC16[(b ^ c) & 0xff], 0x3692);

/** One message from the app (0x02) to the gimbal (0x04) or another part of it (`to`; 0x27 is the handle board). */
export function duml(seq: number, set: number, cmd: number, payload: number[], attr = 0, to = 0x04): Uint8Array {
  const len = (13 + payload.length) | (1 << 10);
  const head = [0x55, len & 0xff, len >> 8];
  const body = [0x02, to, seq & 0xff, (seq >> 8) & 0xff, attr, set, cmd, ...payload];
  const frame = [...head, crc8(head), ...body];
  const c = crc16(frame);
  return Uint8Array.from([...frame, c & 0xff, c >> 8]);
}

const le16 = (v: number) => {
  const x = Math.max(-32768, Math.min(32767, Math.round(v))) & 0xffff;
  return [x & 0xff, x >> 8];
};

/** Turn at a rate (degrees a second); the gimbal keeps turning ~0.2 s without a fresh one. */
export const speedMessage = (seq: number, yaw: number, pitch: number, roll: number) =>
  duml(seq, 0x04, 0x0c, [...le16(yaw * 10), ...le16(roll * 10), ...le16(pitch * 10), 0x80, 2]);

/** w x y z */
export type Quat = [number, number, number, number];

/** Rotates body vector v by q (body to world). */
export function rotateByQuat(q: Quat, v: [number, number, number]): [number, number, number] {
  const [w, x, y, z] = q;
  const [a, b, c] = v;
  // t = 2 (q.xyz × v); v' = v + w t + q.xyz × t
  const tx = 2 * (y * c - z * b), ty = 2 * (z * a - x * c), tz = 2 * (x * b - y * a);
  return [a + w * tx + (y * tz - z * ty), b + w * ty + (z * tx - x * tz), c + w * tz + (x * ty - y * tx)];
}

/** Where the clamp's long axis points (the X1's laser rides it out of the clamp top): `end` +1 for body +Y, −1 for −Y.
 * Azimuth is clockwise from the gimbal's own north (where it woke up), inclination up from level. */
export function clampAxis(q: Quat, end: 1 | -1 = 1): { az: number; inc: number } {
  const [n, e, d] = rotateByQuat(q, [0, end, 0]);
  const deg = 180 / Math.PI;
  return { az: Math.atan2(e, n) * deg, inc: Math.asin(Math.max(-1, Math.min(1, -d))) * deg };
}

type Char = {
  writeValueWithoutResponse?: (d: BufferSource) => Promise<void>;
  writeValue: (d: BufferSource) => Promise<void>;
  startNotifications?: () => Promise<unknown>;
  addEventListener?: (e: string, f: (ev: Event) => void) => void;
  value?: DataView;
};

/** One message as it came from the gimbal. */
export interface Heard {
  sender: number;
  seq: number;
  attr: number;
  set: number;
  cmd: number;
  payload: Uint8Array;
}

/** Splits the gimbal's byte stream into messages (whole-message checksum); junk and bad messages are skipped. */
export class DumlParser {
  private buf: number[] = [];
  feed(bytes: Uint8Array): Heard[] {
    this.buf.push(...bytes);
    const out: Heard[] = [];
    for (;;) {
      const i = this.buf.indexOf(0x55);
      if (i < 0) {
        this.buf = [];
        break;
      }
      if (i) this.buf.splice(0, i);
      if (this.buf.length < 4) break;
      const len = (this.buf[1] | (this.buf[2] << 8)) & 0x3ff;
      if (len < 13 || crc8(this.buf.slice(0, 3)) !== this.buf[3]) {
        this.buf.shift();
        continue;
      }
      if (this.buf.length < len) break;
      const raw = this.buf.splice(0, len);
      if (crc16(raw.slice(0, len - 2)) !== (raw[len - 2] | (raw[len - 1] << 8))) continue;
      out.push({ sender: raw[4], seq: raw[6] | (raw[7] << 8), attr: raw[8], set: raw[9], cmd: raw[10], payload: Uint8Array.from(raw.slice(11, len - 2)) });
    }
    return out;
  }
}
type Dev = { name?: string; gatt?: { connect: () => Promise<{ getPrimaryService: (u: string) => Promise<{ getCharacteristic: (u: string) => Promise<Char> }> }>; disconnect: () => void; connected?: boolean }; addEventListener: (e: string, f: () => void) => void };

export class OsmoGimbal {
  name = '';
  connected = false;
  private dev: Dev | null = null;
  private writer: Char | null = null;
  private seq = 1 + Math.floor(Math.random() * 60000);
  private parser = new DumlParser();
  onChange: () => void = () => {};
  /** The gimbal's last position report (04/05, about once a second once the handshake is answered): its int16 fields
   * in tenths of a degree by byte offset. Which field is which joint is still being worked out (FINDINGS). */
  position: { at: number; o0: number; o2: number; o4: number; o6: number; o8: number; o20: number; o22: number } | null = null;
  /** The clamp's attitude from the same report (bench test 2026-11-17, FINDINGS): a unit quaternion w x y z, world
   * north-east-down with north = wherever the gimbal woke up (no compass), and the joint angles in degrees (pan o4,
   * tilt o20, roll o22; they move together when the clamp is on its side, so aim by the quaternion, guard by these). */
  attitude: { at: number; q: Quat; pan: number; tilt: number; roll: number } | null = null;
  /** The thumb stick on the gimbal's handle (04/57 from the handle board, about 18 a second): −660…660 each way. */
  stick = { x: 0, y: 0 };
  onPosition: () => void = () => {};

  /** What the gimbal says. Mimo's handshake: it sends 00/00 flagged 0x20 and wants it acknowledged (00/00, flags 0xc0,
   * payload 00 + its byte, its sequence number); position reports follow. */
  private heard(m: Heard) {
    if (m.set === 0x00 && m.cmd === 0x00 && m.attr & 0x20) {
      void this.send(duml(m.seq, 0x00, 0x00, [0x00, m.payload[0] ?? 0], 0xc0, m.sender));
    } else if (m.set === 0x04 && m.cmd === 0x05 && m.payload.length >= 24) {
      const v = (o: number) => (((m.payload[o] | (m.payload[o + 1] << 8)) << 16) >> 16) / 10;
      this.position = { at: performance.now(), o0: v(0), o2: v(2), o4: v(4), o6: m.payload[6], o8: v(8), o20: v(20), o22: v(22) };
      if (m.payload.length >= 40) {
        const dv = new DataView(m.payload.buffer, m.payload.byteOffset, m.payload.byteLength);
        const q = [24, 28, 32, 36].map((o) => dv.getFloat32(o, true)) as Quat;
        this.attitude = { at: this.position.at, q, pan: v(4), tilt: v(20), roll: v(22) };
      }
      (globalThis as { __cavecadDlog?: (s: string) => void }).__cavecadDlog?.(`gimbal pos ${JSON.stringify(this.position)}`);
      this.onPosition();
    } else if (m.set === 0x04 && m.cmd === 0x57 && m.payload.length >= 4) {
      const i16 = (o: number) => (((m.payload[o] | (m.payload[o + 1] << 8)) << 16) >> 16);
      this.stick = { x: i16(0), y: i16(2) };
    }
  }

  /** The first attitude report that arrives after `after` (performance.now() time), or null after `waitMs`. */
  async freshAttitude(after: number, waitMs = 3000) {
    const end = performance.now() + waitMs;
    while (performance.now() < end) {
      const a = this.attitude;
      if (a && a.at > after) return a;
      await new Promise((r) => setTimeout(r, 40));
    }
    return null;
  }

  /** DJI Mimo's 04/54 [01 dc 05 00 00 00 00 80 00 00 00], once a second, three times. The OM7 acknowledges it and does
   * NOT move (bench 2026-11-17): kept for the record, not used. */
  async recentre() {
    for (let k = 0; k < 3; k++) {
      await this.send(duml(this.next(), 0x04, 0x54, [0x01, 0xdc, 0x05, 0, 0, 0, 0, 0x80, 0, 0, 0], 0x40));
      await new Promise((r) => setTimeout(r, 1000));
    }
  }

  async connect(): Promise<void> {
    const bt = (navigator as unknown as { bluetooth?: { requestDevice: (o: unknown) => Promise<Dev> } }).bluetooth;
    if (!bt) throw new Error('No Bluetooth here');
    this.dev = await bt.requestDevice({ filters: [{ namePrefix: 'OM' }, { namePrefix: 'Osmo' }, { services: [SERVICE] }], optionalServices: [SERVICE] });
    this.dev.addEventListener('gattserverdisconnected', () => {
      this.connected = false;
      this.onChange();
    });
    const server = await this.dev.gatt!.connect();
    const svc = await server.getPrimaryService(SERVICE);
    this.writer = await svc.getCharacteristic(WRITE);
    try {
      const n = await svc.getCharacteristic(NOTIFY);
      n.addEventListener?.('characteristicvaluechanged', (ev: Event) => {
        const v = (ev.target as unknown as Char).value;
        if (v) for (const m of this.parser.feed(new Uint8Array(v.buffer, v.byteOffset, v.byteLength))) this.heard(m);
      });
      await n.startNotifications?.();
    } catch {
      /* the gimbal moves without anyone listening */
    }
    this.name = this.dev.name ?? 'Osmo gimbal';
    this.connected = true;
    this.onChange();
  }

  disconnect() {
    this.dev?.gatt?.disconnect();
    this.connected = false;
    this.onChange();
  }

  private async send(b: Uint8Array) {
    const w = this.writer;
    if (!w) return;
    // 20-byte pieces fit any link; a speed message is 21 bytes
    for (let i = 0; i < b.length; i += 20) {
      const part = b.slice(i, i + 20);
      try {
        if (w.writeValueWithoutResponse) await w.writeValueWithoutResponse(part);
        else await w.writeValue(part);
      } catch {
        /* a lost piece is covered by the next refresh */
      }
    }
  }

  private next() {
    this.seq = (this.seq % 65000) + 1;
    return this.seq;
  }

  /** Turns at the given rates (°/s) for `ms`, refreshed every 100 ms, then stops. */
  async turn(yaw: number, pitch: number, roll: number, ms: number) {
    const end = performance.now() + ms;
    while (performance.now() < end) {
      await this.send(speedMessage(this.next(), yaw, pitch, roll));
      await new Promise((r) => setTimeout(r, Math.min(100, Math.max(0, end - performance.now()))));
    }
    await this.stop();
  }

  async stop() {
    await this.send(speedMessage(this.next(), 0, 0, 0));
  }

  private holdTimer: ReturnType<typeof setInterval> | undefined;
  private rates = { yaw: 0, pitch: 0, roll: 0 };
  /** Keeps turning at the current rates (°/s), refreshed every 100 ms, until `release()`; `setRates` changes them
   * on the fly (a joystick). */
  setRates(yaw: number, pitch: number, roll: number) {
    this.rates = { yaw, pitch, roll };
    if (this.holdTimer) return;
    const tick = () => void this.send(speedMessage(this.next(), this.rates.yaw, this.rates.pitch, this.rates.roll));
    tick();
    this.holdTimer = setInterval(tick, 100);
  }
  hold(yaw: number, pitch: number, roll: number) {
    this.setRates(yaw, pitch, roll);
  }
  release() {
    clearInterval(this.holdTimer);
    this.holdTimer = undefined;
    this.rates = { yaw: 0, pitch: 0, roll: 0 };
    void this.stop();
  }
}
