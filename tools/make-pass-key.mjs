// Makes the key pair that signs offline passes. Run once per server:
//   node tools/make-pass-key.mjs /path/to/pass-key.json
// Keep the file on the server only (mode 600). Put the printed public value in the web build as
// VITE_PASS_PUBLIC_KEY (GitHub secret PASS_PUBLIC_KEY). A new key signs everyone out of offline use
// until their device is next online.
import { generateKeyPairSync } from 'node:crypto';
import { writeFileSync, existsSync } from 'node:fs';

const file = process.argv[2];
if (!file) throw new Error('usage: node tools/make-pass-key.mjs <path>');
if (existsSync(file)) throw new Error(`${file} exists; delete it first if you mean to replace the key`);
const { privateKey, publicKey } = generateKeyPairSync('ed25519');
writeFileSync(file, JSON.stringify(privateKey.export({ format: 'jwk' })), { mode: 0o600 });
console.log('VITE_PASS_PUBLIC_KEY=' + publicKey.export({ format: 'jwk' }).x);
