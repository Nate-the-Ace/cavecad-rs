<script lang="ts">
  // Drives: every drive this member has connected, on every device. A row says whether this device can use
  // it yet; Disconnect stops CaveCAD using it everywhere and leaves the caves in the drive untouched.
  import { account, disconnectDrive, recordDrive } from '../../lib/account.svelte';
  import type { Drive } from '../../lib/account-api';

  const NAMES: Record<Drive['provider'], string> = { gdrive: 'Google Drive', onedrive: 'OneDrive', dropbox: 'Dropbox', webdav: 'WebDAV' };
  let asking = $state<string | null>(null);
  let adding = $state(false);
  let label = $state('');
  let error = $state('');
  // Piece 2 (drive connectors) answers this per device; until then no device holds a key.
  const connectedHere = (_d: Drive) => false;
  // How many caves on the shelf come from this drive: piece 2 (drive connectors) fills this in.
  const cavesFrom = (_d: Drive) => 0;

  async function run(f: () => Promise<void>) {
    error = '';
    try {
      await f();
    } catch {
      error = "Couldn't do that. Check your connection and try again.";
    }
  }
</script>

<div class="pane">
  {#if account.drives.length === 0}
    <p class="hint">Connect the drive where you keep your cave folders: Google Drive, OneDrive, Dropbox or a WebDAV server.</p>
  {/if}
  <p class="hint">Coming soon: opening caves from a drive. For now this only remembers which drives you use.</p>
  {#if account.drives.length > 0}
    <ul class="list">
      {#each account.drives as d (d.id)}
        <li class:warn={asking === d.id}>
          {#if asking === d.id}
            <div class="ask">
              <strong>Disconnect {NAMES[d.provider]}{d.accountLabel ? ` (${d.accountLabel})` : ''}?</strong>
              <span>CaveCAD stops using it on every device. Your caves stay in your {NAMES[d.provider]}, untouched.{#if cavesFrom(d) > 0} {cavesFrom(d)} {cavesFrom(d) === 1 ? 'cave on your shelf comes' : 'caves on your shelf come'} from it.{/if}</span>
              <div class="row end">
                <button class="btn" onclick={() => (asking = null)}>Keep it</button>
                <button class="btn danger" onclick={() => run(async () => { await disconnectDrive(d.id); asking = null; })}>Disconnect</button>
              </div>
            </div>
          {:else}
            <div class="who"><span>{NAMES[d.provider]}</span><span class="sub">{d.accountLabel}</span></div>
            {#if connectedHere(d)}<span class="badge">Connected here</span>{:else}<button class="btn" disabled title="Coming soon: drive connections">Reconnect</button>{/if}
            <button class="btn" onclick={() => (asking = d.id)}>Disconnect</button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if adding}
    <div class="add">
      <input class="field" bind:value={label} placeholder="you@example.com" aria-label="Which account on that drive" />
      <div class="row">
        {#each Object.entries(NAMES) as [id, name]}
          <button class="btn" onclick={() => run(async () => { await recordDrive(id as Drive['provider'], label.trim()); adding = false; label = ''; })}>{name}</button>
        {/each}
      </div>
      <button class="btn" onclick={() => (adding = false)}>Cancel</button>
    </div>
  {:else}
    <div><button class="btn" onclick={() => (adding = true)}>+ Connect a drive</button></div>
  {/if}
  {#if error}<p class="hint bad">{error}</p>{/if}
</div>

<style>
  .pane { display: flex; flex-direction: column; gap: 10px; padding: 12px 14px 14px; }
  .list { list-style: none; margin: 0; padding: 0; border: 1px solid var(--line); border-radius: var(--radius); }
  .list li { display: flex; align-items: center; gap: 8px; padding: 8px 10px; }
  .list li + li { border-top: 1px solid var(--line); }
  .list li.warn { background: var(--accent-bg); }
  .who { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .sub { color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; }
  .ask { display: flex; flex-direction: column; gap: 6px; flex: 1; }
  .row { display: flex; gap: 6px; flex-wrap: wrap; }
  .end { justify-content: flex-end; }
  .add { display: flex; flex-direction: column; gap: 8px; }
  .field { font: inherit; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 5px 8px; }
  .badge { font-size: 12px; padding: 2px 8px; border-radius: var(--radius); background: var(--accent-bg); }
  .btn { display: inline-flex; align-items: center; gap: 6px; padding: 5px 10px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); cursor: pointer; font: inherit; font-size: 12px; }
  .btn:hover { background: var(--accent-bg); }
  .btn.danger { color: var(--danger); border-color: var(--danger); }
  .hint { margin: 0; color: var(--muted); font-size: 12px; }
  .bad { color: var(--danger); }
</style>
