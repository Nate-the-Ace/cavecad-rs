<script lang="ts">
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  // slides up from the bottom edge (and back down); no motion for people who ask for less
  const slide = { y: '100%', duration: typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 240, easing: cubicOut };
  // The settings of the picked or selected object (a drawn line, symbol or text; a station, leg, splay) as a strip over
  // the bottom of the Sketch canvas: the same parts the ribbon's contextual tab shows on the desktop. It shows when
  // something is picked; Done puts it away (the object stays picked), and a long press on the object brings it back.
  import SelectionTools from './SelectionTools.svelte';
  import { cave, selectedIds, setPick } from '../lib/cave.svelte';
  import { engine } from '../lib/engine';
  import { INSPECTORS } from '../lib/inspectors';
  import { isStampKind } from '../lib/symbols';
  import { isFillKind } from '../lib/fills';
  import { phone } from '../lib/phone.svelte';
  import type { PickKind } from '../lib/cave.svelte';

  const ids = $derived.by(() => {
    void cave.also;
    return selectedIds();
  });
  const infos = $derived.by(() => {
    void cave.version;
    return ids.map((id) => engine.lineInfo(id)).filter((i): i is NonNullable<ReturnType<typeof engine.lineInfo>> => !!i);
  });
  const pickKind = $derived(cave.pick.kind as Exclude<PickKind, ''> | '');
  const mode = $derived(cave.tool ? '' : infos.length ? 'selection' : pickKind ? 'pick' : '');
  const alike = $derived(infos.every((i) => i.kind === infos[0]?.kind));
  const kind = $derived(infos[0]?.kind ?? '');
  const title = $derived(mode === 'pick' && pickKind ? INSPECTORS[pickKind].title : infos.length > 1 && !alike ? 'Several objects' : `Settings${infos.length > 1 ? ` (${infos.length})` : ''}`);

  // what shows, as [heading, part] pairs: a selection's parts follow the ribbon's tab
  const parts = $derived.by<{ name: string; part: string }[]>(() => {
    if (mode !== 'selection') return [];
    const stamps = infos.every((i) => isStampKind(i.kind));
    const words = infos.some((i) => i.kind === 'text' || i.kind === 'callout' || !!i.text);
    const out = [{ name: stamps ? 'Annotation' : 'Line', part: 'type' }];
    if (stamps && words) out.push({ name: 'Text format', part: 'format' });
    if (alike && kind === 'callout') out.push({ name: 'Leader', part: 'leader' });
    if (alike && !isStampKind(kind) && kind !== 'fill-custom') out.push({ name: isFillKind(kind) ? 'Area' : 'Line style', part: 'style' });
    return out;
  });

  // a new thing picked shows its settings
  const key = $derived(mode === 'selection' ? `l:${ids.join(',')}` : mode === 'pick' ? `p:${cave.pick.kind}:${cave.pick.id}` : '');
  let last = '';
  $effect(() => {
    if (key && key !== last) phone.objectSettings = true;
    last = key;
  });

  const done = () => (phone.objectSettings = false);
  const remove = () => {
    const doomed = selectedIds();
    cave.selected = '';
    for (const id of doomed) engine.deleteLine(id);
    cave.version += 1;
  };
  const Inspector = $derived(pickKind ? INSPECTORS[pickKind].component : null);
</script>

{#if mode && phone.objectSettings}
  <div class="strip" role="group" aria-label="Object settings" transition:fly={slide}>
    <header>
      <strong>{title}</strong>
      {#if Inspector && pickKind}<span class="hinfo"><Inspector part="info" kind={pickKind} /></span>{/if}
      <span class="acts">
        {#if mode === 'selection'}<button class="del" onclick={remove}>Delete</button>{/if}
        <button class="done" onclick={done}>Done</button>
      </span>
    </header>
    <div class="body" class:pick={mode === 'pick'}>
      {#if mode === 'selection'}
        {#each parts as g (g.part)}<section><h4>{g.name}</h4><SelectionTools part={g.part as 'type'} /></section>{/each}
      {:else if Inspector && pickKind}
        {#each INSPECTORS[pickKind].groups.filter((g) => g.part !== 'info') as g (g.part)}<section><h4>{g.name}</h4><Inspector part={g.part} kind={pickKind} /></section>{/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .strip { position: absolute; left: 0; right: 0; bottom: 0; max-height: 75%; display: flex; flex-direction: column; background: var(--panel); border-top: 1px solid var(--line); z-index: 20; touch-action: pan-y; }
  header { display: flex; align-items: center; justify-content: space-between; min-height: var(--p-row); padding: 0 var(--p-gap) 0 var(--p-pad); flex: none; font-size: var(--p-font); }
  /* the picked thing's name, elevation and legs sit on the title's line */
  .hinfo { flex: 1 1 0; min-width: 0; margin: 0 var(--p-gap); overflow: hidden; }
  .hinfo :global(.col.info) { flex-direction: row; flex-wrap: nowrap; align-items: baseline; gap: 0 8px; height: auto; min-width: 0; white-space: nowrap; font-size: var(--p-font-small); }
  .hinfo :global(.col.info .row) { height: auto; gap: 6px; min-width: 0; }
  .hinfo :global(.col.info .row:last-child) { overflow: hidden; text-overflow: ellipsis; }
  .hinfo :global(.label) { font-size: var(--p-font); }
  .acts { display: flex; gap: var(--p-gap); }
  button { min-height: var(--p-row); min-width: 64px; border: 0; border-radius: var(--p-radius); background: var(--panel-2); color: var(--text); font: inherit; touch-action: manipulation; }
  .done { background: var(--accent); color: #fff; font-weight: 600; }
  /* everything is meant to fit with no scrolling: the groups flow side by side and wrap, and the ribbon-sized buttons shrink to a touch row */
  .body { display: flex; flex-wrap: wrap; align-items: flex-start; gap: 0 var(--p-pad); padding: 0 var(--p-gap) var(--p-gap); font-size: var(--p-font-small); }
  section { padding: 2px 0 4px; min-width: 0; }
  .body :global(.col), .body :global(.notes), .body :global(.note) { min-width: 0; height: auto; }
  .body :global(.walls), .body :global(.grid) { height: auto; }
  .body :global(.btns) { height: auto; flex-wrap: wrap; }
  /* a picked station or shot: no group headings, and each group is one or two lines across the whole strip */
  .body section { flex: 1 1 100%; padding: 1px 0; }
  .body h4 { display: none; }
  .body.pick :global(.col.info) { flex-direction: row; flex-wrap: wrap; align-items: baseline; gap: 0 10px; }
  .body.pick :global(.col .row) { height: auto; }
  .body.pick :global(.walls) { grid-template-columns: 1fr 1fr; grid-template-rows: auto auto; gap: 2px 10px; width: 100%; }
  .body.pick :global(.notes) { flex-direction: row; align-items: center; gap: 6px; }
  .body.pick :global(.notes select) { flex: 0 1 38%; }
  .body.pick :global(.notes input) { flex: 1 1 0; }
  .body.pick :global(.note) { width: 100%; }
  .body.pick :global(.btns) { flex-wrap: nowrap; width: 100%; gap: var(--p-gap); }
  .body.pick :global(.btns .tool) { flex: 1 1 0; justify-content: center; padding: 0 4px; }
  .body.pick :global(.btns .tool span) { font-size: var(--p-font-small); }
  /* the same for lines, symbols, text, the halo, viewframes and shots: every group runs across the strip and wraps, with no ribbon-sized rows */
  .body :global(.sel), .body :global(.types), .body :global(.curve), .body :global(.halo), .body :global(.sliders), .body :global(.row) { flex-wrap: wrap; height: auto; gap: 4px 8px; width: 100%; }
  .body :global(.style) { display: flex; flex-wrap: wrap; align-items: center; height: auto; gap: 4px 8px; width: 100%; }
  .body :global(.detail), .body :global(.style .detail) { flex-direction: row; align-items: center; height: auto; gap: 6px; }
  .body :global(.detail input) { width: 5.5em; }
  .body :global(.words) { flex: 1 1 140px; width: auto; min-width: 0; }
  .body :global(.kind) { max-width: 100%; }
  .body :global(.tip), .body :global(.many), .body :global(.msg) { max-width: none; white-space: normal; }
  .body :global(.grid) { grid-template-columns: 1fr 1fr; grid-template-rows: none; width: 100%; gap: 4px 10px; }
  .body :global(.grid > span:empty) { display: none; }
  .body :global(.sliders label), .body :global(.col label) { width: 100%; justify-content: space-between; }
  .body :global(.sliders input[type='range']) { flex: 1 1 0; width: auto; min-width: 0; }
  .body :global(.val) { min-width: 0; }
  .body :global(.tool) { height: var(--p-hit); min-height: 0; min-width: 0; flex-direction: row; gap: 6px; padding: 0 10px; }
  h4 { margin: 0 0 2px; font-size: var(--p-font-small); font-weight: 600; color: var(--muted); }
</style>
