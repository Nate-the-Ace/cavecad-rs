// Capture mode for the Handbook's screenshots (tools/handbook-shots.mjs): /?handbook-shot=<panel> opens the
// Pitfall sample with only that panel showing, maximised, and says when it has settled. Development builds only.
import { account } from './account.svelte';
import { openSample } from './caves';
import { shelf } from './shelf.svelte';
import { PANELS, panelState, showPanel, togglePanel } from './workspace.svelte';

/** The panel to photograph, when the page was opened in capture mode (never in a production build). */
export function shotPanel(): string | null {
  if (!import.meta.env.DEV) return null;
  return new URLSearchParams(location.search).get('handbook-shot');
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Call once the engine and the workspace are ready. */
export async function runShot(panel: string): Promise<void> {
  if (!import.meta.env.DEV || !PANELS[panel]) return;
  window.confirm = () => true;
  setInterval(() => {
    account.phase = 'ready';
    shelf.open = false;
  }, 200);
  account.phase = 'ready';
  shelf.open = false;
  await openSample('pitfall');
  shelf.open = false;
  for (const id of Object.keys(panelState.open)) if (panelState.open[id] && id !== panel) togglePanel(id);
  showPanel(panel);
  (window as unknown as { __dock?: { getPanel(id: string): { api: { maximize(): void } } | undefined } }).__dock?.getPanel(panel)?.api.maximize();
  await wait(2000);
  document.body.dataset.shotReady = '1';
}
