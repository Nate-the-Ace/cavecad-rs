<script lang="ts">
  // Surface Data: the aerial photo and ground elevation above the cave, from USGS. Finds where the cave is
  // (the stored location, else the fixed point an imported survey carried, else asks with the map picker),
  // downloads what is ticked, records it in the cave, and says what came back. The coordinate is shown here
  // to the person and nowhere else: never logged.
  import { untrack } from 'svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, type GeoStatus } from '../lib/engine';
  import { fetchSurface, surface } from '../lib/surface.svelte';
  import { FOOT, inUnits, metresPer, formatLength } from '../lib/units';
  import LengthField from './LengthField.svelte';
  import LocationPicker from './LocationPicker.svelte';

  let status = $state<GeoStatus | null>(null);
  let problem = $state('');
  let photo = $state(true);
  let elevation = $state(true);
  let intervalM = $state<number | null>(null);
  let lines = $state<string[]>([]);
  let note = $state('');
  let picker = $state<{ start: { lat: number; lon: number } | null; station: string | null; thenDownload: boolean } | null>(null);

  const defaultInterval = () => (cave.units === 'imperial' ? 10 * FOOT : 5);

  function refresh() {
    const s = engine.geoStatus();
    status = 'error' in s ? null : s;
    problem = 'error' in s ? s.error : '';
  }

  // Each time the dialog opens it starts fresh from the cave.
  $effect(() => {
    if (!surface.dialog) return;
    untrack(() => {
      refresh();
      intervalM = status?.interval_m ?? defaultInterval();
      lines = [];
      note = '';
      picker = null;
    });
  });

  const label = (id: string) => status?.stations.find((s) => s.id === id)?.label ?? id;
  const coord = (p: { lat: number; lon: number }) => `${p.lat.toFixed(6)}, ${p.lon.toFixed(6)}`;
  const SOURCES: Record<string, string> = { import: 'from the imported survey', map: 'picked on the map', typed: 'typed in', pick: 'picked on the surface', 'surface-pick': 'picked on the surface' };

  /** Stores the contour spacing when it changed (metres, rounded to 3 decimals in the display unit). */
  function applyInterval() {
    if (intervalM === null || !(intervalM > 0)) return;
    const u = cave.units;
    const m = (Math.round(inUnits(intervalM, u) * 1000) / 1000) * metresPer(u);
    const was = status?.interval_m ?? defaultInterval();
    if (Math.abs(m - was) < 1e-9) return;
    const r = engine.setContourInterval(m);
    if (r !== 'ok') problem = r.replace(/^error: /, '');
    else changed();
  }

  function setLocation(l: { station: string; lat: number; lon: number; source: string }): boolean {
    const r = engine.geoSetLocation(l);
    if (r !== 'ok') {
      problem = r.replace(/^error: /, '');
      return false;
    }
    changed();
    refresh();
    return true;
  }

  async function download() {
    lines = ['Downloading…'];
    const r = await fetchSurface({ photo, elevation });
    lines = r.lines;
    refresh();
  }

  async function ok() {
    problem = '';
    note = '';
    applyInterval();
    refresh();
    const st = status;
    if (!st) return;
    const c = st.candidate;
    if (c.kind === 'stored') return download();
    if (c.kind === 'imported') {
      note = `Using the fixed point from the imported survey (station ${label(c.station)}).`;
      if (setLocation({ station: c.station, lat: c.lat, lon: c.lon, source: 'import' })) await download();
      return;
    }
    picker = { start: null, station: c.station, thenDownload: true };
  }

  function changeLocation() {
    const st = status;
    if (!st) return;
    const c = st.candidate;
    const start = st.location ?? (c.kind === 'imported' ? c : null);
    const station = st.location?.station ?? (c.kind === 'stored' ? null : c.station);
    picker = { start: start ? { lat: start.lat, lon: start.lon } : null, station, thenDownload: false };
  }

  async function picked(p: { lat: number; lon: number; station: string; source: 'map' | 'typed' }) {
    const then = picker?.thenDownload;
    picker = null;
    if (!setLocation(p)) return;
    if (then) await download();
    else lines = ['The location changed. Download to fetch the surface for it.'];
  }

  const close = () => {
    picker = null;
    surface.dialog = false;
  };

  const anything = $derived(photo || elevation);
</script>

{#if surface.dialog}
  <div class="veil" role="presentation" onclick={close}>
    <div class="box" role="dialog" aria-modal="true" aria-label="Surface Data" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && !picker && close()}>
      <h2>Surface Data</h2>
      <p class="hint">The aerial photo and the ground elevation above the cave, downloaded from USGS. They are kept with the cave and drawn under the map.</p>
      {#if problem}<p class="err" role="alert">{problem}</p>{/if}
      {#if status}
        <section class="where">
          {#if status.location}
            <div><b>Entrance:</b> station {label(status.location.station)}, {coord(status.location)} <span class="muted">({SOURCES[status.location.source] ?? status.location.source})</span></div>
            {#if status.location.ground_m !== undefined && Number.isFinite(status.location.ground_m)}<div><b>Ground at the entrance:</b> {formatLength(status.location.ground_m, cave.units, 1)} (NAVD88)</div>{/if}
          {:else if status.candidate.kind === 'imported'}
            <div><b>Entrance:</b> the imported survey fixes station {label(status.candidate.station)} at {coord(status.candidate)}. It will be used when you download.</div>
          {:else}
            <div><b>Entrance:</b> not placed yet. You will be asked where the cave is.</div>
          {/if}
          {#if status.surface}
            <div class="muted">Downloaded {status.surface.fetched}: {[status.surface.photo ? 'aerial photo' : '', status.surface.elevation ? 'ground elevation' : ''].filter(Boolean).join(' and ') || 'nothing kept'}.</div>
          {/if}
          <button class="link" disabled={surface.busy} onclick={changeLocation}>Change location…</button>
        </section>
        <fieldset>
          <legend>Download</legend>
          <label><input type="checkbox" bind:checked={photo} /> Aerial photo</label>
          <label><input type="checkbox" bind:checked={elevation} /> Ground elevation</label>
        </fieldset>
        <div class="interval">
          <LengthField id="surface-contour-interval" label="Contour spacing" bind:value={intervalM} min={0.001} />
        </div>
      {/if}
      {#if note}<p class="note">{note}</p>{/if}
      {#if lines.length}
        <ul class="report" aria-live="polite">
          {#each lines as l}<li>{l}</li>{/each}
        </ul>
      {/if}
      <div class="row">
        <button onclick={close}>Close</button>
        <button class="go" disabled={!status || surface.busy || !anything} onclick={ok}>{surface.busy ? 'Downloading…' : status?.surface ? 'Download again' : 'Download'}</button>
      </div>
    </div>
  </div>
  {#if picker && status}
    <LocationPicker start={picker.start} station={picker.station} stations={status.stations} onpick={picked} oncancel={() => (picker = null)} />
  {/if}
{/if}

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); display: grid; place-items: center; z-index: 50; }
  .box { width: min(480px, 92vw); max-height: 92vh; overflow: auto; background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; outline: none; box-sizing: border-box; }
  h2 { margin: 0; font-size: 17px; }
  .where { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  .muted { color: var(--muted); }
  fieldset { border: 1px solid var(--line); border-radius: 8px; display: flex; gap: 16px; padding: 6px 10px 8px; font-size: 13px; }
  fieldset label { display: flex; align-items: center; gap: 6px; }
  legend { font-size: 12px; color: var(--muted); padding: 0 4px; }
  .hint { margin: 0; font-size: 12px; color: var(--muted); }
  .note { margin: 0; font-size: 13px; }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  .report { margin: 0; padding-left: 18px; font-size: 13px; display: flex; flex-direction: column; gap: 2px; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
  button { font: inherit; font-size: 13px; padding: 6px 14px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
  button.link { align-self: flex-start; padding: 2px 0; border: 0; background: none; color: var(--accent); }
  button:disabled { opacity: 0.5; cursor: default; }
</style>
