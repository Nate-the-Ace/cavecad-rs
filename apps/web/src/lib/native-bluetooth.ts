// Bluetooth in the desktop app. Its web view has no Web Bluetooth, so this puts a stand-in for `navigator.bluetooth`
// in its place, backed by the app's native Bluetooth (apps/desktop/src-tauri/src/ble.rs). It covers only what
// `instrument/webble.ts` uses (requestDevice with filters, GATT connect, services and characteristics, read, write,
// notifications, the disconnect event), so the instrument code is the same in Chrome and in the app.
//
// The device chooser is CaveCAD's own: it scans for a few seconds and lists what it found, instruments first.

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
const invoke = (): Invoke | undefined => (window as unknown as { __TAURI_INTERNALS__?: { invoke: Invoke } }).__TAURI_INTERNALS__?.invoke;
const call = <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
  const f = invoke();
  if (!f) return Promise.reject(new Error('not in the desktop app'));
  return f(cmd, args) as Promise<T>;
};

// ---- the diagnostic log: Bluetooth events written to a file in the app's own folder (cavecad-debug.log), so a
// problem on a phone can be read afterwards (xcrun devicectl ... copy from the app's data container) ----
let pendingLog: string[] = [];
let logTimer: ReturnType<typeof setTimeout> | undefined;
/** Notes a line in the diagnostic log (kept in the app's folder; nothing leaves the device). */
export function dlog(line: string) {
  if (!invoke()) return;
  pendingLog.push(`${new Date().toISOString()} page ${line}`);
  clearTimeout(logTimer);
  logTimer = setTimeout(() => {
    const lines = pendingLog;
    pendingLog = [];
    void call('debug_log', { lines }).catch(() => undefined);
  }, 500);
}

/** A Bluetooth uuid as the full lowercase 128-bit form (a 16-bit one, as a number or 4 hex digits, is expanded). */
export function fullUuid(u: string | number): string {
  const s = typeof u === 'number' ? u.toString(16) : u.toLowerCase().replace(/^0x/, '');
  if (/^[0-9a-f]{1,4}$/.test(s)) return `0000${s.padStart(4, '0')}-0000-1000-8000-00805f9b34fb`;
  if (/^[0-9a-f]{8}$/.test(s)) return `${s}-0000-1000-8000-00805f9b34fb`;
  return s;
}

/** How long one connect may take before it counts as failed (so a native call that never answers cannot stall the
 * reconnecting: seen on an iPhone, 2026-11-16). Tests shorten it. */
export const limits = { connectMs: 20_000 };

/** `p`, or a failure with `message` once `ms` have passed. */
function within<T>(p: Promise<T>, ms: number, message: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const t = setTimeout(() => reject(new Error(message)), ms);
    p.then((v) => (clearTimeout(t), resolve(v)), (e) => (clearTimeout(t), reject(e)));
  });
}

type Found = { id: string; name: string | null; services: string[]; rssi: number | null };
type Service = { uuid: string; chars: { uuid: string; props: string[] }[] };
type Filter = { namePrefix?: string; name?: string; services?: (string | number)[] };
type Request = { filters?: Filter[]; acceptAllDevices?: boolean; optionalServices?: (string | number)[] };

/** True when a found device fits the request (no filters, or `acceptAllDevices`: anything). */
export function fits(d: Found, req: Request): boolean {
  if (req.acceptAllDevices || !req.filters?.length) return true;
  return req.filters.some((f) => {
    if (f.name && d.name !== f.name) return false;
    if (f.namePrefix && !(d.name ?? '').startsWith(f.namePrefix)) return false;
    if (f.services?.length && !f.services.every((s) => d.services.includes(fullUuid(s)))) return false;
    return !!(f.name || f.namePrefix || f.services?.length);
  });
}

type Listener = (e: Event) => void;
/** Characteristics waiting for notifications, by `device id|characteristic uuid`. */
const notifiable = new Map<string, NativeCharacteristic>();
/** Devices waiting to hear they were disconnected, by id. */
const devices = new Map<string, NativeDevice>();

class NativeCharacteristic {
  value?: DataView;
  private listeners: Listener[] = [];
  readonly properties: { notify: boolean; indicate: boolean; read: boolean; write: boolean; writeWithoutResponse: boolean };
  private readonly deviceId: string;
  private readonly serviceUuid: string;
  readonly uuid: string;
  constructor(deviceId: string, serviceUuid: string, uuid: string, props: string[]) {
    this.deviceId = deviceId;
    this.serviceUuid = serviceUuid;
    this.uuid = uuid;
    this.properties = {
      notify: props.includes('notify'),
      indicate: props.includes('indicate'),
      read: props.includes('read'),
      write: props.includes('write'),
      writeWithoutResponse: props.includes('writeWithoutResponse'),
    };
  }
  private args(extra: Record<string, unknown> = {}) {
    return { id: this.deviceId, service: this.serviceUuid, uuid: this.uuid, ...extra };
  }
  async startNotifications() {
    notifiable.set(`${this.deviceId}|${this.uuid}`, this);
    await call('ble_subscribe', this.args());
    return this;
  }
  async readValue(): Promise<DataView> {
    const bytes = Uint8Array.from(await call<number[]>('ble_read', this.args()));
    this.value = new DataView(bytes.buffer);
    return this.value;
  }
  async writeValue(v: ArrayBufferView) {
    await call('ble_write', this.args({ bytes: Array.from(new Uint8Array(v.buffer, v.byteOffset, v.byteLength)), withResponse: true }));
  }
  async writeValueWithoutResponse(v: ArrayBufferView) {
    await call('ble_write', this.args({ bytes: Array.from(new Uint8Array(v.buffer, v.byteOffset, v.byteLength)), withResponse: !this.properties.writeWithoutResponse }));
  }
  addEventListener(_type: 'characteristicvaluechanged', l: Listener) {
    this.listeners.push(l);
  }
  /** A notification arrived from the native side. */
  arrived(bytes: number[]) {
    this.value = new DataView(Uint8Array.from(bytes).buffer);
    const e = { target: this } as unknown as Event;
    for (const l of this.listeners) l(e);
  }
}

class NativeService {
  readonly uuid: string;
  private readonly chars: NativeCharacteristic[];
  constructor(uuid: string, chars: NativeCharacteristic[]) {
    this.uuid = uuid;
    this.chars = chars;
  }
  async getCharacteristics() {
    return this.chars;
  }
  async getCharacteristic(uuid: string | number) {
    const c = this.chars.find((x) => x.uuid === fullUuid(uuid));
    if (!c) throw new Error(`No characteristic ${fullUuid(uuid)}.`);
    return c;
  }
}

class NativeGatt {
  connected = false;
  private services: NativeService[] = [];
  private readonly device: NativeDevice;
  constructor(device: NativeDevice) {
    this.device = device;
  }
  async connect() {
    dlog(`connect ${this.device.name ?? '?'} id=${this.device.id}`);
    let list: Service[];
    try {
      list = await within(call<Service[]>('ble_connect', { id: this.device.id }), limits.connectMs, 'The instrument did not answer in time.');
    } catch (e) {
      dlog(`connect failed: ${e instanceof Error ? e.message : String(e)}`);
      // An instrument switched off and on again can come back under a new identity (the X1's "sleep" is a power-off):
      // look for it by its name and use whatever identity it has now.
      const now = await this.device.refind();
      if (!now) throw e;
      list = await within(call<Service[]>('ble_connect', { id: this.device.id }), limits.connectMs, 'The instrument did not answer in time.');
    }
    dlog(`connected ${this.device.name ?? '?'} id=${this.device.id}`);
    // a reconnect gets fresh characteristics: the old ones' notifications go to the new ones
    this.services = list.map((s) => new NativeService(s.uuid, s.chars.map((c) => new NativeCharacteristic(this.device.id, s.uuid, c.uuid, c.props))));
    this.connected = true;
    devices.set(this.device.id, this.device);
    return this;
  }
  disconnect() {
    this.connected = false;
    void call('ble_disconnect', { id: this.device.id });
  }
  async getPrimaryServices() {
    return this.services;
  }
  async getPrimaryService(uuid: string | number) {
    const s = this.services.find((x) => x.uuid === fullUuid(uuid));
    if (!s) throw new Error(`The instrument has no service ${fullUuid(uuid)}.`);
    return s;
  }
}

/** Exported for tests. */
export class NativeDevice {
  readonly gatt: NativeGatt;
  private gone: (() => void)[] = [];
  /** Its identity on this phone or computer: it can change when the instrument is switched off and on again. */
  id: string;
  readonly name: string | undefined;
  constructor(id: string, name: string | undefined) {
    this.id = id;
    this.name = name;
    this.gatt = new NativeGatt(this);
  }
  addEventListener(_type: 'gattserverdisconnected', l: () => void) {
    this.gone.push(l);
  }
  /** The native side says the connection dropped. */
  lost() {
    dlog(`gone ${this.name ?? '?'} id=${this.id}`);
    this.gatt.connected = false;
    for (const l of this.gone) l();
  }
  /** Looks for this instrument by its name (3 s scan) and takes its identity now. True when found under a new one. */
  async refind(): Promise<boolean> {
    if (!this.name) return false;
    try {
      const found = (await call<Found[]>('ble_scan', { ms: 3000 })).find((d) => d.name === this.name);
      dlog(`refind ${this.name}: ${found ? `seen id=${found.id}` : 'not seen'}`);
      if (!found || found.id === this.id) return false;
      devices.delete(this.id);
      this.id = found.id;
      devices.set(this.id, this);
      return true;
    } catch (e) {
      dlog(`refind failed: ${e instanceof Error ? e.message : String(e)}`);
      return false;
    }
  }
}

/** Messages from the native side: a notification, or a dropped connection. */
function fromNative(msg: { t: 'notify'; id: string; char: string; bytes: number[] } | { t: 'gone'; id: string }) {
  if (msg.t === 'notify') notifiable.get(`${msg.id}|${msg.char.toLowerCase()}`)?.arrived(msg.bytes);
  else if (msg.t === 'gone') {
    const d = devices.get(msg.id);
    if (d?.gatt.connected) d.lost();
  }
}

const cancelled = () => new Error('User cancelled the requestDevice() chooser.');

/** CaveCAD's device chooser: scans, lists what fits the request (strongest first), and gives back the one picked. */
function choose(req: Request): Promise<Found> {
  return new Promise((resolve, reject) => {
    const back = document.createElement('div');
    back.className = 'ble-chooser';
    back.setAttribute('role', 'dialog');
    back.setAttribute('aria-label', 'Choose a Bluetooth instrument');
    back.style.cssText = 'position:fixed;inset:0;z-index:1000;display:flex;align-items:center;justify-content:center;background:rgba(0,0,0,.45);font:13px system-ui,sans-serif';
    const box = document.createElement('div');
    box.style.cssText = 'width:min(380px,92vw);max-height:70vh;display:flex;flex-direction:column;gap:8px;padding:14px;border-radius:10px;background:var(--panel,#1e2430);color:var(--text,#eee);border:1px solid var(--line,#444)';
    const title = document.createElement('b');
    title.textContent = 'Choose your instrument';
    const status = document.createElement('div');
    status.style.cssText = 'color:var(--muted,#aaa);font-size:12px';
    const list = document.createElement('div');
    list.style.cssText = 'display:flex;flex-direction:column;gap:4px;overflow:auto;min-height:40px';
    const row = document.createElement('div');
    row.style.cssText = 'display:flex;gap:6px;justify-content:flex-end';
    const btn = (label: string) => {
      const b = document.createElement('button');
      b.textContent = label;
      b.style.cssText = 'font:inherit;padding:4px 10px;border-radius:6px;border:1px solid var(--line,#555);background:var(--panel-2,#2a3140);color:inherit;cursor:pointer';
      return b;
    };
    const again = btn('Scan again');
    const cancel = btn('Cancel');
    row.append(again, cancel);
    box.append(title, status, list, row);
    back.append(box);
    document.body.append(back);
    const done = () => back.remove();
    cancel.onclick = () => (done(), reject(cancelled()));
    const scan = async () => {
      again.disabled = true;
      status.textContent = 'Looking for instruments… Make sure yours is on and Bluetooth is turned on in it.';
      list.replaceChildren();
      try {
        const found = (await call<Found[]>('ble_scan', { ms: 4000 })).filter((d) => fits(d, req));
        status.textContent = found.length ? 'Pick yours:' : 'Nothing found. Turn the instrument on (or wake it), then Scan again.';
        for (const d of found) {
          const b = btn(`${d.name ?? 'Unnamed device'}${d.rssi !== null ? `  ·  signal ${d.rssi}` : ''}`);
          b.style.textAlign = 'left';
          b.dataset.bleDevice = d.id;
          b.onclick = () => (done(), resolve(d));
          list.append(b);
        }
      } catch (e) {
        status.textContent = e instanceof Error ? e.message : String(e);
      }
      again.disabled = false;
    };
    again.onclick = () => void scan();
    void scan();
  });
}

/** The stand-in for `navigator.bluetooth`. */
const nativeBluetooth = {
  async requestDevice(req: Request) {
    const d = await choose(req ?? {});
    return new NativeDevice(d.id, d.name ?? undefined);
  },
};

/** In the desktop app, gives the page `navigator.bluetooth` backed by native Bluetooth. Does nothing in a browser. */
export function installNativeBluetooth(): void {
  if (!invoke() || 'bluetooth' in navigator) return;
  (window as unknown as { __cavecadBle?: typeof fromNative }).__cavecadBle = fromNative;
  (globalThis as { __cavecadDlog?: (s: string) => void }).__cavecadDlog = dlog;
  Object.defineProperty(navigator, 'bluetooth', { value: nativeBluetooth, configurable: true });
}
