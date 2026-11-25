// Which devices get the share sheet for an export (lib/deliver-file.ts). Run: npm run test:deliverfile
import { isIOSDevice } from '../src/lib/deliver-file.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
ok(isIOSDevice('Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15', 'iPhone', 5), 'an iPhone');
ok(isIOSDevice('Mozilla/5.0 (iPad; CPU OS 16_7 like Mac OS X) AppleWebKit/605.1.15', 'iPad', 5), 'an older iPad that says so');
ok(isIOSDevice('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/18.1 Safari/605.1.15', 'MacIntel', 5), 'iPadOS, which calls itself a Mac with touch');
ok(!isIOSDevice('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/131 Safari/537.36', 'MacIntel', 0), 'a Mac downloads');
ok(!isIOSDevice('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131', 'Win32', 0), 'Windows downloads');
ok(!isIOSDevice('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131', 'Win32', 10), 'a Windows touch screen downloads');
ok(!isIOSDevice('Mozilla/5.0 (Linux; Android 14; Pixel 8) Chrome/131 Mobile', 'Linux armv8l', 5), 'Android is not iOS (it keeps the phone path)');
console.log(bad ? 'FAILED' : 'deliverfile ok');
process.exit(bad ? 1 : 0);
