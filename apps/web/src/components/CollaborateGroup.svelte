<script lang="ts">
  // Live editing with other devices, straight between them (no server): the button opens the Share live
  // window, where invite and reply codes are swapped. Joining a session is on the Cave Shelf.
  import { Users, LogOut } from '@lucide/svelte';
  import { leaveP2p, p2p } from '../lib/p2p.svelte';
  import { leave, live } from '../lib/sync.svelte';

  const on = $derived(live.status !== 'off');
  const stop = () => {
    leaveP2p();
    leave();
  };
</script>

<div class="collab">
  <div class="row">
    <button class="tool" title="Share this cave live with other devices: one link, no server in between" onclick={() => (p2p.dialog = true)}>
      <Users size={20} strokeWidth={1.6} />
      <span>{p2p.role === 'host' ? 'Sharing' : 'Share live…'}</span>
    </button>
    {#if on}
      <button class="tool" title="Stop sharing edits (the cave stays open here)" onclick={stop}>
        <LogOut size={20} strokeWidth={1.6} />
        <span>Leave</span>
      </button>
    {/if}
  </div>
  <div class="status" role="status">
    {#if live.status === 'live'}{p2p.role === 'guest' ? 'In a session' : 'Sharing'} · {live.people.length} {live.people.length === 1 ? 'person' : 'people'}
    {:else if live.status === 'connecting'}Connecting…
    {:else if p2p.error}{p2p.error}
    {:else}Not shared{/if}
  </div>
</div>

<style>
  .collab { display: flex; flex-direction: column; justify-content: center; gap: 4px; height: 100%; width: 200px; }
  .row { display: flex; align-items: center; gap: 6px; }
  .tool { display: flex; align-items: center; gap: 4px; height: 26px; box-sizing: border-box; padding: 0 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .status { font-size: 11px; color: var(--muted); height: 14px; line-height: 14px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
</style>
