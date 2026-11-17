<script lang="ts">
  // What each X1 class draws (Feature, Ridge, Generic: the X1's M button while the laser is on). Folded away until opened.
  import { engine, loadEngine } from '../lib/engine';
  import { MARK_CLASSES, setMarkAction, x1Marks } from '../lib/x1-marks.svelte';

  let symbols = $state<{ key: string; name: string; category: string }[]>([]);
  let lines = $state<{ key: string; label: string }[]>([]);
  loadEngine().then(() => {
    symbols = engine.symbols().map((s) => ({ key: s.key, name: s.name, category: s.category }));
    lines = engine.drawTools().filter((t) => !t.key.startsWith('sym-')).map((t) => ({ key: t.key, label: t.label }));
  });
  const categories = $derived([...new Set(symbols.map((s) => s.category))]);
</script>

<details class="marks">
  <summary>X1 marks: what Feature, Ridge and Generic draw</summary>
  <p class="hint">Mark a shot on the X1 with <b>M</b> while the laser is on. The splay is coloured by its mark; here you choose what it also draws.</p>
  {#each MARK_CLASSES as c (c.id)}
    <label class="row" title={c.hint}>
      <span class="name">{c.label}</span>
      <select value={x1Marks[c.id]} onchange={(e) => setMarkAction(c.id, e.currentTarget.value)} aria-label={`What a shot marked ${c.label} draws`}>
        <option value="">Colour the splay only</option>
        <optgroup label="Line through the hits">
          {#each lines as l (l.key)}<option value={l.key}>{l.label}</option>{/each}
        </optgroup>
        {#each categories as cat (cat)}
          <optgroup label={`Symbol: ${cat}`}>
            {#each symbols.filter((s) => s.category === cat) as s (s.key)}<option value={s.key}>{s.name}</option>{/each}
          </optgroup>
        {/each}
      </select>
    </label>
  {/each}
</details>

<style>
  .marks { border: 1px solid var(--line); border-radius: var(--radius); padding: 6px 8px; }
  summary { cursor: pointer; font-size: 13px; min-height: 28px; display: flex; align-items: center; }
  .hint { margin: 6px 0; font-size: 12px; color: var(--muted); }
  .row { display: flex; align-items: center; gap: 8px; margin: 4px 0; }
  .name { min-width: 64px; font-size: 13px; }
  select { flex: 1; min-width: 0; min-height: 34px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); font: inherit; font-size: 13px; }
</style>
