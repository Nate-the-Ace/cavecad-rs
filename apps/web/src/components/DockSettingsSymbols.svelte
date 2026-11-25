<script lang="ts">
  // User window > Options > Symbols (the dock layout): the symbol standard, NSS or UIS. The same stored choice
  // (setSymbolStandard, STANDARD_KEY) the Symbols panel and the phone's picker read; the Symbols panel keeps its own
  // switch too, where the choice is used.
  import { phonePrefs, setSymbolStandard } from '../lib/phone-prefs.svelte';
</script>

<div class="dset">
  <div class="row"><span class="lab">Standard</span>
    <div class="seg" role="radiogroup" aria-label="Symbol standard">
      <button role="radio" aria-checked={phonePrefs.standard === 'nss'} class:on={phonePrefs.standard === 'nss'} title="The NSS symbols used on most US cave maps" onclick={() => setSymbolStandard('nss')}>US (NSS)</button>
      <button role="radio" aria-checked={phonePrefs.standard === 'uis'} class:on={phonePrefs.standard === 'uis'} title="The UIS symbols used on cave maps in much of the rest of the world" onclick={() => setSymbolStandard('uis')}>International (UIS)</button>
    </div>
  </div>
</div>

<style>
  .dset { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  .row { display: flex; align-items: center; gap: 8px; min-height: 30px; }
  .lab { min-width: 60px; color: var(--muted); }
  .seg { display: flex; }
  .seg button { height: 28px; padding: 0 12px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); cursor: pointer; font: inherit; font-size: 12px; }
  .seg button + button { border-left: 0; }
  .seg button:first-child { border-radius: var(--radius) 0 0 var(--radius); }
  .seg button:last-child { border-radius: 0 var(--radius) var(--radius) 0; }
  .seg button.on { background: var(--accent-bg); outline: 1px solid var(--accent); position: relative; }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
</style>
