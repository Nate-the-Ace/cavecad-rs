<script lang="ts">
  // Where invite links get their first handshake: CaveCAD's class mailbox (an Apps Script). Built into this copy of
  // CaveCAD when it was made with the address and key; otherwise typed in here once and kept on this machine.
  // Used wherever a link can be made (Share live, Start a Classroom).
  import { mailboxConfig, saveMailboxConfig } from '../lib/signal';

  let { onsave }: { onsave?: () => void } = $props();

  const built = mailboxConfig();
  let endpoint = $state(built?.endpoint ?? '');
  let key = $state(built?.key ?? '');
  let kept = $state(false);

  function keep() {
    saveMailboxConfig({ endpoint, key });
    kept = true;
    onsave?.();
  }
</script>

{#if !built || built.source === 'local'}
  <div class="mail">
    <label>Class mailbox address <input bind:value={endpoint} placeholder="https://script.google.com/macros/s/…/exec" oninput={() => (kept = false)} /></label>
    <label>Mailbox key <input bind:value={key} type="password" autocomplete="off" oninput={() => (kept = false)} /></label>
    <div class="row">
      <button type="button" onclick={keep} disabled={!endpoint.trim() || !key.trim()}>Keep on this computer</button>
      {#if kept}<span class="fine">Kept.</span>{/if}
      {#if !built}<span class="fine">This copy was built without a mailbox.</span>{/if}
    </div>
  </div>
{/if}

<style>
  .mail { display: flex; flex-direction: column; gap: 6px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
  input { font: inherit; font-size: 13px; padding: 5px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--field); color: var(--text); }
  .row { display: flex; align-items: center; gap: 8px; }
  .fine { font-size: 11px; color: var(--muted); }
  button { font: inherit; font-size: 12px; padding: 3px 9px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
</style>
