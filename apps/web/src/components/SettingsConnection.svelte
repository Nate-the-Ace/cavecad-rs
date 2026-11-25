<script lang="ts">
  // Settings > Connection (phone): the instrument link, the X1's device control (standard, accuracy check; what its
  // own scan is checked as is the Sweep default in Settings > Survey), guided calibration and the X1 quick reference, and the instrument log (capture mode for
  // an instrument that is not supported yet). The log is the one box that scrolls. No simulator on the phone.
  import { BookOpen, Bluetooth, Copy, Crosshair, Trash2, Unplug } from '@lucide/svelte';
  import CalibrationGuide from './CalibrationGuide.svelte';
  import QuickRef from './QuickRef.svelte';
  import * as live from '../lib/survey.svelte';
  import { survey, type AccuracyCheck } from '../lib/survey.svelte';
  import { isLinked, linkLabel, showsDetail } from '../lib/phone-survey';
  import './settings.css';

  const linked = $derived(isLinked(survey.status));
  const manual = $derived(survey.standard === 'manual');
  let quickRef = $state(false);
</script>

<div class="set">
  <div class="row">
    <span class="status" class:on={survey.status === 'connected'} class:warn={survey.status === 'lost' || survey.status === 'error'} role="status">{linkLabel(survey.status, survey.device)}</span>
    {#if linked}
      <button onclick={live.disconnect}><Unplug size={18} /> Disconnect</button>
    {:else}
      <button class="on" onclick={() => void live.connect('auto')}><Bluetooth size={18} /> Connect</button>
    {/if}
  </div>
  {#if survey.detail && showsDetail(survey.status)}<p class="line warn" role="alert">{survey.detail}</p>{/if}
  <label class="chk"><input type="checkbox" bind:checked={survey.anyDevice} /> Show every Bluetooth device</label>
  <div class="row">
    <button onclick={live.startCalibration} disabled={!live.aimsFirst() || !!survey.calibration}><Crosshair size={16} /> Calibration</button>
    <button onclick={() => (quickRef = true)}><BookOpen size={16} /> X1 quick reference</button>
  </div>
  <div class="row">
    <span class="lab">Standard</span>
    <div class="seg" role="radiogroup" aria-label="Survey standard">
      <button role="radio" aria-checked={manual} class:on={manual} onclick={() => live.setStandard('manual')}>Manual</button>
      <button role="radio" aria-checked={!manual} class:on={!manual} onclick={() => live.setStandard('digital')}>Digital</button>
    </div>
  </div>
  <label class="row"><span class="lab">Accuracy</span>
    <select value={survey.accuracy} onchange={(e) => live.setAccuracy(e.currentTarget.value as AccuracyCheck)} aria-label="Accuracy check">
      <option value="strict">Strict</option>
      <option value="normal">Normal</option>
      <option value="relaxed">Relaxed</option>
    </select>
  </label>
  <div class="row">
    <span class="lab">Log</span>
    <button onclick={() => void live.connect('capture')} disabled={linked}>Capture</button>
    <button onclick={() => navigator.clipboard?.writeText(survey.frames.join('\n'))} disabled={survey.frames.length === 0} aria-label="Copy log"><Copy size={16} /></button>
    <button onclick={() => (survey.frames = [])} disabled={survey.frames.length === 0} aria-label="Clear log"><Trash2 size={16} /></button>
  </div>
  <input type="text" bind:value={survey.services} placeholder="Capture service UUIDs" aria-label="Service UUIDs to reach" autocapitalize="off" autocomplete="off" spellcheck="false" />
  <pre class="log" aria-label="Instrument log">{survey.frames.join('\n') || 'No frames yet.'}</pre>
</div>

{#if survey.calibration}
  <div class="calib" role="dialog" aria-modal="true" aria-label="Guided calibration"><CalibrationGuide guide={survey.calibration} /></div>
{/if}
{#if quickRef}<QuickRef page="cavway-x1" onclose={() => (quickRef = false)} />{/if}

<style>
  .status { min-height: var(--p-row); display: flex; align-items: center; padding: 0 10px; border: 1px solid var(--line); border-radius: 999px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; box-sizing: border-box; }
  .status.on { border-color: var(--accent); color: var(--text); }
  .status.warn { border-color: var(--warn); color: var(--warn); }
  .log { flex: 1 1 0; min-height: 48px; overflow: auto; margin: 0; font-size: 11px; background: var(--panel-2); padding: var(--p-pad); border-radius: var(--p-radius); white-space: pre-wrap; word-break: break-all; -webkit-user-select: text; user-select: text; touch-action: pan-y; }
  /* the calibration guide over the whole phone screen, until its Close guide */
  .calib { position: fixed; inset: 0; z-index: 60; background: var(--panel); padding: calc(env(safe-area-inset-top) + var(--p-pad)) var(--p-pad) calc(env(safe-area-inset-bottom) + var(--p-pad)); box-sizing: border-box; overflow: auto; }
  .calib :global(.guide) { border: 0; padding: 0; gap: var(--p-gap); }
  .calib :global(.circles) { max-width: none; }
  .calib :global(.row button) { min-height: var(--p-row); }
</style>
