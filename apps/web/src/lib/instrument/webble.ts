// Web Bluetooth transport. Chromium browsers have it, and so does Bluefy on an iPad; Safari does not.
// Three modes: `x1` (the Cavway X1: Nordic UART service, framed writes, packets acknowledged), `caveble` (the
// open protocol of the Shetland Attack Pony and similar), and `capture`, which connects to any device the
// person names services for and writes every frame it sends to the log as hex: how an instrument whose
// protocol is unknown gets worked out.
//
// Written against the Web Bluetooth API without a browser to run it in; everything it hands bytes to is
// tested (tests/cavway.check.ts, caveble.check.ts), the connecting is first exercised on a real instrument.

import { CAVE_BLE_SERVICE, COMMAND_CHARACTERISTIC, Command, LEG_CHARACTERISTIC, NAME_CHARACTERISTIC, hex } from './caveble.ts';
import { X1Command, X1_NOTIFY, X1_SERVICE, X1_WRITE, command } from './cavway.ts';
import { LegLink, X1Link } from './link.ts';
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

/** The X1's microcontroller needs a gap between frames (TopoDroid waits 100 ms): writes go one at a time, this far apart. */
const WRITE_GAP_MS = 100;

export interface WebBleOptions {
  /** `x1`: the Cavway X1. `caveble`: the open protocol. `capture`: any device, log every frame. */
  mode: 'x1' | 'caveble' | 'capture';
  /** In capture mode: service uuids to be allowed to reach (the browser cannot list what it was not told). */
  services?: string[];
  /** Offer every Bluetooth device, not only those advertising the instrument's service (the X1's advertised name and service list are not known yet). */
  anyDevice?: boolean;
}

export class WebBleInstrument implements Instrument {
  name = 'Bluetooth instrument';
  readonly canTakeShot: boolean;
  readonly groupsLegs: boolean;
  private device: BtDevice | null = null;
  private writer: GattCharacteristic | null = null;
  private readonly legLink: LegLink;
  private readonly x1Link: X1Link;
  private readonly events: LinkEvents;
  private readonly opts: WebBleOptions;
  private wanted = false;
  private queue: Promise<void> = Promise.resolve();

  constructor(events: LinkEvents, opts: WebBleOptions) {
    this.events = events;
    this.opts = opts;
    this.canTakeShot = opts.mode !== 'capture';
    this.groupsLegs = opts.mode === 'x1';
    this.legLink = new LegLink(events, (b) => this.write(Uint8Array.of(b)), 'bluetooth');
    this.x1Link = new X1Link(events, (bytes) => this.write(bytes), 'x1');
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
      this.name = this.device.name || (this.opts.mode === 'x1' ? 'Cavway X1' : this.name);
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
    this.device?.gatt?.disconnect();
    this.events.status('idle');
  }

  takeShot() {
    if (this.opts.mode === 'x1') this.write(command(X1Command.TRIGGER));
    else if (this.opts.mode === 'caveble') this.write(Uint8Array.of(Command.TAKE_SHOT));
  }

  /** What the browser's device chooser is asked to show. */
  private chooser(): unknown {
    const service = this.opts.mode === 'x1' ? X1_SERVICE : CAVE_BLE_SERVICE;
    const reach = [service, ...(this.opts.services ?? [])];
    if (this.opts.mode === 'capture' || this.opts.anyDevice) return { acceptAllDevices: true, optionalServices: reach };
    return { filters: [{ services: [service] }] };
  }

  /** Queues a write: one at a time, with a gap, and never throwing (a lost ack is sent again by the instrument). */
  private write(data: Uint8Array) {
    this.queue = this.queue.then(async () => {
      const c = this.writer;
      if (!c) return;
      try {
        if (c.writeValueWithoutResponse) await c.writeValueWithoutResponse(data);
        else await c.writeValue(data);
      } catch {
        /* nothing to do: see above */
      }
      await new Promise((r) => setTimeout(r, WRITE_GAP_MS));
    });
  }

  private async attach() {
    const gatt = await this.device!.gatt!.connect();
    this.legLink.reset();
    this.x1Link.reset();
    if (this.opts.mode === 'x1') {
      const service = await gatt.getPrimaryService(X1_SERVICE);
      this.writer = await service.getCharacteristic(X1_WRITE);
      const notify = await service.getCharacteristic(X1_NOTIFY);
      notify.addEventListener('characteristicvaluechanged', (e) => {
        const v = valueOf(e);
        if (v) this.x1Link.onFrame(bytesOf(v));
      });
      await notify.startNotifications();
    } else if (this.opts.mode === 'caveble') {
      const service = await gatt.getPrimaryService(CAVE_BLE_SERVICE);
      this.writer = await service.getCharacteristic(COMMAND_CHARACTERISTIC);
      try {
        const nameChar = await service.getCharacteristic(NAME_CHARACTERISTIC);
        this.events.frame(`protocol: ${new TextDecoder().decode(await nameChar.readValue())}`, 'name');
      } catch {
        /* the name is only for the log */
      }
      const leg = await service.getCharacteristic(LEG_CHARACTERISTIC);
      leg.addEventListener('characteristicvaluechanged', (e) => {
        const v = valueOf(e);
        if (v) this.legLink.onFrame(bytesOf(v));
      });
      await leg.startNotifications();
    } else {
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
            c.addEventListener('characteristicvaluechanged', (e) => {
              const v = valueOf(e);
              if (v) this.events.frame(hex(bytesOf(v)), c.uuid);
            });
            await c.startNotifications();
          }
        }
      }
    }
    this.events.status('connected', this.name);
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
