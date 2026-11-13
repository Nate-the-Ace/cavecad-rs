// The desktop app updating itself (spec docs/superpowers/specs/2026-11-09-desktop-self-update-design.md): on start
// and every 6 hours it asks the CaveCAD server for a newer version, which the app downloads and verifies in the
// background; then the bar offers "Restart now". Nothing installs until the person says so. In a browser, and
// signed out, nothing happens: cavecad.com is always current, and updates are for members.

import { isDesktop, updateInstallNative, updatePrepareNative, type FoundUpdate } from './desktop';
import { desktopBearer } from './desktop-signin';
import { storeOpenCave } from './shelf.svelte';

const BASE = (import.meta.env?.VITE_ACCOUNT_BASE as string | undefined) ?? '';
const CHANNEL_KEY = 'cavecad.updateChannel';

/** Test builds ("dev"): admins may switch this copy to them in Account; the server ignores it for anyone else.
 *  Kept on this device only (a laptop on test builds, the field iPad on regular ones). */
export function updateChannel(): 'stable' | 'dev' {
  try {
    return localStorage.getItem(CHANNEL_KEY) === 'dev' ? 'dev' : 'stable';
  } catch {
    return 'stable';
  }
}

export function setUpdateChannel(c: 'stable' | 'dev'): void {
  try {
    if (c === 'dev') localStorage.setItem(CHANNEL_KEY, 'dev');
    else localStorage.removeItem(CHANNEL_KEY);
  } catch {
    /* private window: stays stable */
  }
  // forget an update found on the other channel (a downloaded test build must not install after switching back)
  update.found = null;
  void checkForUpdate();
}

/** Whether this copy is a test build (set by desktop.yml for the dev channel). */
export const isTestBuild = (import.meta.env?.VITE_UPDATE_CHANNEL as string | undefined) === 'dev';
const EVERY = 6 * 60 * 60 * 1000;

export const update = $state<{ found: FoundUpdate | null; hidden: boolean; installing: boolean; problem: string }>({
  found: null,
  hidden: false,
  installing: false,
  problem: '',
});

let checking = false;

/** One check: quiet when it cannot (offline, signed out, the server down); the next one tries again. */
export async function checkForUpdate(): Promise<void> {
  const token = desktopBearer();
  if (!isDesktop() || !BASE || !token || checking || update.found?.installable) return;
  checking = true;
  try {
    const found = await updatePrepareNative(BASE, token, updateChannel());
    if (found && found.version !== update.found?.version) {
      update.found = found;
      update.hidden = false;
    }
  } catch (e) {
    console.warn('update check:', e);
  } finally {
    checking = false;
  }
}

/** Starts checking: now, then every 6 hours. */
export function startUpdateChecks(): void {
  if (!isDesktop()) return;
  void checkForUpdate();
  setInterval(() => void checkForUpdate(), EVERY);
}

/** "Restart now": keeps the open cave first (as every change already is), then installs and restarts. */
export async function restartIntoUpdate(): Promise<void> {
  update.installing = true;
  update.problem = '';
  try {
    await storeOpenCave();
    await updateInstallNative();
  } catch (e) {
    update.installing = false;
    update.problem = `The update could not be installed: ${e instanceof Error ? e.message : String(e)}`;
  }
}
