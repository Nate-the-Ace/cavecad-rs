<script lang="ts">
  // The page around the Survey panel: the panel at tablet or phone width on the left, and what the mock
  // notebook holds on the right, so every tap can be checked against what was written.
  import { untrack } from 'svelte';
  import SurveyPanel from '../src/panels/SurveyPanel.svelte';
  import { cave, setUnits } from '../src/lib/cave.svelte';
  import { loadEngine } from '../src/lib/engine';

  type Shot = { id: string; from: string; to: string | null; c: Record<string, number | null> };
  type Book = { shots: Shot[]; walls: Record<string, unknown> };
  const bookOf = () => (globalThis as unknown as { __book?: Book }).__book;

  let shots = $state<Shot[]>([]);
  let width = $state(420);

  $effect(() => {
    void cave.version;
    untrack(() => {
      shots = structuredClone($state.snapshot(bookOf()?.shots ?? []));
    });
  });
  loadEngine();

  const f = (v: number | null | undefined, d = 2) => (v === null || v === undefined ? '' : (+v.toFixed(d)).toString());
  function reset() {
    const book = bookOf();
    if (book) {
      book.shots.length = 0;
      for (const k of Object.keys(book.walls)) delete book.walls[k];
    }
    location.reload();
  }
</script>

<div class="page">
  <header>
    <b>CaveCAD Survey harness</b>
    <span>The real Survey panel on a mock notebook. The Map, Profile and Cross Section need the real engine; Bluetooth needs a real X1. Use the Simulator.</span>
    <span class="sp"></span>
    <label>Width
      <select bind:value={width}>
        <option value={380}>Phone (380)</option>
        <option value={420}>Narrow tablet panel (420)</option>
        <option value={520}>Wide tablet panel (520)</option>
        <option value={820}>Full tablet (820)</option>
      </select>
    </label>
    <label>Units
      <select onchange={(e) => setUnits(e.currentTarget.value === 'imperial' ? 'imperial' : 'metric')}><option>metric</option><option>imperial</option></select>
    </label>
    <button onclick={reset}>Reset notebook</button>
  </header>
  <main>
    <section class="panelbox" style="width:{width}px"><SurveyPanel /></section>
    <section class="book" aria-label="Mock notebook">
      <h3>Mock notebook <small>what the panel has written</small></h3>
      <table>
        <thead><tr><th>From</th><th>To</th><th>Tape (m)</th><th>Az fs</th><th>Az bs</th><th>Inc fs</th><th>Inc bs</th></tr></thead>
        <tbody>
          {#each shots as s (s.id)}
            <tr class:splay={!s.to}>
              <td>{s.from}</td><td>{s.to ?? 'splay'}</td><td>{f(s.c.distance, 3)}</td><td>{f(s.c.az_fs)}</td><td>{f(s.c.az_bs)}</td><td>{f(s.c.inc_fs)}</td><td>{f(s.c.inc_bs)}</td>
            </tr>
          {:else}
            <tr><td colspan="7" class="none">Nothing written yet. Press Simulator, then Take shot.</td></tr>
          {/each}
        </tbody>
      </table>
    </section>
  </main>
</div>

<style>
  .page { display: flex; flex-direction: column; height: 100vh; background: var(--bg, #111); color: var(--text); font-size: 13px; }
  header { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid var(--line); }
  header span { color: var(--muted); font-size: 12px; }
  .sp { flex: 1; }
  main { flex: 1; display: flex; gap: 12px; padding: 12px; min-height: 0; }
  .panelbox { border: 1px solid var(--line); border-radius: 10px; overflow: hidden; background: var(--panel); flex: none; max-width: 100%; }
  .book { flex: 1; overflow: auto; min-width: 280px; }
  h3 { margin: 0 0 8px; }
  h3 small { color: var(--muted); font-weight: 400; margin-left: 8px; }
  table { border-collapse: collapse; width: 100%; font-variant-numeric: tabular-nums; }
  th { text-align: left; color: var(--muted); font-weight: 600; font-size: 11px; padding: 3px 6px; border-bottom: 1px solid var(--line); }
  td { padding: 3px 6px; border-bottom: 1px solid var(--line); }
  tr.splay td { color: var(--muted); }
  .none { color: var(--muted); padding: 14px 6px; }
  select, button { font: inherit; padding: 4px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  label { display: inline-flex; gap: 6px; align-items: center; color: var(--muted); }
  @media (max-width: 900px) {
    main { flex-direction: column; overflow: auto; }
    .panelbox { height: 78vh; }
  }
</style>
