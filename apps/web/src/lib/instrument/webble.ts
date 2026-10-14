// Web Bluetooth transport. Chromium browsers have it, and so does Bluefy on an iPad; Safari does not.
// Two modes: the CaveBLE protocol (shots become readings), and "capture", which connects to any
// device the person names services for and writes every frame it sends to the log as hex. Capture is
// how an instrument whose protocol is not known yet gets figured out: the log is the sniffed packets.
//
// Written against the Web Bluetooth API without a browser to run it in; the decoding it hands bytes
// to is tested (tests/caveble.check.ts), the connecting is first exercised on the real instrument.

import { CAVE_BLE_SERVICE, COMMAND_CHARACTERISTIC, Command, LEG_CHARACTERISTIC, NAME_CHARACTERISTIC, hex } from './caveble.ts';
import { LegLink } from './link.ts';
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
  writeValueWithoutResponse?(v: BufferSource): Promise<void>;
  writeValue(v: BufferSource): Promise<void>;
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

export interface WebBleOptions {
  /** `cave`: the CaveBLE protocol. `capture`: any device, log every frame. */
  mode: 'cave' | 'capture';
  /** In capture mode: service uuids to be allowed to reach (the browser cannot list what it was not told). */
  services?: string[];
}

export class WebBleInstrument implements Instrument {
  name = 'Bluetooth instrument';
  readonly canTakeShot: boolean;
  private device: BtDevice | null = null;
  private command: GattCharacteristic | null = null;
  private readonly link: LegLink;
  private wanted = false;

  private readonly events: LinkEvents;
  private readonly opts: WebBleOptions;

  constructor(events: LinkEvents, opts: WebBleOptions) {
    this.events = events;
    this.opts = opts;
    this.canTakeShot = opts.mode === 'cave';
    this.link = new LegLink(events, (b) => void this.write(b), 'bluetooth');
  }

  async connect() {
    const bt = api();
    if (!bt) {
      this.events.status('unsupported', 'This browser has no Web Bluetooth. On an iPad, open CaveCAD in the Bluefy browser.');
      return;
    }
    this.events.status('connecting');
    try {
      const services = [CAVE_BLE_SERVICE, ...(this.opts.services ?? [])];
      this.device = await bt.requestDevice(
        this.opts.mode === 'cave' ? { filters: [{ services: [CAVE_BLE_SERVICE] }] } : { acceptAllDevices: true, optionalServices: services },
      );
      this.name = this.device.name || this.name;
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
    void this.write(Command.TAKE_SHOT);
  }

  private async write(byte: number) {
    const c = this.command;
    if (!c) return;
    const data = Uint8Array.of(byte);
    try {
      if (c.writeValueWithoutResponse) await c.writeValueWithoutResponse(data);
      else await c.writeValue(data);
    } catch {
      /* a lost ack is repeated by the instrument: nothing to do */
    }
  }

  private async attach() {
    const gatt = await this.device!.gatt!.connect();
    this.link.reset();
    if (this.opts.mode === 'cave') {
      const service = await gatt.getPrimaryService(CAVE_BLE_SERVICE);
      this.command = await service.getCharacteristic(COMMAND_CHARACTERISTIC);
      try {
        const nameChar = await service.getCharacteristic(NAME_CHARACTERISTIC);
        this.events.frame(`protocol: ${new TextDecoder().decode(await nameChar.readValue())}`, 'name');
      } catch {
        /* the name is only for the log */
      }
      const leg = await service.getCharacteristic(LEG_CHARACTERISTIC);
      leg.addEventListener('characteristicvaluechanged', (e) => {
        const v = valueOf(e);
        if (v) this.link.onFrame(bytesOf(v));
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
    this.command = null;
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
