// Web Bluetooth transport. Chromium browsers have it, and so does Bluefy on an iPad; Safari does not.
// One Connect serves every supported instrument: the person picks a device in the browser's chooser (offered the
// instruments by the names they advertise), and this works out which it is (`identify.ts`) and speaks its protocol:
// the Cavway X1 and the SAP5 (Nordic UART), the SAP6 (CaveBLE), the BRIC4 and BRIC5 (service 0x58D0). `capture`
// mode instead connects to any device and writes every frame it sends to the log as hex, for an instrument whose
// protocol is not known yet.
//
// This file is only the transport: it opens the connection, finds the characteristics, and hands every frame to
// the Rust engine through a `LinkBridge` (decoding, acknowledging, repeats, leg grouping and the notebook writes
// are `cave-instrument` and `cave-session`). The ids, names and commands come from the engine's catalog, so the
// browser keeps no copy. Written against the Web Bluetooth API without a real instrument; the connecting is
// tested against a stand-in for the API (tests/webble.check.ts). The first real connection is the first real test.

import type { Catalog, Instrument, InstrumentKind, LinkBridge, LinkEvents, Step } from '../survey-types';

// The Web Bluetooth types are not in the DOM lib this project builds against: only what is used.
interface Gatt {
  connected: boolean;
  connect(): Promise<Gatt>;
  disconnect(): void;
  getPrimaryServices(): Promise<GattService[]>;
  getPrimaryService(uuid: string): Promise<GattService>;
}
interface GattService {
  uuid: string;
  getCharacteristics(): Promise<GattCharacteristic[]>;
  getCharacteristic(uuid: string): Promise<GattCharacteristic>;
}
interface GattCharacteristic {
  uuid: string;
  value?: DataView;
  properties: { notify: boolean; indicate: boolean; read: boolean };
  startNotifications(): Promise<GattCharacteristic>;
  readValue(): Promise<DataView>;
  writeValueWithoutResponse?(v: ArrayBufferView): Promise<void>;
  writeValue(v: ArrayBufferView): Promise<void>;
  addEventListener(type: 'characteristicvaluechanged', l: (e: Event) => void): void;
}
interface BtDevice {
  name?: string;
  gatt?: Gatt;
  addEventListener(type: 'gattserverdisconnected', l: () => void): void;
}
interface BtApi {
  requestDevice(o: unknown): Promise<BtDevice>;
}

const api = (): BtApi | undefined => (navigator as unknown as { bluetooth?: BtApi }).bluetooth;

/** True when this browser can talk to Bluetooth devices from a page. */
export const webBluetoothAvailable = (): boolean => api() !== undefined;

const bytesOf = (v: DataView) => new Uint8Array(v.buffer, v.byteOffset, v.byteLength);
const valueOf = (e: Event) => (e.target as unknown as GattCharacteristic).value;

/** The instruments' microcontrollers need a gap between frames (TopoDroid waits 100 ms): writes go one at a time, this far apart. */
const WRITE_GAP_MS = 100;

/** Every service the supported instruments use. The browser only lets a page reach services it named when asking, so all of them are named. */
export const allServices = (c: Catalog): string[] => [c.x1.service, c.sap6.service, c.bric.measureService, c.bric.controlService, c.bric.batteryService, c.bric.deviceInfoService];

const hex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join(' ');

export interface WebBleOptions {
  /** `auto`: any supported instrument, told apart by its name. `capture`: any device, log every frame. */
  mode: 'auto' | 'capture';
  /** In capture mode: more service uuids to be allowed to reach (the browser cannot list what it was not told). */
  services?: string[];
  /** Offer every Bluetooth device, not only those advertising a known instrument name (an instrument with a name this does not know). */
  anyDevice?: boolean;
}

export class WebBleInstrument implements Instrument {
  name = 'Bluetooth instrument';
  /** What was found: true once an instrument that can be told to fire is connected. */
  canTakeShot = false;
  /** Which instrument it turned out to be, once known. */
  kind: InstrumentKind | 'nus' | null = null;
  private device: BtDevice | null = null;
  private writer: GattCharacteristic | null = null;
  private withResponse = false;
  private readonly events: LinkEvents;
  private readonly bridge: LinkBridge;
  private readonly opts: WebBleOptions;
  private readonly cat: Catalog;
  /** The first reconnect wait (ms); it doubles up to eight times this. Tests make it short. */
  static retryMs = 500;
  private wanted = false;
  /** A reconnect loop is running (one at a time). */
  private retrying = false;
  /** The engine has been told which instrument this is (once per connection: a reconnect keeps the link). */
  private linked = false;
  private settle: ReturnType<typeof setTimeout> | undefined;
  private queue: Promise<void> = Promise.resolve();

  constructor(events: LinkEvents, bridge: LinkBridge, opts: WebBleOptions) {
    this.events = events;
    this.bridge = bridge;
    this.opts = opts;
    this.cat = bridge.catalog();
  }

  async connect() {
    const bt = api();
    if (!bt) {
      const inApp = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
      this.events.status(
        'unsupported',
        inApp
          ? "This copy of the desktop app has no Bluetooth. Update it, or connect from Chrome at cavecad.com."
          : 'This browser has no Web Bluetooth. On an iPad, open CaveCAD in the Bluefy browser.',
      );
      return;
    }
    this.events.status('connecting');
    try {
      this.device = await bt.requestDevice(this.chooser());
      this.wanted = true;
      this.device.addEventListener('gattserverdisconnected', () => void this.lost());
      await this.attach();
    } catch (e) {
      // The person closing the chooser is not an error worth shouting about.
      const msg = e instanceof Error ? e.message : String(e);
      this.events.status(/cancel|chosen/i.test(msg) ? 'idle' : 'error', msg);
    }
  }

  disconnect() {
    this.wanted = false;
    clearTimeout(this.settle);
    this.device?.gatt?.disconnect();
    this.events.status('idle');
  }

  /** Fires the instrument, as pressing its own button does (where that is known to work). */
  takeShot() {
    const c = this.cat;
    if (this.kind === 'x1') this.write(Uint8Array.from(c.x1.takeShot));
    else if (this.kind === 'sap6') this.write(Uint8Array.from(c.sap6.takeShot));
    else if (this.kind === 'bric') this.write(Uint8Array.from(c.bric.takeShot));
  }

  /** The X1's laser on or off (other instruments have no such command known). */
  get canLaser() {
    return this.kind === 'x1' && this.canTakeShot;
  }

  laser(on: boolean) {
    if (this.kind === 'x1') this.write(Uint8Array.from(on ? this.cat.x1.laserOn : this.cat.x1.laserOff));
  }

  /** A single-byte X1 command, framed (CaveCAD firmware's scan control: 39 start, 3A stop, 3B status). */
  x1Command(cmd: number) {
    if (this.kind === 'x1' && this.canTakeShot) this.write(Uint8Array.from([0x64, 0x61, 0x74, 0x61, 0x3a, 1, cmd & 0xff, 0x0d, 0x0a]));
  }

  /** Raw bytes to the X1 in pieces (its Bluetooth module takes 20 bytes a write), for firmware flashing. Resolves once
   * all pieces are written. Pings and other writes are held back while `quiet` is set. */
  async x1Raw(data: Uint8Array, piece = 20, gapMs = 12) {
    const c = this.writer;
    if (!c || this.kind !== 'x1') throw new Error('No X1 connected');
    for (let at = 0; at < data.length; at += piece) {
      const part = data.slice(at, at + piece);
      if (c.writeValueWithoutResponse) await c.writeValueWithoutResponse(part);
      else await c.writeValue(part);
      await new Promise((r) => setTimeout(r, gapMs));
    }
  }
  /** While flashing: no keep-awake pings or clock sets into the bootloader's stream. */
  quiet = false;

  private awakeTimer: ReturnType<typeof setInterval> | undefined;
  /** Keeps an X1 from going to sleep: a harmless one-byte memory read (3d, address 0, 1 byte) every 45 s, the only X1
   * command known to change nothing (cave-x1 FINDINGS; no sleep setting is known, and memory writes are refused). */
  keepAwake(on: boolean) {
    clearInterval(this.awakeTimer);
    this.awakeTimer = undefined;
    if (!on) return;
    const ping = Uint8Array.from([0x64, 0x61, 0x74, 0x61, 0x3a, 4, 0x3d, 0x00, 0x00, 0x01, 0x0d, 0x0a]);
    this.awakeTimer = setInterval(() => {
      if (this.kind === 'x1' && this.canTakeShot && !this.quiet) this.write(ping);
    }, 45_000);
  }

  /** What the browser's device chooser is asked to show: the known instruments by their names, or everything. */
  private chooser(): unknown {
    const reach = [...allServices(this.cat), ...(this.opts.services ?? [])];
    if (this.opts.mode === 'capture' || this.opts.anyDevice) return { acceptAllDevices: true, optionalServices: reach };
    return {
      filters: [...this.cat.names.map((n) => ({ namePrefix: n.prefix })), { services: [this.cat.sap6.service] }, { services: [this.cat.bric.measureService] }],
      optionalServices: reach,
    };
  }

  /** Queues a write: one at a time, with a gap, and never throwing (a lost ack is sent again by the instrument). */
  private write(data: Uint8Array) {
    (globalThis as { __cavecadDlog?: (s: string) => void }).__cavecadDlog?.(`send ${hex(data)}`);
    this.queue = this.queue.then(async () => {
      const c = this.writer;
      if (!c) return;
      try {
        if (!this.withResponse && c.writeValueWithoutResponse) await c.writeValueWithoutResponse(data);
        else await c.writeValue(data);
      } catch {
        /* nothing to do: see above */
      }
      await new Promise((r) => setTimeout(r, WRITE_GAP_MS));
    });
  }

  private listen(c: GattCharacteristic, handle: (bytes: Uint8Array) => void) {
    c.addEventListener('characteristicvaluechanged', (e) => {
      const v = valueOf(e);
      if (v) handle(bytesOf(v));
    });
  }

  /** The engine's answer to something: write back what it says to, let the store show the rest, and ask to be woken if a shot is still arriving. */
  private take(step: Step) {
    for (const bytes of step.sends) this.write(Uint8Array.from(bytes));
    this.events.step(step);
    clearTimeout(this.settle);
    if (step.settle_ms !== null) this.settle = setTimeout(() => this.take(this.bridge.flushLink()), step.settle_ms);
  }

  /** A frame from the instrument goes to the engine. */
  private frame(kind: string, bytes: Uint8Array) {
    (globalThis as { __cavecadDlog?: (s: string) => void }).__cavecadDlog?.(`recv ${kind} ${hex(bytes)}`);
    // a listener outside the engine (the gimbal dome scan reads each shot's angle to steer the next move)
    (globalThis as { __cavecadShotTap?: (b: Uint8Array) => void }).__cavecadShotTap?.(bytes);
    // X1 firmware flashing listens to the bootloader's answers (lib/x1firmware.svelte.ts)
    (globalThis as { __cavecadFwTap?: (b: Uint8Array) => void }).__cavecadFwTap?.(bytes);
    const step = this.bridge.frame(kind, bytes);
    // what the engine made of it (a leg, a splay, held back), for reading the log afterwards
    const last = step.snapshot?.log?.[step.snapshot.log.length - 1];
    (globalThis as { __cavecadDlog?: (s: string) => void }).__cavecadDlog?.(`engine waiting=${step.snapshot?.waiting ?? '?'} stored=${step.snapshot?.stored?.length ?? 0} last=${last ? `${last.label}` : '-'}`);
    this.take(step);
  }

  /** Tells the engine which instrument this is, once. */
  private link(kind: string) {
    if (this.linked) return;
    this.linked = true;
    this.take(this.bridge.connect(kind));
  }

  private async attach() {
    const gatt = await this.device!.gatt!.connect();
    this.writer = null;
    this.withResponse = false;
    this.canTakeShot = false;
    const services = (await gatt.getPrimaryServices()).map((s) => s.uuid);
    if (this.opts.mode === 'capture') {
      this.link('capture');
      await this.attachCapture(gatt);
      this.name = this.device?.name || 'Bluetooth device';
      this.events.status('connected', this.name);
      return;
    }
    const { identified: found, nordicUart } = this.bridge.identify(this.device?.name ?? '', services);
    const kind: InstrumentKind | 'nus' | null = found?.kind ?? (nordicUart ? 'nus' : null);
    if (kind === null) throw new Error('This device offers none of the services of a supported instrument. Try "Show every Bluetooth device" under the capture settings, or capture its frames to find out what it sends.');
    if (kind === 'unsupported') throw new Error(`${found?.label ?? 'This instrument'} is recognised but not supported yet.`);
    this.kind = kind;
    // A Nordic UART instrument whose name is not known is told by its packets (below); until then it is read as an X1.
    this.link(kind === 'nus' ? 'x1' : kind);
    if (kind === 'x1') await this.attachX1(gatt);
    else if (kind === 'sap5') await this.attachSap5(gatt);
    else if (kind === 'sap6') await this.attachSap6(gatt);
    else if (kind === 'bric') await this.attachBric(gatt);
    else await this.attachNordic(gatt);
    const label = found?.label ?? 'Nordic UART instrument';
    this.name = this.device?.name ? `${label} ${this.device.name}` : label;
    this.events.status('connected', this.name);
  }

  /** The Cavway X1: write on 0002, packets notified on 0003, each one acknowledged. */
  private async attachX1(gatt: Gatt) {
    const x1 = this.cat.x1;
    const service = await gatt.getPrimaryService(x1.service);
    this.writer = await service.getCharacteristic(x1.write);
    const notify = await service.getCharacteristic(x1.notify);
    this.listen(notify, (b) => this.frame('frame', b));
    await notify.startNotifications();
    this.canTakeShot = true;
    this.setX1Clock();
    this.askX1ForItsList();
  }

  /** Asks the X1 for its feature list (what its `<` long-press picker shows), so a picked feature is named from it. Only a
   * 1.1.7-cc9 or later X1 answers; any other ignores a read of an address it does not know. */
  private askX1ForItsList() {
    for (const f of this.cat.x1.listRequest ?? []) this.write(Uint8Array.from(f));
  }

  /** Sets the X1's clock to this device's local time (the X1 keeps local wall-clock seconds, no time zone), so shot
   * timestamps are right. Every connect; harmless on any X1 firmware (a memory write it has always accepted). */
  private setX1Clock() {
    const c = this.cat.x1.setClock;
    if (!c) return;
    const now = new Date();
    const local = Math.floor(now.getTime() / 1000) - now.getTimezoneOffset() * 60;
    const le = [local & 0xff, (local >>> 8) & 0xff, (local >>> 16) & 0xff, (local >>> 24) & 0xff];
    this.write(Uint8Array.from([...c.prefix, ...le, ...c.suffix]));
  }

  /** The SAP5: 8-byte DistoX packets notified on 0002, nothing to acknowledge. */
  private async attachSap5(gatt: Gatt) {
    const sap5 = this.cat.sap5;
    const service = await gatt.getPrimaryService(sap5.service);
    const notify = await service.getCharacteristic(sap5.notify);
    this.listen(notify, (b) => this.frame('frame', b));
    await notify.startNotifications();
    this.writer = await service.getCharacteristic(sap5.write).catch(() => null);
  }

  /** A Nordic UART instrument whose name this does not know: listen on both characteristics and tell by the packets (8 bytes: SAP5; 33 or more: X1). */
  private async attachNordic(gatt: Gatt) {
    const { x1, sap5 } = this.cat;
    const service = await gatt.getPrimaryService(x1.service);
    let current = 'x1';
    const route = (b: Uint8Array) => {
      const want = b.length === 8 ? 'sap5' : 'x1';
      if (want !== current) {
        this.linked = false;
        this.link(want);
        current = want;
      }
      this.kind = want;
      this.frame('frame', b);
    };
    for (const uuid of [sap5.notify, x1.notify]) {
      const c = await service.getCharacteristic(uuid).catch(() => null);
      if (c && (c.properties.notify || c.properties.indicate)) {
        this.listen(c, route);
        await c.startNotifications();
      }
    }
    this.writer = await service.getCharacteristic(x1.write).catch(() => null);
  }

  /** The SAP6 (CaveBLE): legs notified, each acknowledged with 0x55 plus its sequence bit. */
  private async attachSap6(gatt: Gatt) {
    const sap6 = this.cat.sap6;
    const service = await gatt.getPrimaryService(sap6.service);
    this.writer = await service.getCharacteristic(sap6.command);
    try {
      const nameChar = await service.getCharacteristic(sap6.name);
      this.events.frame(`protocol: ${new TextDecoder().decode(await nameChar.readValue())}`, 'name');
    } catch {
      /* the name is only for the log */
    }
    const leg = await service.getCharacteristic(sap6.leg);
    this.listen(leg, (b) => this.frame('frame', b));
    await leg.startNotifications();
    this.canTakeShot = true;
  }

  /** The BRIC4 and BRIC5: three indications make a shot; commands are ASCII words written with a response. */
  private async attachBric(gatt: Gatt) {
    const bric = this.cat.bric;
    const service = await gatt.getPrimaryService(bric.measureService);
    for (const [uuid, kind] of [[bric.primary, 'primary'], [bric.meta, 'meta'], [bric.errors, 'errors']] as const) {
      const c = await service.getCharacteristic(uuid);
      this.listen(c, (b) => this.frame(kind, b));
      await c.startNotifications(); // these characteristics indicate; the browser enables whichever the instrument offers
    }
    try {
      const control = await gatt.getPrimaryService(bric.controlService);
      this.writer = await control.getCharacteristic(bric.control);
      this.withResponse = true;
      this.canTakeShot = true;
    } catch {
      /* shots still arrive without the control service; it just cannot be told to fire */
    }
  }

  /** Capture: every notifiable characteristic of every service reached, logged as hex. */
  private async attachCapture(gatt: Gatt) {
    for (const service of await gatt.getPrimaryServices()) {
      for (const c of await service.getCharacteristics()) {
        const props = Object.entries(c.properties).filter(([, on]) => on).map(([k]) => k).join(' ');
        this.events.frame(`found ${service.uuid} / ${c.uuid}: ${props}`, 'found');
        if (c.properties.read) {
          try {
            this.events.frame(hex(bytesOf(await c.readValue())), c.uuid);
          } catch {
            /* some characteristics refuse a read: the log says so by saying nothing */
          }
        }
        if (c.properties.notify || c.properties.indicate) {
          this.listen(c, (b) => this.events.frame(hex(b), c.uuid));
          await c.startNotifications();
        }
      }
    }
  }

  /**
   * The link dropped: the instrument went to sleep, out of range, or into a pocket. Keep trying, every few seconds,
   * for as long as the surveyor has not pressed Disconnect: an X1 asleep for ten minutes comes back by itself the
   * moment it is woken, with no new pairing. (It used to give up after about 14 seconds, so a sleeping X1 meant
   * Disconnect and pick it again.)
   */
  private async lost() {
    this.writer = null;
    if (!this.wanted || this.retrying) return;
    this.retrying = true;
    let tries = 0;
    try {
      while (this.wanted) {
        this.events.status('lost', tries < 3 ? 'Reconnecting…' : 'Waiting for the instrument: wake it (press any button) and it reconnects by itself.');
        await new Promise((r) => setTimeout(r, Math.min(WebBleInstrument.retryMs * 2 ** tries, WebBleInstrument.retryMs * 8)));
        if (!this.wanted) return;
        try {
          await this.attach();
          return;
        } catch (e) {
          tries += 1;
          (globalThis as { __cavecadDlog?: (s: string) => void }).__cavecadDlog?.(`reconnect try ${tries} failed: ${e instanceof Error ? e.message : String(e)}`);
        }
      }
    } finally {
      this.retrying = false;
    }
  }
}
