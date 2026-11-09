<script lang="ts">
  // The ribbon's Import and export group: bring a survey in from another program (Compass, Walls, Survex,
  // Therion, CSV: single files, or a whole project folder), and send the cave out from one menu -- as a
  // DXF drawing, as survey data for those programs, or as the whole project -- with what saved files carry.
  import { FileInput, FileOutput, ChevronDown, Check } from '@lucide/svelte';
  import { option, setOption } from '../lib/cave.svelte';
  import { engine, loadEngine, type SurveyFormat } from '../lib/engine';
  import { exportDxf, exportProject, exportSurvey, importSurvey, importSurveyFolder } from '../lib/files';
  import { onMount } from 'svelte';

  let formats = $state<SurveyFormat[]>([]);
  onMount(() => {
    loadEngine().then(() => (formats = engine.surveyFormats()));
  });
  let menu = $state<{ x: number; y: number; kind: 'import' | 'export' } | null>(null);

  /** Says what happened in the console (and as an error when it did not work). */
  const tell = (r: { ok: boolean; message: string } | string | null) => {
    if (r === null) return;
    const ok = typeof r === 'string' ? !/^(Nothing|There is no|Saving and exporting)/.test(r) : r.ok;
    engine.log(ok ? 'info' : 'error', 'file', typeof r === 'string' ? r : r.message);
  };
  const run = (job: Promise<{ ok: boolean; message: string } | string | null>) => job.then(tell, (e) => engine.log('error', 'file', String(e)));

  function toggle(e: MouseEvent, kind: 'import' | 'export') {
    if (menu?.kind === kind) {
      menu = null;
      return;
    }
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { x: Math.max(4, Math.min(r.left, window.innerWidth - 230)), y: r.bottom + 2, kind };
  }
  function bringIn(folder: boolean) {
    menu = null;
    run(folder ? importSurveyFolder() : importSurvey());
  }
  /** Runs one export and closes the menu. */
  function send(job: () => Promise<string | null>) {
    menu = null;
    run(job());
  }
  function outside(e: PointerEvent) {
    if (menu && !(e.target as Element).closest('.formats, .export, .survey-in')) menu = null;
  }
  function keys(e: KeyboardEvent) {
    if (menu && e.key === 'Escape') menu = null;
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={keys} />
<div class="tools">
  <button class="tool survey-in" class:pressed={menu?.kind === 'import'} title="Open a survey from Compass, Walls, Survex, Therion or a CSV file, a TopoDroid survey ZIP (shots and sketches), or a whole project folder, as the cave" aria-haspopup="menu" aria-expanded={menu?.kind === 'import'} onclick={(e) => toggle(e, 'import')}>
    <FileInput size={22} strokeWidth={1.6} />
    <span>Import <ChevronDown size={11} strokeWidth={2} /></span>
  </button>
  <button class="tool export" class:pressed={menu?.kind === 'export'} title="Send the cave to another program: a DXF drawing, survey data, or the whole project" aria-haspopup="menu" aria-expanded={menu?.kind === 'export'} onclick={(e) => toggle(e, 'export')}>
    <FileOutput size={22} strokeWidth={1.6} />
    <span>Export <ChevronDown size={11} strokeWidth={2} /></span>
  </button>
</div>
{#if menu?.kind === 'import'}
  <div class="formats" role="menu" style="left:{menu.x}px; top:{menu.y}px">
    <button role="menuitem" title="One or more survey files; pick a project file (.mak, .wpj, thconfig) together with the files it names" onclick={() => bringIn(false)}>
      <span>Survey files…</span>
      <span class="ext">.dat .srv .svx .th .csv</span>
    </button>
    <button role="menuitem" title="A whole folder: read through its project file, or every survey file in it" onclick={() => bringIn(true)}>
      <span>Project folder…</span>
      <span class="ext">.mak .wpj thconfig</span>
    </button>
  </div>
{:else if menu}
  <div class="formats" role="menu" style="left:{menu.x}px; top:{menu.y}px">
    <div class="head">Drawing</div>
    <button role="menuitem" title="The drawing as a DXF, for other CAD programs" onclick={() => send(exportDxf)}>
      <span>DXF drawing…</span>
      <span class="ext">.dxf</span>
    </button>
    <div class="head">Survey data</div>
    {#each formats as f}
      <button role="menuitem" onclick={() => send(() => exportSurvey(f.id))}>
        <span>{f.label.replace(/ \(.*\)$/, '')}…</span>
        <span class="ext">.{f.extensions[0]}</span>
      </button>
    {/each}
    <div class="head">Everything</div>
    <button role="menuitem" title="The cave, its DXF and its scans, as one zip" onclick={() => send(exportProject)}>
      <span>Whole project…</span>
      <span class="ext">.zip</span>
    </button>
    <div class="head">Saved cave files include</div>
    <button role="menuitemcheckbox" aria-checked={option('saveScans', true)} title="Put the scan pages linked to trips inside the cave file (bigger, but nothing left behind)" onclick={() => setOption('saveScans', !option('saveScans', true))}>
      <span><span class="tick">{#if option('saveScans', true)}<Check size={13} />{/if}</span>Scans</span>
    </button>
    <button role="menuitemcheckbox" aria-checked={option('saveSurface', true)} title="Put the downloaded aerial photo and ground elevation inside the cave file" onclick={() => setOption('saveSurface', !option('saveSurface', true))}>
      <span><span class="tick">{#if option('saveSurface', true)}<Check size={13} />{/if}</span>Surface data</span>
    </button>
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
  .head { padding: 6px 10px 2px; font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .tick { display: inline-flex; width: 18px; }
</style>
