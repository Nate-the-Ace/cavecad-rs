<script lang="ts">
  // The page around the dock: a thin bar (panel toggles, units, resets), the dock itself (panels the person
  // arranges as they like, remembered on this device), and the feed that keeps the sketchpad in step with
  // the mock notebook. The Survey panel is the real one; the notebook is a mock.
  import { onMount, untrack } from 'svelte';
  import { cave, setUnits } from '../src/lib/cave.svelte';
  import { loadEngine } from '../src/lib/engine';
  import { survey } from '../src/lib/survey.svelte';
  import { standing } from '../src/lib/survey';
  import { sketch } from '../src/lib/sketch.svelte';
  import type { DimsByStation, SurveyShot } from '../src/lib/sketch-geometry';
  import { PANELS, initDock, panelState, resetDock, togglePanel } from './dock.svelte';

  type Shot = { id: string; from: string; to: string | null; c: Record<string, number | null> };
  type Book = { shots: Shot[]; walls: Record<string, unknown> };
  const bookOf = () => (globalThis as unknown as { __book?: Book }).__book;

  let host: HTMLDivElement;
  onMount(() => {
    sketch.open('harness');
    return initDock(host);
  });
  loadEngine();

  // The sketchpad draws what the mock notebook holds (the real app would read the notebook page the same way).
  $effect(() => {
    void cave.version;
    const here = survey.state.from ? standing(survey.state) : null;
    untrack(() => {
      const book = bookOf();
      const shots: SurveyShot[] = ($state.snapshot(book?.shots ?? []) as Shot[]).map((s) => ({
        id: s.id, from: s.from, to: s.to, distance: s.c.distance ?? null, az: s.c.az_fs ?? null, inc: s.c.inc_fs ?? null, azBack: s.c.az_bs ?? null, incBack: s.c.inc_bs ?? null,
      }));
      sketch.setSource(shots, structuredClone($state.snapshot(book?.walls ?? {})) as DimsByStation, here);
    });
  });

  function resetNotebook() {
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
    <span class="note">The real Survey panel and sketchpad on a mock notebook. Drag tabs, resize, float or close any panel; the layout is remembered. The real Map needs the real engine; Bluetooth needs a real X1: use the Simulator.</span>
    <span class="sp"></span>
    <span class="toggles" role="group" aria-label="Panels">
      {#each Object.entries(PANELS) as [id, def]}
        <button class:on={panelState.open[id]} aria-pressed={!!panelState.open[id]} onclick={() => togglePanel(id)}>{def.title}</button>
      {/each}
    </span>
    <label>Units
      <select onchange={(e) => setUnits(e.currentTarget.value === 'imperial' ? 'imperial' : 'metric')}><option>metric</option><option>imperial</option></select>
    </label>
    <button onclick={resetDock}>Reset layout</button>
    <button onclick={resetNotebook}>Reset notebook</button>
  </header>
  <div class="dock" bind:this={host}></div>
</div>

<style>
  .page { display: flex; flex-direction: column; height: 100vh; background: var(--bg, #111); color: var(--text); font-size: 13px; }
  header { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line); }
  .note { color: var(--muted); font-size: 12px; max-width: 46em; }
  .sp { flex: 1; }
  .toggles { display: inline-flex; gap: 4px; }
  .dock { flex: 1; min-height: 0; }
  select, button { font: inherit; padding: 4px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button.on { border-color: var(--accent); color: var(--accent); }
  label { display: inline-flex; gap: 6px; align-items: center; color: var(--muted); }
</style>
