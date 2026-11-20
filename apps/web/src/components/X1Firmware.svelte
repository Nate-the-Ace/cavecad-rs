<script lang="ts">
  // Flash the Cavway X1 from here, over Bluetooth (lib/x1firmware.svelte.ts). The X1 must be in boot mode.
  import { fw, chooseFile, checkBoot, flash } from '../lib/x1firmware.svelte';

  let open = $state(false);
  let confirming = $state(false);
</script>

<details class="x1fw" bind:open>
  <summary>X1 firmware</summary>
  <p class="hint">Flash a Cavway <code>.bin</code> over Bluetooth. Put the X1 in boot mode first: power off, hold both arrow keys, press DIST. Then connect it here. Shots stored on the X1 are erased.</p>
  <label class="pick">Firmware file
    <input type="file" onchange={(e) => { const f = e.currentTarget.files?.[0]; if (f) void chooseFile(f); }} disabled={fw.running} />
  </label>
  {#if fw.name}
    <p><strong>{fw.version}</strong> · {fw.name} · {fw.packets} packets</p>
    {#each fw.warnings as w}<p class="hint warn">{w}</p>{/each}
  {/if}
  <div class="row">
    <button onclick={() => void checkBoot()} disabled={fw.running || fw.checking}>{fw.checking ? 'Checking…' : 'Check boot mode'}</button>
    {#if fw.boot !== null}
      <span class="hint" role="status">{fw.boot >= 0 ? `Boot mode: ready (bootloader 0x${fw.boot.toString(16).toUpperCase().padStart(2, '0')})` : 'Not in boot mode (or not connected)'}</span>
    {/if}
  </div>
  {#if fw.running}
    <progress max={Math.max(fw.total, 1)} value={fw.done}></progress>
    <p role="status">{fw.phase}{fw.resends ? ` · ${fw.resends} resent` : ''}</p>
    <p class="hint warn">Keep the X1 on and this screen open until it says done.</p>
  {:else if confirming}
    <p><strong>Replace the X1's firmware with {fw.version}?</strong> Stored shots are erased. If anything goes wrong the bootloader stays intact and you can flash again.</p>
    <div class="row">
      <button onclick={() => (confirming = false)}>Cancel</button>
      <button class="go" onclick={() => { confirming = false; void flash(); }}>Flash</button>
    </div>
  {:else}
    <button class="go" onclick={() => (confirming = true)} disabled={!fw.name}>Flash X1…</button>
  {/if}
  {#if fw.result === 'ok'}<p class="ok" role="status">{fw.message}</p>{/if}
  {#if fw.result === 'failed'}<p class="hint warn" role="alert">{fw.message}</p>{/if}
  {#if fw.error}<p class="hint warn" role="alert">{fw.error}</p>{/if}
</details>

<style>
  .x1fw { border-top: 1px solid var(--line, #ccc); padding-top: 8px; margin-top: 8px; display: grid; gap: 8px; }
  .x1fw summary { cursor: pointer; font-weight: 600; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .pick { display: grid; gap: 4px; }
  progress { width: 100%; }
  .ok { color: var(--ok, #1f7a4d); }
  .warn { color: var(--bad, #b3261e); }
</style>
