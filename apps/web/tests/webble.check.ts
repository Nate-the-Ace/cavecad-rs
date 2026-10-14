// The Web Bluetooth transport against a stand-in for the browser's API: a device with services, characteristics that
// notify and record what is written to them. Each supported instrument is "connected" and its packets sent; what
// comes out (readings, acknowledgements, the command a Take shot writes, the status) is checked.
import { ALL_SERVICES, WebBleInstrument } from '../src/lib/instrument/webble.ts';
import { CAVE_BLE_SERVICE, COMMAND_CHARACTERISTIC, LEG_CHARACTERISTIC, NAME_CHARACTERISTIC, encodeLeg, hex } from '../src/lib/instrument/caveble.ts';
import { X1_NOTIFY, X1_SERVICE, X1_WRITE, encodePacket } from '../src/lib/instrument/cavway.ts';
import { BRIC_CONTROL, BRIC_CONTROL_SERVICE, BRIC_ERRORS, BRIC_MEASURE_SERVICE, BRIC_META, BRIC_PRIMARY, encodeShot } from '../src/lib/instrument/bric.ts';
import { SAP5_NOTIFY, SAP5_WRITE, encodeDistoXPacket } from '../src/lib/instrument/sap5.ts';
import type { LinkEvents, LinkStatus, Reading } from '../src/lib/instrument/types.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

class FakeChar {
  uuid: string;
  properties: { notify: boolean; indicate: boolean; read: boolean };
  written: { bytes: Uint8Array; response: boolean }[] = [];
  notifying = false;
  private listeners: ((e: Event) => void)[] = [];
  private readonly valueText: string;
  constructor(uuid: string, how: { notify?: boolean; indicate?: boolean; read?: string } = {}) {
    this.uuid = uuid;
    this.properties = { notify: !!how.notify, indicate: !!how.indicate, read: how.read !== undefined };
    this.valueText = how.read ?? '';
  }
  async startNotifications() { this.notifying = true; return this; }
  async readValue() { return new DataView(new TextEncoder().encode(this.valueText).buffer); }
  async writeValueWithoutResponse(v: ArrayBufferView) { this.written.push({ bytes: Uint8Array.from(new Uint8Array(v.buffer, v.byteOffset, v.byteLength)), response: false }); }
  async writeValue(v: ArrayBufferView) { this.written.push({ bytes: Uint8Array.from(new Uint8Array(v.buffer, v.byteOffset, v.byteLength)), response: true }); }
  addEventListener(_t: string, l: (e: Event) => void) { this.listeners.push(l); }
  /** The instrument sends something. */
  send(bytes: Uint8Array) {
    const value = new DataView(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
    for (const l of this.listeners) l({ target: { value } } as unknown as Event);
  }
}
class FakeService {
  uuid: string;
  chars: FakeChar[];
  constructor(uuid: string, chars: FakeChar[]) { this.uuid = uuid; this.chars = chars; }
  async getCharacteristics() { return this.chars; }
  async getCharacteristic(uuid: string) {
    const c = this.chars.find((x) => x.uuid === uuid);
    if (!c) throw new Error(`no characteristic ${uuid}`);
    return c;
  }
}
class FakeDevice {
  name?: string;
  services: FakeService[];
  connected = false;
  constructor(name: string | undefined, services: FakeService[]) { this.name = name; this.services = services; }
  gatt = {
    connected: false,
    connect: async () => { this.connected = true; return this.gatt; },
    disconnect: () => { this.connected = false; },
    getPrimaryServices: async () => this.services,
    getPrimaryService: async (uuid: string) => {
      const s = this.services.find((x) => x.uuid === uuid);
      if (!s) throw new Error(`no service ${uuid}`);
      return s;
    },
  };
  addEventListener() {}
}

// The instruments, as a device each.
const x1 = () => new FakeDevice('CavwayX1-0042', [new FakeService(X1_SERVICE, [new FakeChar(X1_WRITE), new FakeChar(X1_NOTIFY, { notify: true })])]);
const sap5 = () => new FakeDevice('SAP5_9f', [new FakeService(X1_SERVICE, [new FakeChar(SAP5_NOTIFY, { notify: true }), new FakeChar(SAP5_WRITE, { notify: true })])]);
const sap6 = (name = 'SAP6_9f') => new FakeDevice(name, [new FakeService(CAVE_BLE_SERVICE, [new FakeChar(NAME_CHARACTERISTIC, { read: 'SAP6' }), new FakeChar(COMMAND_CHARACTERISTIC), new FakeChar(LEG_CHARACTERISTIC, { notify: true })])]);
const bric = (name = 'BRIC5_01') => new FakeDevice(name, [
  new FakeService(BRIC_MEASURE_SERVICE, [new FakeChar(BRIC_PRIMARY, { indicate: true }), new FakeChar(BRIC_META, { indicate: true }), new FakeChar(BRIC_ERRORS, { indicate: true })]),
  new FakeService(BRIC_CONTROL_SERVICE, [new FakeChar(BRIC_CONTROL, { read: '' })]),
]);

let lastOptions: unknown = null;
function install(device: FakeDevice | Error) {
  Object.defineProperty(globalThis, 'navigator', {
    configurable: true,
    value: { bluetooth: { requestDevice: async (o: unknown) => { lastOptions = o; if (device instanceof Error) throw device; return device; } } },
  });
}
function rig(opts: ConstructorParameters<typeof WebBleInstrument>[1] = { mode: 'auto' }) {
  const readings: Reading[] = [];
  const frames: string[] = [];
  const statuses: { s: LinkStatus; d: string }[] = [];
  const ev: LinkEvents = { status: (s, d = '') => statuses.push({ s, d }), frame: (h) => frames.push(h), reading: (r) => readings.push(r) };
  return { readings, frames, statuses, inst: new WebBleInstrument(ev, opts) };
}
const char = (d: FakeDevice, service: string, uuid: string) => d.services.find((s) => s.uuid === service)!.chars.find((c) => c.uuid === uuid)!;

// ---- what the chooser is asked ----
{
  const r = rig();
  install(x1());
  await r.inst.connect();
  const o = lastOptions as { filters: { namePrefix?: string; services?: string[] }[]; optionalServices: string[] };
  ok(Array.isArray(o.filters) && ['CavwayX1-', 'SAP6_', 'SAP5_', 'BRIC4_', 'BRIC5_'].every((p) => o.filters.some((f) => f.namePrefix === p)), 'the chooser offers the instruments by the names they advertise');
  ok(o.filters.some((f) => f.services?.[0] === CAVE_BLE_SERVICE) && o.filters.some((f) => f.services?.[0] === BRIC_MEASURE_SERVICE), 'and by the services only one of them has');
  ok(ALL_SERVICES.every((s) => o.optionalServices.includes(s)), 'every service the instruments use is named, or the browser would not let the page reach it');
  const any = rig({ mode: 'auto', anyDevice: true });
  install(x1());
  await any.inst.connect();
  ok((lastOptions as { acceptAllDevices?: boolean }).acceptAllDevices === true, 'with "show every device" it offers them all');
}

// ---- the Cavway X1 ----
{
  const dev = x1();
  const r = rig();
  install(dev);
  await r.inst.connect();
  ok(r.inst.kind === 'x1' && r.inst.canTakeShot && r.inst.groupsLegs && r.inst.name === 'Cavway X1 CavwayX1-0042', 'a CavwayX1- device is the X1: it can be told to fire, and its name is shown');
  ok(r.statuses.at(-1)?.s === 'connected' && char(dev, X1_SERVICE, X1_NOTIFY).notifying, 'it is connected and notifications are on');
  char(dev, X1_SERVICE, X1_NOTIFY).send(encodePacket({ distance: 4.25, azimuth: 120, inclination: 5, leg: false, time: 7 }));
  await sleep(40);
  ok(r.readings.length === 1 && r.readings[0].distance === 4.25 && r.readings[0].source === 'x1', 'a packet becomes a reading');
  const w = char(dev, X1_SERVICE, X1_WRITE).written;
  ok(w.length === 1 && hex(w[0].bytes) === '64 61 74 61 3a 01 ff 0d 0a' && !w[0].response, 'and is acknowledged on the write characteristic, framed, without a response');
  r.inst.takeShot();
  await sleep(250);
  ok(hex(char(dev, X1_SERVICE, X1_WRITE).written.at(-1)!.bytes) === '64 61 74 61 3a 01 38 0d 0a', 'Take shot writes the framed laser trigger');
}

// ---- the SAP5 ----
{
  const dev = sap5();
  const r = rig();
  install(dev);
  await r.inst.connect();
  ok(r.inst.kind === 'sap5' && !r.inst.canTakeShot && r.inst.name === 'SAP5 SAP5_9f', 'a SAP5_ device is the SAP5; it is not known to be told to fire');
  char(dev, X1_SERVICE, SAP5_NOTIFY).send(encodeDistoXPacket({ distance: 6.5, azimuth: 200, inclination: -4, seq: 1 }));
  await sleep(30);
  ok(r.readings.length === 1 && r.readings[0].distance === 6.5 && r.readings[0].source === 'sap5', 'its 8-byte packet becomes a reading');
  ok(char(dev, X1_SERVICE, SAP5_NOTIFY).written.length === 0 && char(dev, X1_SERVICE, SAP5_WRITE).written.length === 0, 'nothing is written to it');
  r.inst.takeShot();
  await sleep(150);
  ok(char(dev, X1_SERVICE, SAP5_WRITE).written.length === 0, 'and Take shot writes nothing (the command is not known)');
}

// ---- the SAP6 ----
{
  const dev = sap6();
  const r = rig();
  install(dev);
  await r.inst.connect();
  ok(r.inst.kind === 'sap6' && r.inst.canTakeShot && r.inst.name === 'SAP6 SAP6_9f', 'a SAP6_ device is the SAP6');
  ok(r.frames.includes('protocol: SAP6'), 'the protocol name it reports is logged');
  char(dev, CAVE_BLE_SERVICE, LEG_CHARACTERISTIC).send(encodeLeg({ seq: 1, azimuth: 90, inclination: -5, roll: 0, distance: 3.5 }));
  await sleep(40);
  ok(r.readings.length === 1 && r.readings[0].distance === 3.5 && r.readings[0].source === 'sap6', 'a leg frame becomes a reading');
  ok(hex(char(dev, CAVE_BLE_SERVICE, COMMAND_CHARACTERISTIC).written[0].bytes) === '56', 'and is acknowledged with 0x56 (0x55 plus its sequence bit 1)');
  r.inst.takeShot();
  await sleep(250);
  ok(hex(char(dev, CAVE_BLE_SERVICE, COMMAND_CHARACTERISTIC).written.at(-1)!.bytes) === '38', 'Take shot writes 0x38');
}

// ---- the BRIC ----
{
  const dev = bric('BRIC4_07');
  const r = rig();
  install(dev);
  await r.inst.connect();
  ok(r.inst.kind === 'bric' && r.inst.canTakeShot && r.inst.name === 'BRIC4 BRIC4_07', 'a BRIC4_ device is a BRIC4');
  const chars = [BRIC_PRIMARY, BRIC_META, BRIC_ERRORS].map((u) => char(dev, BRIC_MEASURE_SERVICE, u));
  ok(chars.every((c) => c.notifying), 'indications are turned on for the primary, meta and error characteristics');
  const shot = encodeShot({ distance: 8.5, azimuth: 33, inclination: 2, index: 4, roll: 7 });
  chars[0].send(shot.primary);
  chars[1].send(shot.meta);
  chars[2].send(shot.errors);
  await sleep(20);
  ok(r.readings.length === 1 && r.readings[0].distance === 8.5 && r.readings[0].roll === 7 && r.readings[0].source === 'bric', 'three indications make a reading');
  r.inst.takeShot();
  await sleep(150);
  const sent = char(dev, BRIC_CONTROL_SERVICE, BRIC_CONTROL).written;
  ok(sent.length === 1 && new TextDecoder().decode(sent[0].bytes) === 'shot' && sent[0].response, 'Take shot writes the ASCII word "shot" to the control characteristic, with a response');
  const five = rig();
  install(bric('BRIC5-02'));
  await five.inst.connect();
  ok(five.inst.name === 'BRIC5 BRIC5-02', 'BRIC5- is a BRIC5');
}

// ---- told by what it offers, when the name is not known ----
{
  const r = rig({ mode: 'auto', anyDevice: true });
  install(sap6('Pony'));
  await r.inst.connect();
  ok(r.inst.kind === 'sap6', 'an unknown name offering the CaveBLE service is a SAP6');
  const b = rig({ mode: 'auto', anyDevice: true });
  install(bric('Mystery'));
  await b.inst.connect();
  ok(b.inst.kind === 'bric', 'one offering the 58d0 service is a BRIC');
  // An unknown name on Nordic UART: told by the packets.
  // Characteristic 0002 notifies on a SAP5 and is written to on an X1: a device whose name is unknown might be either, so both can notify here.
  const dev = new FakeDevice('Mystery', [new FakeService(X1_SERVICE, [new FakeChar(SAP5_NOTIFY, { notify: true }), new FakeChar(X1_NOTIFY, { notify: true })])]);
  const n = rig({ mode: 'auto', anyDevice: true });
  install(dev);
  await n.inst.connect();
  ok(n.inst.kind === 'nus' && n.statuses.at(-1)?.s === 'connected', 'an unknown name on Nordic UART connects, not yet knowing which');
  char(dev, X1_SERVICE, SAP5_NOTIFY).send(encodeDistoXPacket({ distance: 2.5, azimuth: 10, inclination: 3, seq: 0 }));
  await sleep(20);
  ok(n.readings.length === 1 && n.readings[0].source === 'sap5', 'an 8-byte packet is a SAP5 shot');
  char(dev, X1_SERVICE, X1_NOTIFY).send(encodePacket({ distance: 5.5, azimuth: 80, inclination: 1, time: 3 }));
  await sleep(20);
  ok(n.readings.length === 2 && n.readings[1].source === 'x1' && n.inst.kind === 'x1', 'a long packet is an X1 shot, and it is then known to be one');
}

// ---- what goes wrong ----
{
  const r = rig();
  install(new FakeDevice('DistoXBLE-12', [new FakeService(X1_SERVICE, [])]));
  await r.inst.connect();
  ok(r.statuses.at(-1)?.s === 'error' && /not supported yet/.test(r.statuses.at(-1)?.d ?? ''), 'a DistoX BLE is recognised and said to be not supported yet');
  const q = rig();
  install(new FakeDevice('Headphones', [new FakeService('0000180f-0000-1000-8000-00805f9b34fb', [])]));
  await q.inst.connect();
  ok(q.statuses.at(-1)?.s === 'error' && /none of the services/.test(q.statuses.at(-1)?.d ?? ''), 'a device with none of the instruments\' services is refused, with what to try');
  const c = rig();
  install(new Error('User cancelled the requestDevice() chooser.'));
  await c.inst.connect();
  ok(c.statuses.at(-1)?.s === 'idle', 'closing the chooser is not an error');
  const e = rig();
  install(new Error('Bluetooth adapter is not available.'));
  await e.inst.connect();
  ok(e.statuses.at(-1)?.s === 'error' && /adapter/.test(e.statuses.at(-1)?.d ?? ''), 'any other failure is said');
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: {} });
  const u = rig();
  await u.inst.connect();
  ok(u.statuses.at(-1)?.s === 'unsupported' && /Bluefy/.test(u.statuses.at(-1)?.d ?? ''), 'a browser without Web Bluetooth says so and names Bluefy for the iPad');
}

// ---- capture ----
{
  const dev = x1();
  const r = rig({ mode: 'capture', services: ['0000ffe0-0000-1000-8000-00805f9b34fb'] });
  install(dev);
  await r.inst.connect();
  const o = lastOptions as { acceptAllDevices?: boolean; optionalServices: string[] };
  ok(o.acceptAllDevices === true && o.optionalServices.includes('0000ffe0-0000-1000-8000-00805f9b34fb'), 'capture asks for every device and the services it was told about');
  ok(r.frames.some((f) => f.startsWith('found ') && f.includes(X1_NOTIFY)), 'it lists every characteristic it finds');
  ok(r.inst.groupsLegs === false && r.inst.canTakeShot === false, 'and groups nothing and fires nothing');
  char(dev, X1_SERVICE, X1_NOTIFY).send(Uint8Array.from([1, 2, 255]));
  await sleep(10);
  ok(r.frames.includes('01 02 ff') && r.readings.length === 0, 'every frame it sends is logged as hex and is never a reading');
}

if (bad) process.exit(1);
console.log('webble ok');
