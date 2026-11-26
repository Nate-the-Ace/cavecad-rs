<script lang="ts">
  // The settings a whole selection shares, whatever it is made of (stations, legs, splays, drawn objects, in any mix): the
  // properties every selected object has (lib/property-defs.ts), each showing the value they share, or VARIOUS where they
  // differ. A change is written to every object that does not already have it, as undoable steps in the Stations history
  // (and the drawing's own). Nothing here is specific to a kind of object: a new property in the registry shows up here.
  import { untrack } from 'svelte';
  import LengthField from './LengthField.svelte';
  import PanelUndo from './PanelUndo.svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { byGroup, sharedValue, writeAll, type Prop, type Value } from '../lib/properties';
  import { sharedProps, type AnySubject } from '../lib/property-defs';

  let { subjects }: { subjects: AnySubject[] } = $props();

  const shared = $derived(sharedProps(subjects));
  const groups = $derived(byGroup(shared));
  let message = $state('');

  // length fields hold what is typed; everything else writes on change
  let lens = $state<Record<string, number | null>>({});
  $effect(() => {
    const list = shared.filter((p) => p.control === 'length');
    const subs = subjects;
    untrack(() => {
      lens = Object.fromEntries(list.map((p) => [p.id, sharedValue(subs, p).value as number | null]));
    });
  });
  $effect(() => {
    const now = { ...lens };
    untrack(() => {
      for (const p of shared) {
        if (p.control !== 'length') continue;
        const v = now[p.id];
        if (v === null || v === undefined) continue;
        const sv = sharedValue(subjects, p);
        if (sv.various || sv.value !== v) write(p, v);
      }
    });
  });

  function write(p: Prop, v: Value) {
    const r = writeAll(subjects, p, v);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  }
  const angleText = (p: Prop): string => {
    const sv = sharedValue(subjects, p);
    return sv.various || sv.value === null ? '' : String(Math.round((sv.value as number) * 100) / 100);
  };
  function angle(p: Prop, text: string) {
    const t = text.trim().replace('°', '');
    if (t === '') return write(p, null);
    const n = Number(t);
    if (Number.isFinite(n)) write(p, n);
  }

  /** The Dimensions group shows each edge's distance with its open box beside it. */
  const edgeCells = (list: Prop[]): { len: Prop; open?: Prop }[] =>
    list.filter((p) => p.control === 'length').map((len) => ({ len, open: list.find((p) => p.id === `${len.id}_open`) }));
  void cave;
</script>

<div class="common">
  {#if !shared.length}
    <p class="none">These have no settings in common.</p>
  {:else}
    {#each groups as g (g.group)}
      <section class="grp" aria-label={g.group}>
        {#if g.group === 'Dimensions'}
          <div class="edges">
            {#each edgeCells(g.props) as cell (cell.len.id)}
              <span class="cell">
                <LengthField id={`common-${cell.len.id}`} label={cell.len.label} min={cell.len.min} bind:value={lens[cell.len.id]} />
                {#if cell.open}
                  {@const op = cell.open}
                  {@const sv = sharedValue(subjects, op)}
                  <label class="chk" title="Open: looked, no wall"><input type="checkbox" checked={sv.value === true} indeterminate={sv.various} onchange={(e) => write(op, e.currentTarget.checked)} />open</label>
                {/if}
              </span>
            {/each}
          </div>
        {:else}
          <div class="items">
            {#each g.props as p (p.id)}
              {@const sv = sharedValue(subjects, p)}
              {#if p.control === 'toggle'}
                <label class="chk"><input type="checkbox" checked={sv.value === true} indeterminate={sv.various} onchange={(e) => write(p, e.currentTarget.checked)} />{p.label}</label>
              {:else if p.control === 'text'}
                <label class="txt"><span>{p.label}</span><input type="text" maxlength="200" value={sv.various ? '' : String(sv.value ?? '')} placeholder={sv.various ? 'VARIOUS' : p.label} aria-label={p.label} onchange={(e) => write(p, e.currentTarget.value)} /></label>
              {:else if p.control === 'angle'}
                <label class="num"><span>{p.label}</span><input type="text" inputmode="decimal" value={angleText(p)} placeholder={sv.various ? 'VARIOUS' : ''} aria-label={p.label} onchange={(e) => angle(p, e.currentTarget.value)} />°</label>
              {:else}
                <LengthField id={`common-${p.id}`} label={p.label} min={p.min} bind:value={lens[p.id]} />
              {/if}
            {/each}
          </div>
        {/if}
      </section>
    {/each}
    <div class="undo"><PanelUndo scope="stations" label="edits to these" /></div>
  {/if}
  {#if message}<div class="msg" role="alert">{message}</div>{/if}
</div>

<style>
  .common { display: flex; flex-direction: column; gap: 4px; width: 100%; font-size: 12px; }
  .none { margin: 6px 0; color: var(--muted); }
  .edges { display: grid; grid-template-columns: 1fr 1fr; gap: 2px 10px; width: 100%; }
  .cell { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
  .items { display: flex; flex-wrap: wrap; gap: 4px 14px; align-items: center; }
  .common :global(.field) { gap: 4px; }
  .common :global(.field label) { min-width: 34px; }
  .common :global(input[inputmode]) { width: 4.6em; }
  .common :global(.msg) { display: none; }
  .chk { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); }
  .txt { display: flex; align-items: center; gap: 8px; width: 100%; }
  .txt span, .num span { flex: none; min-width: 3.4em; color: var(--muted); }
  .txt input { flex: 1 1 0; min-width: 0; height: 28px; padding: 0 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); font: inherit; font-size: var(--p-font-input, 16px); }
  .num { display: inline-flex; align-items: center; gap: 4px; }
  .num input { width: 4.6em; height: 26px; padding: 0 6px; text-align: right; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); font: inherit; }
  .undo { display: flex; gap: 2px; }
  .msg { color: var(--danger); font-size: 11px; }
</style>
