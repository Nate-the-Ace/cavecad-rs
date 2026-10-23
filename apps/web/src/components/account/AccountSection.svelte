<script lang="ts">
  // Account: who you are signed in as, the name and colour others see in live sessions, and Sign out.
  import { account, saveProfile, signOut } from '../../lib/account.svelte';
  import { COLORS } from '../../lib/sync.svelte';
  import { flush } from '../../lib/shelf.svelte';

  const m = $derived(account.member);
  const by = $derived(m?.provider === 'google' ? 'Google' : m?.provider === 'microsoft' ? 'Microsoft' : '');
  let error = $state('');
  async function save(p: { displayName?: string; colour?: string }) {
    error = '';
    try {
      await saveProfile(p);
    } catch {
      error = "Couldn't save that. Check your connection and try again.";
    }
  }
</script>

{#if m}
  <div class="pane">
    <div class="row">
      <span class="lab">Signed in</span>
      <span class="val">{by ? `with ${by} · ` : ''}{m.email}</span>
      <button class="btn" onclick={() => flush().catch(() => {}).then(signOut)}>Sign out</button>
    </div>
    <label class="row"><span class="lab">Display name</span>
      <input class="field" value={m.displayName} maxlength="40" onchange={(e) => save({ displayName: e.currentTarget.value })} />
    </label>
    <div class="row"><span class="lab">Colour</span>
      <div class="swatches" role="radiogroup" aria-label="Your colour">
        {#each COLORS as c}
          <button role="radio" aria-checked={m.colour === c} aria-label={c} class="sw" class:on={m.colour === c} style="--c:{c}" onclick={() => save({ colour: c })}></button>
        {/each}
      </div>
    </div>
    <div class="row"><span class="lab">Role</span><span class="val">{m.role === 'admin' ? 'Admin' : 'Member'}</span></div>
    {#if account.phase === 'offline' || account.phase === 'offline-expired'}<p class="hint">Offline: changes here wait until you're back online.</p>{/if}
    {#if error}<p class="hint bad">{error}</p>{/if}
  </div>
{/if}

<style>
  .pane { display: flex; flex-direction: column; gap: 10px; padding: 12px 14px 14px; }
  .row { display: flex; align-items: center; gap: 8px; }
  .lab { min-width: 92px; color: var(--muted); }
  .val { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .field { flex: 1; min-width: 0; font: inherit; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 5px 8px; }
  .swatches { display: flex; gap: 6px; flex-wrap: wrap; }
  .sw { width: 20px; height: 20px; border-radius: 50%; background: var(--c); border: 0; cursor: pointer; }
  .sw.on { outline: 2px solid var(--text-strong); outline-offset: 2px; }
  .btn { display: inline-flex; align-items: center; gap: 6px; padding: 5px 10px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); cursor: pointer; font: inherit; font-size: 12px; }
  .btn:hover { background: var(--accent-bg); }
  .hint { margin: 0; color: var(--muted); font-size: 12px; }
  .bad { color: var(--danger); }
</style>
