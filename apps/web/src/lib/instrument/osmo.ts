// A DJI Osmo Mobile gimbal over Web Bluetooth (in the apps, the native bridge stands in for navigator.bluetooth).
// DJI publishes nothing; the gimbal speaks DJI's DUML messages on service fff0 (write fff5, it talks back on fff4).
// What the Osmo Mobile 7 actually does is measured in tools/cave-osmo/FINDINGS.md: only SPEED commands move it (no
// angle command works, it reports no angles), timed turns land within a few per cent, and its clamp holds a strong
// magnet that upsets an instrument's compass. Kept behind this small class so another pan-tilt head can replace it.

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

/** One message to the gimbal (sender app 0x02, receiver gimbal 0x04). */
export function duml(seq: number, set: number, cmd: number, payload: number[], attr = 0): Uint8Array {
  const len = (13 + payload.length) | (1 << 10);
  const head = [0x55, len & 0xff, len >> 8];
  const body = [0x02, 0x04, seq & 0xff, (seq >> 8) & 0xff, attr, set, cmd, ...payload];
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

type Char = { writeValueWithoutResponse?: (d: BufferSource) => Promise<void>; writeValue: (d: BufferSource) => Promise<void>; startNotifications?: () => Promise<unknown> };
type Dev = { name?: string; gatt?: { connect: () => Promise<{ getPrimaryService: (u: string) => Promise<{ getCharacteristic: (u: string) => Promise<Char> }> }>; disconnect: () => void; connected?: boolean }; addEventListener: (e: string, f: () => void) => void };

export class OsmoGimbal {
  name = '';
  connected = false;
  private dev: Dev | null = null;
  private writer: Char | null = null;
  private seq = 1 + Math.floor(Math.random() * 60000);
  onChange: () => void = () => {};

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
      await (await svc.getCharacteristic(NOTIFY)).startNotifications?.();
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
