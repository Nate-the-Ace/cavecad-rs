<script lang="ts">
  // User window > Options > Survey (the dock layout, only while instruments are on): whether the Map panel shows the
  // live shot strip when the Survey panel is not visible. The same switch as the phone's Settings > Sketch
  // (phonePrefs.liveStrip, one key on this device).
  import { phonePrefs, setLiveStrip } from '../lib/phone-prefs.svelte';
</script>

<div class="dset">
  <label class="row check"><input type="checkbox" checked={phonePrefs.liveStrip} onchange={(e) => setLiveStrip(e.currentTarget.checked)} /> Live shot strip on the Map (while the Survey panel is hidden)</label>
</div>

<style>
  .dset { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  .row { display: flex; align-items: center; gap: 8px; min-height: 30px; cursor: pointer; }
</style>
