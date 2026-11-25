<script lang="ts">
  // Settings > Cave (phone): the open cave's name (with the unsaved dot), Surface Data (the same dialog the desktop
  // ribbon opens: App mounts it, this only raises it), and the exports: the drawing as DXF and the station checklist
  // as CSV (lib/phone-export.ts: the share sheet where there is one, otherwise a download).
  import { Download, Mountain, Table } from '@lucide/svelte';
  import { cave, unsaved } from '../lib/cave.svelte';
  import { surface } from '../lib/surface.svelte';
  import { exportCaveDxf, exportStationsCsv } from '../lib/phone-export';
  import './settings.css';

  let message = $state('');
  let busy = $state(false);
  async function run(f: () => Promise<string | null>) {
    if (busy) return;
    busy = true;
    try {
      message = (await f()) ?? '';
    } catch (e) {
      message = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
  const dirty = $derived(cave.hasCave && unsaved());
</script>

<div class="set">
  <div class="name" role="status" aria-label="Open cave{dirty ? ', unsaved changes' : ''}">
    <span class="n">{cave.hasCave ? cave.name || 'Untitled cave' : 'No cave open'}</span>
    {#if dirty}<i class="dot" title="Unsaved changes"></i>{/if}
  </div>
  <button class="big" onclick={() => (surface.dialog = true)} disabled={!cave.hasCave}><Mountain size={18} /> Surface Data</button>
  <button class="big" onclick={() => run(exportCaveDxf)} disabled={!cave.hasCave || busy}><Download size={18} /> Export cave (DXF)</button>
  <button class="big" onclick={() => run(exportStationsCsv)} disabled={!cave.hasCave || busy}><Table size={18} /> Export stations (CSV)</button>
  {#if message}<p class="line" role="status">{message}</p>{/if}
</div>

<style>
  .name { display: flex; align-items: center; gap: 8px; min-height: var(--p-hit); flex: none; padding: 0 var(--p-pad); font-size: var(--p-font); font-weight: 600; }
  .name .n { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot { flex: none; width: 8px; height: 8px; border-radius: 50%; background: var(--warn, #f0a020); }
  .set .big { flex: none; min-height: var(--p-hit); justify-content: flex-start; padding: 0 12px; gap: 10px; }
</style>
