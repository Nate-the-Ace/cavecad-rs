// Run: npm run test:nativeble
import assert from 'node:assert/strict';
import { fits, fullUuid, NativeDevice } from '../src/lib/native-bluetooth.ts';

assert.equal(fullUuid(0x58d0), '000058d0-0000-1000-8000-00805f9b34fb');
assert.equal(fullUuid('180F'), '0000180f-0000-1000-8000-00805f9b34fb');
assert.equal(fullUuid('6E400001-B5A3-F393-E0A9-E50E24DCCA9E'), '6e400001-b5a3-f393-e0a9-e50e24dcca9e');

const x1 = { id: 'a', name: 'CavwayX1-1234', services: [], rssi: -60 };
const bric = { id: 'b', name: 'BRIC5', services: [fullUuid(0x58d0)], rssi: -70 };
const other = { id: 'c', name: 'Headphones', services: [], rssi: -40 };
const req = { filters: [{ namePrefix: 'Cavway' }, { services: [0x58d0] }] };
assert.equal(fits(x1, req), true, 'name prefix');
assert.equal(fits(bric, req), true, 'advertised service');
assert.equal(fits(other, req), false, 'anything else is left out');
assert.equal(fits(other, { acceptAllDevices: true }), true, 'capture mode shows everything');
assert.equal(fits({ ...x1, name: null }, req), false, 'no name, no service: not offered');
// ---- an X1 switched off and on again comes back under a new identity: found again by its name ----
{
  const calls: string[] = [];
  const g = globalThis as unknown as { window: unknown; __TAURI_INTERNALS__: { invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown> } };
  g.window = globalThis;
  g.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      calls.push(`${cmd}${args?.id ? ` ${args.id}` : ''}`);
      if (cmd === 'ble_connect' && args?.id === 'old') throw 'The instrument did not answer. Is it on and close by?';
      if (cmd === 'ble_connect') return [{ uuid: '6e400001-b5a3-f393-e0a9-e50e24dcca9e', chars: [] }];
      if (cmd === 'ble_scan') return [{ id: 'other', name: 'Headphones', services: [], rssi: -40 }, { id: 'new', name: 'CavwayX1-0312', services: [], rssi: -60 }];
      return null;
    },
  };
  const d = new NativeDevice('old', 'CavwayX1-0312');
  await d.gatt.connect();
  assert.equal(d.id, 'new', 'it takes the identity the instrument has now');
  assert.ok(d.gatt.connected, 'and is connected');
  assert.deepEqual(calls.filter((c) => !c.startsWith('debug_log')), ['ble_connect old', 'ble_scan', 'ble_connect new']);
  // the same identity seen again (still asleep, say): the failure stands, nothing is renamed
  const e = new NativeDevice('old', 'CavwayX1-0312');
  g.__TAURI_INTERNALS__.invoke = async (cmd) => {
    if (cmd === 'ble_connect') throw 'no answer';
    if (cmd === 'ble_scan') return [{ id: 'old', name: 'CavwayX1-0312', services: [], rssi: -60 }];
    return null;
  };
  await assert.rejects(() => e.gatt.connect());
  assert.equal(e.id, 'old');
}
console.log('nativeble: ok');
