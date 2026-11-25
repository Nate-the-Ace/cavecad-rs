<script lang="ts">
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
  <div class="strip" role="group" aria-label="Object settings">
    <header>
      <strong>{title}</strong>
      <span class="acts">
        {#if mode === 'selection'}<button class="del" onclick={remove}>Delete</button>{/if}
        <button class="done" onclick={done}>Done</button>
      </span>
    </header>
    <div class="body">
      {#if mode === 'selection'}
        {#each parts as g (g.part)}<section><h4>{g.name}</h4><SelectionTools part={g.part as 'type'} /></section>{/each}
      {:else if Inspector && pickKind}
        {#each INSPECTORS[pickKind].groups as g (g.part)}<section><h4>{g.name}</h4><Inspector part={g.part} kind={pickKind} /></section>{/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .strip { position: absolute; left: 0; right: 0; bottom: var(--p-live-inset, 0px); max-height: 45%; display: flex; flex-direction: column; background: var(--panel); border-top: 1px solid var(--line); z-index: 20; touch-action: pan-y; }
  header { display: flex; align-items: center; justify-content: space-between; min-height: var(--p-row); padding: 0 var(--p-gap) 0 var(--p-pad); flex: none; font-size: var(--p-font); }
  .acts { display: flex; gap: var(--p-gap); }
  button { min-height: var(--p-row); min-width: 64px; border: 0; border-radius: var(--p-radius); background: var(--panel-2); color: var(--text); font: inherit; touch-action: manipulation; }
  .done { background: var(--accent); color: #fff; font-weight: 600; }
  .body { overflow: auto; padding: 0 var(--p-gap) var(--p-gap); font-size: var(--p-font-small); }
  section { padding: 2px 0 6px; }
  h4 { margin: 0 0 2px; font-size: var(--p-font-small); font-weight: 600; color: var(--muted); }
</style>
