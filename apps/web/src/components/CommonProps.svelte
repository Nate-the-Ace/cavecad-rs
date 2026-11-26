<script lang="ts">
  // The settings a whole selection shares, whatever it is made of (stations, legs, splays, drawn objects, in any mix): the
  // properties every selected object has (lib/property-defs.ts), each showing the value they share, or VARIOUS where they
  // differ. A change is written to every object that does not already have it, as undoable steps in the Stations history
  // (and the drawing's own). Nothing here is specific to a kind of object: a new property in the registry shows up here.
  import { untrack } from 'svelte';
  import LengthField from './LengthField.svelte';
  import PanelUndo from './PanelUndo.svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, type Scope } from '../lib/engine';
  import { byGroup, sharedValue, writeAll, type Prop, type Value } from '../lib/properties';
  import { sharedProps, type AnySubject } from '../lib/property-defs';

  let { subjects }: { subjects: AnySubject[] } = $props();

  const shared = $derived(sharedProps(subjects));
  const groups = $derived(byGroup(shared));
  let message = $state('');
  /** A slider being dragged: shown at once, written when it is let go. */
  let drag = $state<Record<string, number>>({});

  // length fields hold what is typed; everything else writes on change
  // (filled before the first render: a length field handed `undefined` has nothing to show)
  let lens = $state<Record<string, number | null>>(
    Object.fromEntries(shared.filter((p) => p.control === 'length').map((p) => [p.id, sharedValue(subjects, p).value as number | null])),
  );
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
    // one change to several things is one undo step (in each panel it touches)
    const r = engine.editGroup(() => writeAll(subjects, p, v));
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
  /** The histories these edits go to: stations and shots in Stations, a drawn object in the view it was drawn in. */
  const scopes = $derived.by<{ scope: Scope; label: string }[]>(() => {
    const names: Record<string, string> = { stations: 'the survey', map: 'the map', profile: 'the profile', section: 'the cross section' };
    const seen = new Set<string>();
    for (const s of subjects) seen.add(s.type === 'drawn' ? (s.data as { view: string }).view : 'stations');
    return [...seen].map((scope) => ({ scope: scope as Scope, label: `in ${names[scope] ?? scope}` }));
  });
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
                <LengthField id={`common-${cell.len.id}`} label={cell.len.label} min={cell.len.min} placeholder={sharedValue(subjects, cell.len).various ? 'VARIOUS' : ''} bind:value={lens[cell.len.id]} />
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
              {:else if p.control === 'select'}
                {@const opts = p.options?.() ?? []}
                <label class="sel"><span>{p.label}</span>
                  <select aria-label={p.label} value={sv.various ? '' : String(sv.value ?? '')} onchange={(e) => write(p, e.currentTarget.value)}>
                    {#if sv.various}<option value="" disabled selected>VARIOUS</option>{/if}
                    {#each opts as [key, label] (key)}<option value={key}>{label}</option>{/each}
                  </select>
                </label>
              {:else if p.control === 'range'}
                {@const shown = drag[p.id] ?? (sv.various ? null : (sv.value as number))}
                <label class="rng"><span>{p.label} <b>{shown === null ? 'VARIOUS' : (p.format?.(shown) ?? String(shown))}</b></span>
                  <input type="range" min={p.min} max={p.max} step={p.step} value={shown ?? p.min} aria-label={p.label}
                    oninput={(e) => (drag[p.id] = Number(e.currentTarget.value))}
                    onchange={(e) => { const v = Number(e.currentTarget.value); delete drag[p.id]; write(p, v); }} />
                </label>
              {:else if p.control === 'color'}
                <label class="clr"><span>{p.label}</span>
                  <input type="color" aria-label={p.label} value={sv.various || !sv.value ? '#e8e6e1' : String(sv.value)} onchange={(e) => write(p, e.currentTarget.value)} />
                  <button class="small" disabled={!sv.various && !sv.value} onclick={() => write(p, '')}>Auto</button>
                </label>
              {:else}
                <LengthField id={`common-${p.id}`} label={p.label} min={p.min} placeholder={sv.various ? 'VARIOUS' : ''} bind:value={lens[p.id]} />
              {/if}
            {/each}
          </div>
        {/if}
      </section>
    {/each}
    <div class="undos">{#each scopes as u (u.scope)}<span class="undo"><PanelUndo scope={u.scope} label={u.label} /></span>{/each}</div>
  {/if}
  {#if message}<div class="msg" role="alert">{message}</div>{/if}
</div>

<style>
  .common { display: flex; flex-direction: column; gap: 4px; width: 100%; font-size: 12px; }
  .none { margin: 6px 0; color: var(--muted); }
  .edges { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 2px 10px; width: 100%; }
  .cell { display: flex; align-items: center; gap: 4px; white-space: nowrap; min-width: 0; }
  /* the distance field takes what is left, so the open box and its word always fit beside it */
  .cell :global(.field) { flex: 1 1 0; min-width: 0; }
  .cell :global(.field input) { flex: 1 1 0; width: auto; min-width: 2.6em; }
  .cell :global(.field label) { flex: none; }
  .cell .chk { flex: none; font-size: 11px; gap: 3px; }
  .items { display: flex; flex-wrap: wrap; gap: 4px 14px; align-items: center; }
  .common :global(.field) { gap: 4px; }
  .common :global(.field label) { min-width: 30px; }
  .items :global(input[inputmode]) { width: 4.6em; }
  .common :global(input::placeholder) { color: var(--muted); font-size: 10px; letter-spacing: -0.2px; }
  .common :global(.msg) { display: none; }
  .chk { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); }
  .txt { display: flex; align-items: center; gap: 8px; width: 100%; }
  .txt span, .num span { flex: none; min-width: 3.4em; color: var(--muted); }
  .txt input { flex: 1 1 0; min-width: 0; height: 28px; padding: 0 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); font: inherit; font-size: var(--p-font-input, 16px); }
  .num { display: inline-flex; align-items: center; gap: 4px; }
  .num input { width: 4.6em; height: 26px; padding: 0 6px; text-align: right; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); font: inherit; }
  .sel, .rng, .clr { display: flex; align-items: center; gap: 8px; color: var(--muted); }
  .sel { width: 100%; }
  .sel select { flex: 1 1 0; min-width: 0; height: 28px; padding: 0 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); font: inherit; font-size: var(--p-font-input, 16px); }
  .rng { flex-direction: column; align-items: stretch; gap: 2px; flex: 1 1 140px; min-width: 120px; }
  .rng b { color: var(--text); font-weight: 600; }
  .rng input { margin: 0; accent-color: var(--accent); }
  .clr input[type='color'] { width: 34px; height: 26px; padding: 0; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); }
  .small { font: inherit; font-size: 12px; padding: 3px 9px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  .small:disabled { opacity: 0.5; }
  .undos { display: flex; flex-wrap: wrap; gap: 4px 12px; }
  .undo { display: inline-flex; gap: 2px; }
  .msg { color: var(--danger); font-size: 11px; }
</style>
