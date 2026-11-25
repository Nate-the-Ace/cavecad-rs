<script lang="ts">
  // Settings > Survey > Hardware keys (the Android app, on a phone with action keys: Sonim, Zebra, Samsung XCover, ...):
  // what each key does, a learn mode that names a key this app does not know yet by pressing it, and keys added by hand
  // (a broadcast the phone sends, or a key code). The names, actions and rules are in lib/hardware-keys.ts.
  import { onDestroy } from 'svelte';
  import {
    hw, setBinding, resetBindings, setLearning, useUnknown, addAction, addKeycode, removeAction, removeKeycode,
  } from '../lib/hardware-keys.svelte';
  import { HW_ACTIONS, HW_KEYS, type HwAction, type HwKey, type HwPhase } from '../lib/hardware-keys';
  import './settings.css';

  let slot = $state<HwKey>('prog1');
  let action = $state('');
  let phase = $state<HwPhase>('down');
  let code = $state('');
  let problem = $state('');

  const label = (id: string) => HW_KEYS.find((k) => k.id === id)?.label ?? id;

  function addBroadcast() {
    problem = addAction(action, slot, phase) ? '' : 'That is not a broadcast name (letters, digits, dots and underscores), or it is in the list already.';
    if (!problem) action = '';
  }
  function addCode() {
    const n = Number(code);
    problem = addKeycode(n, slot) ? '' : 'Give a key code (a whole number above 0) that is not in the list already.';
    if (!problem) code = '';
  }

  onDestroy(() => setLearning(false)); // leaving the screen ends learn mode: keys do their usual jobs again
</script>

{#if hw.native}
  <details class="hwk">
    <summary>Hardware keys</summary>
    <div class="set-in">
      <p class="line">{hw.native.manufacturer} {hw.native.model}{hw.last ? ` · last key: ${hw.last}` : ''}</p>
      {#each HW_KEYS as k (k.id)}
        <div class="row">
          <span class="lab">{k.label}</span>
          <select aria-label="What {k.label} does" value={hw.bindings[k.id]} onchange={(e) => setBinding(k.id, e.currentTarget.value as HwAction)}>
            {#each HW_ACTIONS as a (a.id)}<option value={a.id}>{a.label}</option>{/each}
          </select>
        </div>
      {/each}
      <p class="line">Keys act only while an instrument is connected.</p>
      <button onclick={resetBindings}>Reset to the defaults</button>

      <p class="line">A key that does nothing: press it here to find it.</p>
      <button class:on={hw.learning} aria-pressed={hw.learning} onclick={() => setLearning(!hw.learning)}>{hw.learning ? 'Stop learning keys' : 'Learn a key'}</button>
      {#if hw.learning}
        <p class="line">{hw.unknown ? `Pressed: key code ${hw.unknown.code}${hw.unknown.scan ? ` (scan ${hw.unknown.scan})` : ''}` : 'Press the key on the phone.'}</p>
        {#if hw.unknown}
          <div class="row">
            <select aria-label="Use this key as" bind:value={slot}>{#each HW_KEYS as k (k.id)}<option value={k.id}>{k.label}</option>{/each}</select>
            <button onclick={() => useUnknown(slot)}>Use it as this key</button>
          </div>
        {/if}
      {/if}

      <p class="line">Add a key by hand</p>
      <div class="row">
        <select aria-label="Which key" bind:value={slot}>{#each HW_KEYS as k (k.id)}<option value={k.id}>{k.label}</option>{/each}</select>
      </div>
      <div class="row">
        <input type="text" placeholder="Broadcast, e.g. com.vendor.PTT_DOWN" autocapitalize="off" autocomplete="off" spellcheck="false" bind:value={action} aria-label="Broadcast name" />
      </div>
      <div class="row">
        <div class="seg" role="group" aria-label="Press or release">
          <button class:on={phase === 'down'} aria-pressed={phase === 'down'} onclick={() => (phase = 'down')}>Press</button>
          <button class:on={phase === 'up'} aria-pressed={phase === 'up'} onclick={() => (phase = 'up')}>Release</button>
        </div>
        <button onclick={addBroadcast} disabled={!action.trim()}>Add broadcast</button>
      </div>
      <div class="row">
        <input type="text" inputmode="numeric" placeholder="Or a key code, e.g. 131" bind:value={code} aria-label="Key code" />
        <button onclick={addCode} disabled={!code.trim()}>Add key code</button>
      </div>
      {#if problem}<p class="line warn" role="alert">{problem}</p>{/if}
      {#each hw.custom.actions as a (a.action)}
        <div class="row"><span class="line" title={a.action}>{label(a.key)} · {a.phase === 'down' ? 'press' : 'release'} · {a.action}</span><button aria-label="Remove {a.action}" onclick={() => removeAction(a.action)}>Remove</button></div>
      {/each}
      {#each hw.custom.keycodes as c (c.code)}
        <div class="row"><span class="line">{label(c.key)} · key code {c.code}</span><button aria-label="Remove key code {c.code}" onclick={() => removeKeycode(c.code)}>Remove</button></div>
      {/each}
    </div>
  </details>
{/if}

<style>
  .set-in { display: flex; flex-direction: column; gap: var(--p-gap); padding: 0 var(--p-pad) var(--p-pad); }
</style>
