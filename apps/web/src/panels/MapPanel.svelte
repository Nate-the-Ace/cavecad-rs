<script lang="ts">
  // The Map. On the dock (desktop, web, iPad) the live shot strip sits over its bottom while instruments are on and the
  // Survey panel is not visible (D3 B; user window > Options > Survey turns it off); the phone mounts its own strip
  // over the Sketch screen, so this file draws only the view there.
  import Viewport3D from '../components/Viewport3D.svelte';
  import LiveStrip from '../components/LiveStrip.svelte';
  import { phone } from '../lib/phone.svelte';
  import { phonePrefs } from '../lib/phone-prefs.svelte';
  import { showInstruments } from '../lib/features';
  import { liveStripOnDock } from '../lib/phone-livestrip';
  import { panelState, showPanel } from '../lib/workspace.svelte';

  const strip = $derived(liveStripOnDock({ phone: phone.on, instruments: showInstruments, enabled: phonePrefs.liveStrip, surveyVisible: !!panelState.visible.survey }));
</script>

{#if phone.on}
  <Viewport3D />
{:else}
  <div class="mapdock">
    <Viewport3D />
    {#if strip}<LiveStrip dock onDetail={() => showPanel('survey')} />{/if}
  </div>
{/if}

<style>
  .mapdock { position: relative; width: 100%; height: 100%; container: mapdock / size; }
</style>
