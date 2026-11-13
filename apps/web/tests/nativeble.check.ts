// Run: npm run test:nativeble
import assert from 'node:assert/strict';
import { fits, fullUuid } from '../src/lib/native-bluetooth.ts';

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
console.log('nativeble: ok');
