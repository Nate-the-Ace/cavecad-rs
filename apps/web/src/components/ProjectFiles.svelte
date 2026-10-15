<script lang="ts">
  // The ribbon's Project files group: bring a survey in from another program (Compass, Walls, Survex,
  // Therion, CSV), and send the cave out -- as a DXF drawing, as survey data for those programs, or as
  // the whole project folder.
  import { FileInput, FileOutput, FileSpreadsheet, Package, ChevronDown } from '@lucide/svelte';
  import { engine, loadEngine, type SurveyFormat } from '../lib/engine';
  import { exportDxf, exportProject, exportSurvey, importSurvey } from '../lib/files';
  import { onMount } from 'svelte';

  let formats = $state<SurveyFormat[]>([]);
  onMount(() => {
    loadEngine().then(() => (formats = engine.surveyFormats()));
  });
  let menu = $state<{ x: number; y: number } | null>(null);

  /** Says what happened in the console (and as an error when it did not work). */
  const tell = (r: { ok: boolean; message: string } | string | null) => {
    if (r === null) return;
    const ok = typeof r === 'string' ? !/^(Nothing|There is no|Saving and exporting)/.test(r) : r.ok;
    engine.log(ok ? 'info' : 'error', 'file', typeof r === 'string' ? r : r.message);
  };
  const run = (job: Promise<{ ok: boolean; message: string } | string | null>) => job.then(tell, (e) => engine.log('error', 'file', String(e)));

  function toggle(e: MouseEvent) {
    if (menu) {
      menu = null;
      return;
    }
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { x: Math.max(4, Math.min(r.left, window.innerWidth - 230)), y: r.bottom + 2 };
  }
  function choose(id: string) {
    menu = null;
    run(exportSurvey(id));
  }
  function outside(e: PointerEvent) {
    if (menu && !(e.target as Element).closest('.formats, .survey-out')) menu = null;
  }
  function keys(e: KeyboardEvent) {
    if (menu && e.key === 'Escape') menu = null;
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={keys} />

<div class="tools">
  <button class="tool" title="Open a survey from Compass, Walls, Survex, Therion or a CSV file as the cave" onclick={() => run(importSurvey())}>
    <FileInput size={22} strokeWidth={1.6} />
    <span>Import</span>
  </button>
  <button class="tool" title="Export the drawing as a DXF, for other CAD programs" onclick={() => run(exportDxf())}>
    <FileOutput size={22} strokeWidth={1.6} />
    <span>DXF</span>
  </button>
  <button class="tool survey-out" class:pressed={!!menu} title="Export the survey for Compass, Walls, Survex, Therion or a spreadsheet" aria-haspopup="menu" aria-expanded={!!menu} onclick={toggle}>
    <FileSpreadsheet size={22} strokeWidth={1.6} />
    <span>Survey <ChevronDown size={11} strokeWidth={2} /></span>
  </button>
  <button class="tool" title="Export the whole project: the cave, its DXF and its scans, as one zip" onclick={() => run(exportProject())}>
    <Package size={22} strokeWidth={1.6} />
    <span>Project</span>
  </button>
</div>

{#if menu}
  <div class="formats" role="menu" style="left:{menu.x}px; top:{menu.y}px">
    {#each formats as f}
      <button role="menuitem" onclick={() => choose(f.id)}>
        <span>{f.label.replace(/ \(.*\)$/, '')}</span>
        <span class="ext">.{f.extensions[0]}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .tools { display: flex; gap: 4px; height: 100%; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .tool.pressed { background: var(--accent-bg); }
  .tool span { display: inline-flex; align-items: center; gap: 2px; white-space: nowrap; }
  .formats { position: fixed; z-index: 60; min-width: 200px; display: flex; flex-direction: column; padding: 4px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 6px 18px rgb(0 0 0 / 0.25); }
  .formats button { display: flex; justify-content: space-between; gap: 16px; padding: 6px 10px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 13px; color: var(--text); text-align: left; }
  .formats button:hover { background: var(--panel-2); }
  .ext { color: var(--text-dim, var(--text)); opacity: 0.7; font-variant-numeric: tabular-nums; }
</style>
