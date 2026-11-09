<script lang="ts">
  // The desktop app's "a new version is ready" bar (lib/desktop-update.svelte.ts). Shown only in the desktop app,
  // only once an update is downloaded and verified, and gone after "Later" until the next start.
  import { restartIntoUpdate, update } from '../lib/desktop-update.svelte';
</script>

{#if update.found && !update.hidden}
  <div class="bar" role="status">
    {#if update.found.installable}
      <span><b>CaveCAD {update.found.version} is ready.</b> {update.problem}</span>
      <button class="go" onclick={() => void restartIntoUpdate()} disabled={update.installing}>{update.installing ? 'Restarting…' : 'Restart now'}</button>
    {:else}
      <span><b>CaveCAD {update.found.version} is out.</b> This copy cannot replace itself: download it from your account menu (your initials, top right), Downloads.</span>
    {/if}
    <button onclick={() => (update.hidden = true)} disabled={update.installing}>Later</button>
  </div>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 12px;
    background: var(--accent-soft, #1d3a5c);
    color: var(--text, #e6edf3);
    font-size: 13px;
    border-bottom: 1px solid var(--border, #30363d);
  }
  .bar span {
    flex: 1;
  }
  button {
    font: inherit;
  }
</style>
