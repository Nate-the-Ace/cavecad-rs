<script lang="ts">
  // Live editing with other devices: type a room name, press Join, and everyone in the same room
  // sees each other's edits as they happen.
  import { Users, LogOut } from '@lucide/svelte';
  import { join, leave, live, me, rename } from '../lib/sync.svelte';

  let room = $state('pitfall');
  const on = $derived(live.status !== 'off');
</script>

<div class="collab">
  <div class="row">
    <input class="room" bind:value={room} disabled={on} aria-label="Room name" placeholder="Room name" onkeydown={(e) => e.key === 'Enter' && !on && join(room)} />
    {#if on}
      <button class="tool" title="Stop sharing edits (the cave stays open here)" onclick={leave}>
        <LogOut size={20} strokeWidth={1.6} />
        <span>Leave</span>
      </button>
    {:else}
      <button class="tool" title="Join this room: you will see each other's edits live" onclick={() => join(room)}>
        <Users size={20} strokeWidth={1.6} />
        <span>Join</span>
      </button>
    {/if}
  </div>
  {#if !on}
    <input class="room" value={me.name} aria-label="Your name" placeholder="Your name" onchange={(e) => rename(e.currentTarget.value)} />
  {/if}
  <div class="status" role="status" class:bad={!!live.error && !on}>
    {#if live.status === 'live'}Live in {live.room} · {live.people.length} {live.people.length === 1 ? 'person' : 'people'}
    {:else if live.status === 'reconnecting'}Reconnecting… your edits are kept
    {:else if live.status === 'connecting'}Connecting…
    {:else if live.error}{live.error}
    {:else}Not shared{/if}
  </div>
</div>

<style>
  .collab { display: flex; flex-direction: column; justify-content: center; gap: 4px; height: 100%; width: 200px; }
  .row { display: flex; align-items: center; gap: 6px; }
  .room { flex: 1; min-width: 0; height: 26px; box-sizing: border-box; padding: 0 8px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); font: inherit; font-size: 13px; }
  .tool { display: flex; align-items: center; gap: 4px; height: 26px; box-sizing: border-box; padding: 0 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .status:not(.bad):empty { display: none; }
  .status { font-size: 11px; color: var(--muted); height: 14px; line-height: 14px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .status.bad { color: var(--danger); }
</style>
