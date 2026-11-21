// CaveCAD on iPhone and iPad shares its caves' stations with CaveCAD Capture on the same device, offline: each
// cave with a server room gets one keychain item (src-tauri/src/capture.rs, tauri-plugin-capture-link) holding the
// shared-stations JSON (crates/cave-capture/src/shared.rs). The open cave is republished 2 s after an edit, at once
// when opened; the other shelf caves once at start. Rust skips a write when the JSON is unchanged.
// Anywhere else (browser, desktop) none of this starts.

import { untrack } from 'svelte';
import { cave } from './cave.svelte';
import { engine } from './engine';
import { dxfOf, isNative, shelf } from './shelf.svelte';
import { captureLinkAvailable, captureShareStations } from './desktop';

/** The open cave's shared-stations text as last published (`'null'` for none): capture-shots re-places on change. */
export const captureShare = $state({ available: false, text: 'null' });

let timer: ReturnType<typeof setTimeout> | undefined;
let lastOpened = -1;
let lastChangeAt = 0;

async function publish(roomId: string, text: string, name: string) {
  try {
    const bytes = await captureShareStations(roomId, text);
    if (bytes !== null) {
      const n = (JSON.parse(text) as { stations: unknown[] }).stations.length;
      engine.log('info', 'capture', `stations written for Capture: ${name || roomId}, ${n} stations, ${bytes} bytes`);
    }
  } catch (e) {
    engine.log('warn', 'capture', `stations not written for Capture (${name || roomId}): ${String(e)}`);
  }
}

async function publishOpen() {
  const id = engine.roomId();
  if (!cave.hasCave || !id) {
    captureShare.text = 'null';
    return;
  }
  const text = engine.captureStationsText(cave.name, lastChangeAt || Date.now());
  captureShare.text = text;
  if (text !== 'null') await publish(id, text, cave.name);
}

function schedule() {
  clearTimeout(timer);
  const opened = cave.opened !== lastOpened;
  if (opened) {
    lastOpened = cave.opened;
    // a cave just opened changed when it was last kept, not now
    lastChangeAt = shelf.records.find((r) => r.id === shelf.currentId)?.updated ?? Date.now();
  } else {
    lastChangeAt = Date.now();
  }
  timer = setTimeout(() => void publishOpen(), opened ? 0 : 2000);
}

/** The shelf's other caves with a room, one at a time, idle between. */
async function publishShelf() {
  for (const r of shelf.records.slice()) {
    if (!r.roomId || r.id === shelf.currentId) continue;
    await new Promise((res) => setTimeout(res, 50));
    const text = await dxfOf(r.id);
    if (!text || !isNative(text)) continue;
    const json = engine.captureStationsOfNativeText(text, r.name, r.updated);
    if (json !== 'null') await publish(r.roomId, json, r.name);
  }
}

/** Starts sharing (CaveCAD on iPhone and iPad only). Call once the engine and the shelf are loaded. */
export async function startCaptureShare(): Promise<void> {
  if (!(await captureLinkAvailable())) return;
  captureShare.available = true;
  engine.log('info', 'capture', 'Capture link: sharing stations with CaveCAD Capture on this device');
  $effect.root(() => {
    $effect(() => {
      void cave.version;
      void cave.opened;
      void cave.hasCave;
      untrack(schedule);
    });
  });
  void publishShelf();
}
