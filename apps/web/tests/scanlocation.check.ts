import { cleanRoot, copyPathHint, fullPath, osOf, revealHint, revealLabel } from '../src/lib/scan-location.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

ok(fullPath('/Users/n/Cave/scans', '2024 Scans/4-6-24/p 1.jpg') === '/Users/n/Cave/scans/2024 Scans/4-6-24/p 1.jpg', 'a Mac path joins with /');
ok(fullPath('C:\\Cave\\scans', '2024 Scans/p1.jpg') === 'C:\\Cave\\scans\\2024 Scans\\p1.jpg', 'a Windows path joins with \\');
ok(fullPath('/a/b', '') === '/a/b', 'the folder itself');
ok(cleanRoot('  "/Users/n/Cave/scans/"  ') === '/Users/n/Cave/scans', 'quotes, spaces and a trailing slash go');
ok(cleanRoot('C:\\Cave\\scans\\') === 'C:\\Cave\\scans' && cleanRoot('scans') === '' && cleanRoot('') === '', 'a bare name is not a path');
ok(osOf('MacIntel', 'Mozilla/5.0 (Macintosh)') === 'mac' && osOf('Win32', 'Windows NT 10') === 'windows' && osOf('Linux x86_64', 'X11') === 'other', 'which computer');
ok(revealLabel('mac') === 'Reveal in Finder' && revealLabel('windows') === 'Show in Explorer', 'the words follow the computer');
ok(revealHint('mac').includes('Shift+Cmd+G') && revealHint('windows').includes('address bar') && copyPathHint('mac').includes('Option'), 'the hints say what to do');

if (bad) process.exit(1);
console.log('scan location ok');
