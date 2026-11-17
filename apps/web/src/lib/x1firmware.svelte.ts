// Flashing the Cavway X1 from the app, over Bluetooth. The X1 must be in boot mode (hold both arrow keys, press
// DIST); its bootloader then listens on Bluetooth as well as USB. The protocol (and every safety rule: nothing is
// installed unless every packet was confirmed) is cave_instrument::cavway_flash, run here through the engine.

import { engine } from './engine';
import { x1RawLink } from './survey.svelte';

export const fw = $state({
  /** The chosen file. */
  name: '',
  version: '',
  packets: 0,
  warnings: [] as string[],
  error: '',
  /** null: not asked; else the bootloader version that answered (boot mode), or -1 for no answer. */
  boot: null as number | null,
  checking: false,
  running: false,
  phase: '',
  done: 0,
  total: 0,
  resends: 0,
  result: '' as '' | 'ok' | 'failed',
  message: '',
});

let file: Uint8Array | null = null;
let rx: Uint8Array[] = [];
(globalThis as { __cavecadFwTap?: (b: Uint8Array) => void }).__cavecadFwTap = (b) => {
  if (fw.running || fw.checking) rx.push(b);
};
const pause = (ms: number) => new Promise((r) => setTimeout(r, ms));
const dlog = (m: string) => (globalThis as { __cavecadDlog?: (s: string) => void }).__cavecadDlog?.(`x1fw: ${m}`);

/** Reads a chosen .bin and checks it. */
export async function chooseFile(f: File) {
  fw.error = '';
  fw.result = '';
  fw.message = '';
  const bytes = new Uint8Array(await f.arrayBuffer());
  const img = engine.x1fwInspect(bytes);
  if (img.error) {
    file = null;
    fw.name = '';
    fw.error = `${f.name}: ${img.error}`;
    return;
  }
  file = bytes;
  fw.name = f.name;
  fw.version = img.version ?? '?';
  fw.packets = img.packets ?? 0;
  fw.warnings = img.warnings ?? [];
}

/** Asks the X1 whether it is in boot mode (only the bootloader answers `J`). */
export async function checkBoot(): Promise<boolean> {
  const link = x1RawLink();
  if (!link) {
    fw.boot = -1;
    fw.error = 'Connect the X1 first (Survey ▸ Connect), with it in boot mode.';
    return false;
  }
  fw.checking = true;
  fw.error = '';
  try {
    for (let i = 0; i < 3; i++) {
      rx = [];
      await link.x1Raw(engine.x1fwHelloFrame());
      const t0 = performance.now();
      while (performance.now() - t0 < 1200) {
        const all = concat(rx);
        const v = engine.x1fwHelloAnswer(all);
        if (v >= 0) {
          fw.boot = v;
          return true;
        }
        await pause(60);
      }
    }
    fw.boot = -1;
    return false;
  } finally {
    fw.checking = false;
  }
}

function concat(parts: Uint8Array[]): Uint8Array {
  const n = parts.reduce((a, p) => a + p.length, 0);
  const out = new Uint8Array(n);
  let at = 0;
  for (const p of parts) out.set(p, at), (at += p.length);
  return out;
}

/** Flashes the chosen file. The X1 must be in boot mode (checked first). */
export async function flash() {
  const link = x1RawLink();
  if (!file || !link || fw.running) return;
  if (!(await checkBoot())) {
    fw.error = "The X1 isn't in boot mode: power it off, hold both arrow keys and press DIST, then connect again.";
    return;
  }
  fw.running = true;
  fw.result = '';
  fw.message = '';
  fw.error = '';
  link.quiet = true;
  fw.total = fw.packets;
  fw.done = 0;
  try {
    let step = engine.x1fwBegin(file);
    fw.phase = 'Preparing the X1';
    for (;;) {
      if (step.done) {
        fw.result = 'ok';
        fw.message = 'UPGRADE SUCCESS. Press CLR on the X1, then start it with DIST. CaveCAD sets its clock when it reconnects.';
        break;
      }
      if (step.failed !== undefined) {
        fw.result = 'failed';
        fw.message = `${step.failed}${step.app_untouched ? '' : ''}`;
        break;
      }
      rx = [];
      await link.x1Raw(Uint8Array.from(step.send ?? []));
      const end = performance.now() + (step.wait_ms ?? 2000) + 1500; // Bluetooth adds latency
      let next: typeof step | null = null;
      let fed = 0;
      while (!next && performance.now() < end) {
        if (rx.length > fed) {
          const fresh = concat(rx.slice(fed));
          fed = rx.length;
          next = engine.x1fwBytes(fresh);
        } else await pause(15);
      }
      step = next ?? engine.x1fwTimeout();
      const p = engine.x1fwProgress();
      if (p) {
        fw.done = p.done;
        fw.total = p.total;
        fw.resends = p.resends;
        fw.phase = p.phase === 'sending' ? `Sending firmware: ${p.done} of ${p.total}` : p.phase === 'installing' ? "Installing (UPGRADING...). Don't switch off." : fw.phase;
      }
    }
  } catch (e) {
    fw.result = 'failed';
    fw.message = `${e instanceof Error ? e.message : e}. The X1's firmware is unchanged unless it said UPGRADING; flash again from boot mode.`;
  } finally {
    link.quiet = false;
    fw.running = false;
    dlog(`flash ${fw.result}: ${fw.message} (resends ${fw.resends})`);
  }
}
