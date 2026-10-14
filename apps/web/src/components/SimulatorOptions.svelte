<script lang="ts">
  // Which instrument the simulator pretends to be, and how it misbehaves: to test the logic behind each device, and
  // to find its quirks, before there is a real one. Two short rows to fit the ribbon. Changing either while the
  // simulator is running starts a new one of that kind, carrying on from the station the surveyor is at.
  import * as live from '../lib/survey.svelte';
  import { survey } from '../lib/survey.svelte';

  const device = $derived(live.simDevices().find((d) => d.kind === survey.sim.device) ?? live.simDevices()[0]);
  const quirk = $derived(device?.quirks.find((q) => q.id === survey.sim.quirk) ?? device?.quirks[0]);

  function pickDevice(kind: string) {
    survey.sim.device = kind as typeof survey.sim.device;
    survey.sim.quirk = 'none'; // each device has its own quirks
    restart();
  }
  function pickQuirk(id: string) {
    survey.sim.quirk = id;
    restart();
  }
  /** A simulator already running becomes the one chosen. */
  function restart() {
    if (survey.kind === 'simulator') void live.connect('simulator');
  }
</script>

<div class="sim">
  <label title="Which instrument to pretend to be: each one sends its own bytes, and they go through that instrument's real decoder">
    <span>Device</span>
    <select value={survey.sim.device} aria-label="Instrument to simulate" onchange={(e) => pickDevice(e.currentTarget.value)}>
      {#each live.simDevices() as d}<option value={d.kind}>{d.label}</option>{/each}
    </select>
  </label>
  <label title={quirk?.expect ?? ''}>
    <span>Quirk</span>
    <select value={survey.sim.quirk} aria-label="Quirk of the simulated instrument" onchange={(e) => pickQuirk(e.currentTarget.value)}>
      {#each device?.quirks ?? [] as q}<option value={q.id}>{q.label}</option>{/each}
    </select>
  </label>
</div>

<style>
  .sim { display: flex; flex-direction: column; justify-content: center; gap: 6px; height: 100%; font-size: 12px; min-width: 230px; }
  label { display: flex; align-items: center; gap: 8px; height: 24px; color: var(--text); }
  span { width: 44px; color: var(--muted); }
  select { flex: 1; min-width: 0; height: 24px; font: inherit; padding: 0 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
</style>
