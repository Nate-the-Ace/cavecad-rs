// Web Bluetooth transport. Chromium browsers have it, and so does Bluefy on an iPad; Safari does not.
// One Connect serves every supported instrument: the person picks a device in the browser's chooser (offered the
// instruments by the names they advertise), and this works out which it is (`identify.ts`) and speaks its protocol:
// the Cavway X1 and the SAP5 (Nordic UART), the SAP6 (CaveBLE), the BRIC4 and BRIC5 (service 0x58D0). `capture`
// mode instead connects to any device and writes every frame it sends to the log as hex, for an instrument whose
// protocol is not known yet.
//
// Written against the Web Bluetooth API without a real instrument; everything it hands bytes to is tested
// (tests/cavway, instruments, caveble), and the connecting is tested against a stand-in for the API
// (tests/webble.check.ts). The first real connection is the first real test.

import { CAVE_BLE_SERVICE, COMMAND_CHARACTERISTIC, Command, LEG_CHARACTERISTIC, NAME_CHARACTERISTIC, hex } from './caveble.ts';
import { X1Command, X1_NOTIFY, X1_SERVICE, X1_WRITE, command } from './cavway.ts';
import { BATTERY_SERVICE, BRIC_CONTROL, BRIC_CONTROL_SERVICE, BRIC_ERRORS, BRIC_MEASURE_SERVICE, BRIC_META, BRIC_PRIMARY, BricCommand, DEVICE_INFO_SERVICE, commandBytes } from './bric.ts';
import { SAP5_NOTIFY, SAP5_SERVICE, SAP5_WRITE } from './sap5.ts';
import { NAMES, hasNordicUart, identify, type InstrumentKind } from './identify.ts';
import { BricLink, LegLink, Sap5Link, X1Link } from './link.ts';
import type { Instrument, LinkEvents } from './types.ts';

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
export const ALL_SERVICES = [X1_SERVICE, CAVE_BLE_SERVICE, BRIC_MEASURE_SERVICE, BRIC_CONTROL_SERVICE, BATTERY_SERVICE, DEVICE_INFO_SERVICE];

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
  /** Every supported instrument sends single shots: three close shots make a leg (the X1 also flags them). */
  groupsLegs = true;
  /** Which instrument it turned out to be, once known. */
  kind: InstrumentKind | 'nus' | null = null;
  private device: BtDevice | null = null;
  private writer: GattCharacteristic | null = null;
  private withResponse = false;
  private readonly x1: X1Link;
  private readonly sap5: Sap5Link;
  private readonly sap6: LegLink;
  private readonly bric: BricLink;
  private readonly events: LinkEvents;
  private readonly opts: WebBleOptions;
  private wanted = false;
  private queue: Promise<void> = Promise.resolve();

  constructor(events: LinkEvents, opts: WebBleOptions) {
    this.events = events;
    this.opts = opts;
    this.groupsLegs = opts.mode !== 'capture';
    this.x1 = new X1Link(events, (bytes) => this.write(bytes), 'x1');
    this.sap5 = new Sap5Link(events, 'sap5');
    this.sap6 = new LegLink(events, (b) => this.write(Uint8Array.of(b)), 'sap6');
    this.bric = new BricLink(events, 'bric');
  }

  async connect() {
    const bt = api();
    if (!bt) {
      this.events.status('unsupported', 'This browser has no Web Bluetooth. On an iPad, open CaveCAD in the Bluefy browser.');
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
    this.bric.flush();
    this.device?.gatt?.disconnect();
    this.events.status('idle');
  }

  /** Fires the instrument, as pressing its own button does (where that is known to work). */
  takeShot() {
    if (this.kind === 'x1') this.write(command(X1Command.TRIGGER));
    else if (this.kind === 'sap6') this.write(Uint8Array.of(Command.TAKE_SHOT));
    else if (this.kind === 'bric') this.write(commandBytes(BricCommand.SHOT));
  }

  /** What the browser's device chooser is asked to show: the known instruments by their names, or everything. */
  private chooser(): unknown {
    const reach = [...ALL_SERVICES, ...(this.opts.services ?? [])];
    if (this.opts.mode === 'capture' || this.opts.anyDevice) return { acceptAllDevices: true, optionalServices: reach };
    return {
      filters: [...NAMES.map((n) => ({ namePrefix: n.prefix })), { services: [CAVE_BLE_SERVICE] }, { services: [BRIC_MEASURE_SERVICE] }],
      optionalServices: reach,
    };
  }

  /** Queues a write: one at a time, with a gap, and never throwing (a lost ack is sent again by the instrument). */
  private write(data: Uint8Array) {
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

  private async attach() {
    const gatt = await this.device!.gatt!.connect();
    for (const l of [this.x1, this.sap5, this.sap6, this.bric]) l.reset();
    this.writer = null;
    this.withResponse = false;
    this.canTakeShot = false;
    const services = (await gatt.getPrimaryServices()).map((s) => s.uuid);
    if (this.opts.mode === 'capture') {
      await this.attachCapture(gatt);
      this.name = this.device?.name || 'Bluetooth device';
      this.events.status('connected', this.name);
      return;
    }
    const found = identify(this.device?.name, services);
    const kind: InstrumentKind | 'nus' | null = found?.kind ?? (hasNordicUart(services) ? 'nus' : null);
    if (kind === null) throw new Error('This device offers none of the services of a supported instrument. Try "Show every Bluetooth device" under the capture settings, or capture its frames to find out what it sends.');
    if (kind === 'unsupported') throw new Error(`${found?.label ?? 'This instrument'} is recognised but not supported yet.`);
    this.kind = kind;
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
    const service = await gatt.getPrimaryService(X1_SERVICE);
    this.writer = await service.getCharacteristic(X1_WRITE);
    const notify = await service.getCharacteristic(X1_NOTIFY);
    this.listen(notify, (b) => this.x1.onFrame(b));
    await notify.startNotifications();
    this.canTakeShot = true;
  }

  /** The SAP5: 8-byte DistoX packets notified on 0002, nothing to acknowledge. */
  private async attachSap5(gatt: Gatt) {
    const service = await gatt.getPrimaryService(SAP5_SERVICE);
    const notify = await service.getCharacteristic(SAP5_NOTIFY);
    this.listen(notify, (b) => this.sap5.onFrame(b));
    await notify.startNotifications();
    this.writer = await service.getCharacteristic(SAP5_WRITE).catch(() => null);
  }

  /** A Nordic UART instrument whose name this does not know: listen on both characteristics and tell by the packets (8 bytes: SAP5; 33 or more: X1). */
  private async attachNordic(gatt: Gatt) {
    const service = await gatt.getPrimaryService(X1_SERVICE);
    const route = (b: Uint8Array) => {
      if (b.length === 8) this.sap5.onFrame(b, 'sap5');
      else {
        this.kind = 'x1'; // a packet of an X1's length: that is what it is
        this.x1.onFrame(b);
      }
    };
    for (const uuid of [SAP5_NOTIFY, X1_NOTIFY]) {
      const c = await service.getCharacteristic(uuid).catch(() => null);
      if (c && (c.properties.notify || c.properties.indicate)) {
        this.listen(c, route);
        await c.startNotifications();
      }
    }
    this.writer = await service.getCharacteristic(X1_WRITE).catch(() => null);
  }

  /** The SAP6 (CaveBLE): legs notified, each acknowledged with 0x55 plus its sequence bit. */
  private async attachSap6(gatt: Gatt) {
    const service = await gatt.getPrimaryService(CAVE_BLE_SERVICE);
    this.writer = await service.getCharacteristic(COMMAND_CHARACTERISTIC);
    try {
      const nameChar = await service.getCharacteristic(NAME_CHARACTERISTIC);
      this.events.frame(`protocol: ${new TextDecoder().decode(await nameChar.readValue())}`, 'name');
    } catch {
      /* the name is only for the log */
    }
    const leg = await service.getCharacteristic(LEG_CHARACTERISTIC);
    this.listen(leg, (b) => this.sap6.onFrame(b));
    await leg.startNotifications();
    this.canTakeShot = true;
  }

  /** The BRIC4 and BRIC5: three indications make a shot; commands are ASCII words written with a response. */
  private async attachBric(gatt: Gatt) {
    const service = await gatt.getPrimaryService(BRIC_MEASURE_SERVICE);
    for (const [uuid, kind] of [[BRIC_PRIMARY, 'primary'], [BRIC_META, 'meta'], [BRIC_ERRORS, 'errors']] as const) {
      const c = await service.getCharacteristic(uuid);
      this.listen(c, (b) => this.bric.onFrame(kind, b));
      await c.startNotifications(); // these characteristics indicate; the browser enables whichever the instrument offers
    }
    try {
      const control = await gatt.getPrimaryService(BRIC_CONTROL_SERVICE);
      this.writer = await control.getCharacteristic(BRIC_CONTROL);
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

  /** The link dropped (the instrument went out of range, or into a pocket): try again a few times, quietly. */
  private async lost() {
    this.writer = null;
    if (!this.wanted) return;
    this.events.status('lost', 'Reconnecting…');
    for (const wait of [500, 1500, 4000, 8000]) {
      await new Promise((r) => setTimeout(r, wait));
      if (!this.wanted) return;
      try {
        await this.attach();
        return;
      } catch {
        /* try the next wait */
      }
    }
    this.events.status('lost', 'Connection lost. Press Connect to try again.');
  }
}
