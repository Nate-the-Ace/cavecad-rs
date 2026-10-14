// The Web Bluetooth transport against a stand-in for the browser's API: a device with services, characteristics that
// notify and record what is written to them, and a stand-in for the engine (the real one is Rust: decoding, acks and
// the notebook are tested in crates/cave-instrument and crates/cave-session). What is checked here is the transport:
// what the chooser is asked, which characteristics are listened to, that every frame goes to the engine as the right
// kind, that what the engine says to write is written where it belongs, and the commands Take shot writes.
import { WebBleInstrument, allServices } from '../src/lib/instrument/webble.ts';
import type { Catalog, Identified, InstrumentKind, LinkBridge, LinkEvents, LinkStatus, Step } from '../src/lib/survey-types.ts';

// The ids, as the engine's catalog gives them.
const X1_SERVICE = '6e400001-b5a3-f393-e0a9-e50e24dcca9e';
const X1_WRITE = '6e400002-b5a3-f393-e0a9-e50e24dcca9e';
const X1_NOTIFY = '6e400003-b5a3-f393-e0a9-e50e24dcca9e';
const SAP5_NOTIFY = X1_WRITE;
const SAP5_WRITE = X1_NOTIFY;
const CAVE_BLE_SERVICE = '137c4435-8a64-4bcb-93f1-3792c6bdc965';
const NAME_CHARACTERISTIC = '137c4435-8a64-4bcb-93f1-3792c6bdc966';
const COMMAND_CHARACTERISTIC = '137c4435-8a64-4bcb-93f1-3792c6bdc967';
const LEG_CHARACTERISTIC = '137c4435-8a64-4bcb-93f1-3792c6bdc968';
const short = (n: number) => `0000${n.toString(16)}-0000-1000-8000-00805f9b34fb`;
const [BRIC_MEASURE_SERVICE, BRIC_PRIMARY, BRIC_META, BRIC_ERRORS, BRIC_CONTROL_SERVICE, BRIC_CONTROL] = [0x58d0, 0x58d1, 0x58d2, 0x58d3, 0x58e0, 0x58e1].map(short);

const catalog: Catalog = {
  devices: [],
  names: [
    { prefix: 'CavwayX1-', kind: 'x1', label: 'Cavway X1' },
    { prefix: 'SAP6_', kind: 'sap6', label: 'SAP6' },
    { prefix: 'SAP5_', kind: 'sap5', label: 'SAP5' },
    { prefix: 'BRIC4_', kind: 'bric', label: 'BRIC4' },
    { prefix: 'BRIC5_', kind: 'bric', label: 'BRIC5' },
    { prefix: 'BRIC5-', kind: 'bric', label: 'BRIC5' },
    { prefix: 'DistoXBLE-', kind: 'unsupported', label: 'DistoX BLE' },
  ],
  x1: { service: X1_SERVICE, write: X1_WRITE, notify: X1_NOTIFY, takeShot: [0x64, 0x61, 0x74, 0x61, 0x3a, 1, 0x38, 0x0d, 0x0a] },
  sap5: { service: X1_SERVICE, write: SAP5_WRITE, notify: SAP5_NOTIFY },
  sap6: { service: CAVE_BLE_SERVICE, name: NAME_CHARACTERISTIC, command: COMMAND_CHARACTERISTIC, leg: LEG_CHARACTERISTIC, takeShot: [0x38] },
  bric: { measureService: BRIC_MEASURE_SERVICE, primary: BRIC_PRIMARY, meta: BRIC_META, errors: BRIC_ERRORS, controlService: BRIC_CONTROL_SERVICE, control: BRIC_CONTROL, takeShot: [115, 104, 111, 116], batteryService: short(0x180f), deviceInfoService: short(0x180a), settleMs: 15 },
};

const hex = (b: ArrayLike<number>) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join(' ');
let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const step = (extra: Partial<Step> = {}): Step => ({ frames: [], sends: [], snapshot: null, error: null, ok: true, settle_ms: null, sim_left: false, sim_note: null, ...extra });

/** A stand-in for the engine: tells instruments apart by name, records what it is given, answers as configured. */
class FakeBridge implements LinkBridge {
  connects: string[] = [];
  frames: { kind: string; hex: string }[] = [];
  flushes = 0;
  /** What to answer a frame with, by kind. */
  answer: Record<string, () => Step> = {};
  catalog() { return catalog; }
  identify(name: string, services: string[]): { identified: Identified | null; nordicUart: boolean } {
    const n = catalog.names.find((x) => name.startsWith(x.prefix));
    if (n) return { identified: { kind: n.kind as InstrumentKind, label: n.label, how: 'name' }, nordicUart: false };
    const has = (u: string) => services.some((s) => s.toLowerCase() === u);
    if (has(CAVE_BLE_SERVICE)) return { identified: { kind: 'sap6', label: 'SAP6', how: 'service' }, nordicUart: false };
    if (has(BRIC_MEASURE_SERVICE)) return { identified: { kind: 'bric', label: 'BRIC', how: 'service' }, nordicUart: false };
    return { identified: null, nordicUart: has(X1_SERVICE) };
  }
  connect(kind: string) { this.connects.push(kind); return step(); }
  frame(kind: string, bytes: Uint8Array) {
    this.frames.push({ kind, hex: hex(bytes) });
    return (this.answer[kind] ?? step)();
  }
  flushLink() { this.flushes += 1; return step(); }
}

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
function rig(opts: ConstructorParameters<typeof WebBleInstrument>[2] = { mode: 'auto' }) {
  const frames: string[] = [];
  const steps: Step[] = [];
  const statuses: { s: LinkStatus; d: string }[] = [];
  const ev: LinkEvents = { status: (s, d = '') => statuses.push({ s, d }), frame: (h) => frames.push(h), step: (st) => steps.push(st) };
  const bridge = new FakeBridge();
  return { frames, steps, statuses, bridge, inst: new WebBleInstrument(ev, bridge, opts) };
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
  ok(allServices(catalog).every((s) => o.optionalServices.includes(s)), 'every service the instruments use is named, or the browser would not let the page reach it');
  const any = rig({ mode: 'auto', anyDevice: true });
  install(x1());
  await any.inst.connect();
  ok((lastOptions as { acceptAllDevices?: boolean }).acceptAllDevices === true, 'with "show every device" it offers them all');
}

// ---- the Cavway X1 ----
{
  const dev = x1();
  const r = rig();
  r.bridge.answer.frame = () => step({ sends: [[0x64, 0x61, 0x74, 0x61, 0x3a, 1, 0xff, 0x0d, 0x0a]] });
  install(dev);
  await r.inst.connect();
  ok(r.inst.kind === 'x1' && r.inst.canTakeShot && r.inst.name === 'Cavway X1 CavwayX1-0042', 'a CavwayX1- device is the X1: it can be told to fire, and its name is shown');
  ok(r.bridge.connects.join() === 'x1', 'the engine is told it is an X1, once');
  ok(r.statuses.at(-1)?.s === 'connected' && char(dev, X1_SERVICE, X1_NOTIFY).notifying, 'it is connected and notifications are on');
  char(dev, X1_SERVICE, X1_NOTIFY).send(Uint8Array.from([1, 2, 3]));
  await sleep(40);
  ok(r.bridge.frames.length === 1 && r.bridge.frames[0].kind === 'frame' && r.bridge.frames[0].hex === '01 02 03', 'a packet goes to the engine as it came');
  ok(r.steps.length >= 2, 'and the engine\'s answer reaches the store');
  const w = char(dev, X1_SERVICE, X1_WRITE).written;
  ok(w.length === 1 && hex(w[0].bytes) === '64 61 74 61 3a 01 ff 0d 0a' && !w[0].response, 'what the engine says to write is written on the write characteristic, without a response');
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
  ok(r.inst.kind === 'sap5' && !r.inst.canTakeShot && r.inst.name === 'SAP5 SAP5_9f' && r.bridge.connects.join() === 'sap5', 'a SAP5_ device is the SAP5; it is not known to be told to fire');
  char(dev, X1_SERVICE, SAP5_NOTIFY).send(Uint8Array.from([1, 0, 0, 0, 0, 0, 0, 0]));
  await sleep(30);
  ok(r.bridge.frames.length === 1 && r.bridge.frames[0].kind === 'frame', 'its 8-byte packet goes to the engine');
  ok(char(dev, X1_SERVICE, SAP5_NOTIFY).written.length === 0 && char(dev, X1_SERVICE, SAP5_WRITE).written.length === 0, 'nothing is written to it');
  r.inst.takeShot();
  await sleep(150);
  ok(char(dev, X1_SERVICE, SAP5_WRITE).written.length === 0, 'and Take shot writes nothing (the command is not known)');
}

// ---- the SAP6 ----
{
  const dev = sap6();
  const r = rig();
  r.bridge.answer.frame = () => step({ sends: [[0x56]] });
  install(dev);
  await r.inst.connect();
  ok(r.inst.kind === 'sap6' && r.inst.canTakeShot && r.inst.name === 'SAP6 SAP6_9f' && r.bridge.connects.join() === 'sap6', 'a SAP6_ device is the SAP6');
  ok(r.frames.includes('protocol: SAP6'), 'the protocol name it reports is logged');
  char(dev, CAVE_BLE_SERVICE, LEG_CHARACTERISTIC).send(Uint8Array.from([1, 9, 9]));
  await sleep(40);
  ok(r.bridge.frames.length === 1, 'a leg frame goes to the engine');
  ok(hex(char(dev, CAVE_BLE_SERVICE, COMMAND_CHARACTERISTIC).written[0].bytes) === '56', 'and what the engine answers is written to the command characteristic');
  r.inst.takeShot();
  await sleep(250);
  ok(hex(char(dev, CAVE_BLE_SERVICE, COMMAND_CHARACTERISTIC).written.at(-1)!.bytes) === '38', 'Take shot writes 0x38');
}

// ---- the BRIC ----
{
  const dev = bric('BRIC4_07');
  const r = rig();
  r.bridge.answer.primary = () => step({ settle_ms: catalog.bric.settleMs });
  r.bridge.answer.meta = () => step({ settle_ms: catalog.bric.settleMs });
  install(dev);
  await r.inst.connect();
  ok(r.inst.kind === 'bric' && r.inst.canTakeShot && r.inst.name === 'BRIC4 BRIC4_07' && r.bridge.connects.join() === 'bric', 'a BRIC4_ device is a BRIC4');
  const chars = [BRIC_PRIMARY, BRIC_META, BRIC_ERRORS].map((u) => char(dev, BRIC_MEASURE_SERVICE, u));
  ok(chars.every((c) => c.notifying), 'indications are turned on for the primary, meta and error characteristics');
  chars[0].send(Uint8Array.from([1]));
  chars[1].send(Uint8Array.from([2]));
  await sleep(5);
  ok(r.bridge.frames.map((f) => f.kind).join() === 'primary,meta', 'indications go to the engine as what they are');
  ok(r.bridge.flushes === 0, 'the engine has not been asked to settle yet');
  await sleep(60);
  ok(r.bridge.flushes === 1, 'when the engine says a shot is still arriving and no error indication follows, it is asked to settle after the time it names');
  chars[0].send(Uint8Array.from([1]));
  chars[2].send(Uint8Array.from([3]));
  await sleep(60);
  ok(r.bridge.frames.map((f) => f.kind).join() === 'primary,meta,primary,errors' && r.bridge.flushes === 1, 'but when the error indication completes the shot there is nothing to settle');
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
  char(dev, X1_SERVICE, SAP5_NOTIFY).send(Uint8Array.from([1, 0, 0, 0, 0, 0, 0, 0]));
  await sleep(20);
  ok(n.inst.kind === 'sap5' && n.bridge.connects.join() === 'x1,sap5', 'an 8-byte packet makes it a SAP5, and the engine is told');
  char(dev, X1_SERVICE, X1_NOTIFY).send(new Uint8Array(40));
  await sleep(20);
  ok(n.inst.kind === 'x1' && n.bridge.connects.join() === 'x1,sap5,x1' && n.bridge.frames.length === 2, 'a long packet makes it an X1 again');
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
  ok(r.bridge.connects.join() === 'capture' && r.inst.canTakeShot === false, 'the engine is told it is a capture, and nothing can be fired');
  char(dev, X1_SERVICE, X1_NOTIFY).send(Uint8Array.from([1, 2, 255]));
  await sleep(10);
  ok(r.frames.includes('01 02 ff') && r.bridge.frames.length === 0, 'every frame it sends is logged as hex and none goes to be decoded');
}

if (bad) process.exit(1);
console.log('webble ok');
