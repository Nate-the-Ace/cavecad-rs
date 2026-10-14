<script lang="ts">
  // The Sheets tab's groups: paper and scale, the title block's words, which pieces to show, and saving.
  import { onMount } from 'svelte';
  import { Download, Printer, FileImage } from '@lucide/svelte';
  import { cave } from '../lib/cave.svelte';
  import { engine, loadEngine, type SheetOptions } from '../lib/engine';
  import { orderedScales, sheet, setSheet } from '../lib/sheet.svelte';
  import { downloadSheet, printSheet } from '../lib/sheet-export';
  import { showPanel } from '../lib/workspace.svelte';

  let { part }: { part: 'paper' | 'title' | 'pieces' | 'save' } = $props();
  let opts = $state<SheetOptions | null>(null);
  onMount(() => {
    loadEngine().then(() => {
      opts = engine.sheetOptions();
      showPanel('sheet');
    });
  });
  // metric scales first when the cave is shown in metric
  const scales = $derived(opts ? orderedScales(opts.scales, cave.units) : []);
  const tell = (m: string) => engine.log('info', 'sheet', m);
</script>

{#if part === 'paper'}
  <div class="col">
    <label class="row"><span class="name">Paper</span>
      <select value={sheet.paper} onchange={(e) => setSheet({ paper: Number(e.currentTarget.value) })} aria-label="Paper">
        {#each opts?.papers ?? [] as p, i}<option value={i}>{p.name}</option>{/each}
      </select>
    </label>
    <div class="row">
      <select value={sheet.turned} onchange={(e) => setSheet({ turned: Number(e.currentTarget.value) as 0 | 1 | 2 })} aria-label="Way up">
        <option value={2}>Best way up</option><option value={0}>Landscape</option><option value={1}>Portrait</option>
      </select>
      <select value={sheet.feetPerInch} onchange={(e) => setSheet({ feetPerInch: Number(e.currentTarget.value) })} aria-label="Scale">
        <option value={0}>Scale: best fit</option>
        {#each scales as s}<option value={s.feet_per_inch}>{s.label}</option>{/each}
      </select>
    </div>
  </div>
{:else if part === 'title'}
  <div class="col wide">
    <label class="row"><span class="name">Name</span><input type="text" value={sheet.name} placeholder="The cave's name" maxlength="80" oninput={(e) => setSheet({ name: e.currentTarget.value })} aria-label="Cave name" /></label>
    <label class="row"><span class="name">Notes</span><input type="text" value={sheet.lines.join('; ')} placeholder="Location; surveyed by; date" maxlength="300" oninput={(e) => setSheet({ lines: e.currentTarget.value.split(';').map((x) => x.trim()) })} aria-label="Title block notes" /></label>
  </div>
{:else if part === 'pieces'}
  <div class="col">
    <label class="row"><input type="checkbox" checked={sheet.title} onchange={(e) => setSheet({ title: e.currentTarget.checked })} /> Title block</label>
    <div class="row">
      <label class="row"><input type="checkbox" checked={sheet.bar} onchange={(e) => setSheet({ bar: e.currentTarget.checked })} /> Scale bar</label>
      <label class="row"><input type="checkbox" checked={sheet.north} onchange={(e) => setSheet({ north: e.currentTarget.checked })} /> North</label>
    </div>
  </div>
{:else}
  <div class="btns">
    <button class="tool" onclick={() => tell(downloadSheet())}><FileImage size={22} strokeWidth={1.6} /><span>PNG</span></button>
    <button class="tool" onclick={() => tell(printSheet())}><Printer size={22} strokeWidth={1.6} /><span>Print</span></button>
  </div>
{/if}

<style>
  .col { display: flex; flex-direction: column; gap: 4px; justify-content: center; height: 100%; min-width: 230px; font-size: 12px; overflow: hidden; }
  .col.wide { min-width: 300px; }
  .row { display: flex; align-items: center; gap: 6px; height: 22px; color: var(--text); white-space: nowrap; }
  .name { min-width: 44px; }
  select, input[type='text'] { flex: 1; min-width: 0; height: 22px; box-sizing: border-box; padding: 0 6px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); font: inherit; }
  .btns { display: flex; gap: 4px; align-items: center; height: 100%; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
</style>
