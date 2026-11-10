<script lang="ts">
  // Where the cave is: click the entrance on USGS imagery, or type its coordinate (decimal or degrees,
  // minutes and seconds), and say which station the entrance is. The coordinate is shown to the person
  // only: it is never logged and never put in the app's address.
  import 'leaflet/dist/leaflet.css';
  import type * as Leaflet from 'leaflet';
  import { onMount, untrack } from 'svelte';
  import { parseLatLon } from '../lib/latlon-text';

  let {
    start = null,
    station = null,
    stations,
    onpick,
    oncancel,
  }: {
    start?: { lat: number; lon: number } | null;
    station?: string | null;
    stations: { id: string; label: string }[];
    onpick: (p: { lat: number; lon: number; station: string; source: 'map' | 'typed' }) => void;
    oncancel: () => void;
  } = $props();

  const TILES = 'https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile/{z}/{y}/{x}';

  let el = $state<HTMLDivElement>();
  let point = $state<{ lat: number; lon: number } | null>(null);
  let source = $state<'map' | 'typed'>('map');
  let text = $state('');
  let bad = $state(false);
  /** The entrance station: the one suggested, else the first. */
  let chosen = $state(untrack(() => (station && stations.some((s) => s.id === station) ? station : (stations[0]?.id ?? ''))));
  let mapFailed = $state('');
  let map: Leaflet.Map | null = null;
  let marker: Leaflet.CircleMarker | null = null;
  let L: typeof Leaflet | null = null;

  const fmt = (p: { lat: number; lon: number }) => `${p.lat.toFixed(6)}, ${p.lon.toFixed(6)}`;

  function place(p: { lat: number; lon: number }, pan: boolean) {
    point = p;
    if (!map || !L) return;
    if (marker) marker.setLatLng([p.lat, p.lon]);
    else marker = L.circleMarker([p.lat, p.lon], { radius: 8, color: '#fff', weight: 2, fillColor: '#e4572e', fillOpacity: 0.9 }).addTo(map);
    if (pan) map.setView([p.lat, p.lon], Math.max(map.getZoom(), 16));
  }

  onMount(() => {
    let gone = false;
    if (start) {
      point = start;
      text = fmt(start);
    }
    (async () => {
      try {
        L = (await import('leaflet')).default;
        if (gone || !el) return;
        map = L.map(el).setView(start ? [start.lat, start.lon] : [37.5, -86.0], start ? 16 : 7);
        L.tileLayer(TILES, { maxZoom: 19, attribution: 'Imagery: USGS The National Map' }).addTo(map);
        map.on('click', (e: Leaflet.LeafletMouseEvent) => {
          const p = { lat: e.latlng.lat, lon: e.latlng.lng };
          text = fmt(p);
          bad = false;
          source = 'map';
          place(p, false);
        });
        if (start) place(start, false);
        queueMicrotask(() => map?.invalidateSize());
      } catch (e) {
        mapFailed = `The map could not be shown (${(e as Error).message}). You can still type the coordinate.`;
      }
    })();
    return () => {
      gone = true;
      map?.remove();
      map = null;
      marker = null;
    };
  });

  /** Reads the typed coordinate; true when it could be read. */
  function readTyped(): boolean {
    if (!text.trim()) {
      bad = false;
      return false;
    }
    const p = parseLatLon(text);
    bad = !p;
    if (!p) return false;
    if (!point || fmt(point) !== fmt(p)) {
      source = 'typed';
      place(p, true);
    }
    return true;
  }

  function use() {
    if (!readTyped() || !point || !chosen) return;
    onpick({ lat: point.lat, lon: point.lon, station: chosen, source });
  }

  function keys(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      oncancel();
    }
  }
</script>

<div class="veil" role="presentation" onclick={oncancel}>
  <div class="box" role="dialog" aria-modal="true" aria-label="Where is the cave?" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={keys}>
    <h2>Where is the cave?</h2>
    <p class="hint">Click the entrance on the aerial imagery, or type its coordinate (for example 36.99, -86.44 or 36°59'24"N 86°26'24"W).</p>
    <div class="map" bind:this={el}></div>
    {#if mapFailed}<p class="err" role="alert">{mapFailed}</p>{/if}
    <div class="fields">
      <label>Coordinate
        <input bind:value={text} placeholder="latitude, longitude" autocomplete="off" spellcheck="false" oninput={() => (bad = false)} onblur={readTyped} onkeydown={(e) => e.key === 'Enter' && readTyped()} aria-invalid={bad} />
      </label>
      <label>Entrance station
        <select bind:value={chosen}>
          {#each stations as s (s.id)}<option value={s.id}>{s.label}</option>{/each}
        </select>
      </label>
    </div>
    {#if bad}<p class="err" role="alert">Couldn't read that coordinate.</p>{/if}
    {#if !stations.length}<p class="err" role="alert">This cave has no stations yet, so there is no entrance to place.</p>{/if}
    <div class="row">
      <button onclick={oncancel}>Cancel</button>
      <button class="go" disabled={!text.trim() || bad || !chosen} onclick={use}>Use this location</button>
    </div>
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); display: grid; place-items: center; z-index: 60; }
  .box { width: min(720px, 94vw); max-height: 94vh; overflow: auto; background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; outline: none; box-sizing: border-box; }
  h2 { margin: 0; font-size: 17px; }
  .map { height: min(420px, 50vh); border: 1px solid var(--line); border-radius: 8px; background: var(--panel-2); }
  .fields { display: flex; gap: 12px; flex-wrap: wrap; }
  .fields label:first-child { flex: 1; min-width: 220px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
  input, select { font: inherit; font-size: 14px; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  input[aria-invalid='true'] { border-color: var(--danger); }
  .hint { margin: 0; font-size: 12px; color: var(--muted); }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
  button { font: inherit; font-size: 13px; padding: 6px 14px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
  button:disabled { opacity: 0.5; cursor: default; }
</style>
