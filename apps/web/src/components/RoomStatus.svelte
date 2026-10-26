<script lang="ts">
  // A small label beside the user icon saying whether the open cave is in step with the server.
  import { room } from '../lib/room.svelte';
  const TEXT: Record<string, string> = {
    connecting: 'Connecting…',
    synced: 'Synced',
    offline: 'Offline: your edits will join when you reconnect',
    'waiting-key': 'Waiting for your other device to share this cave',
    error: "Sync stopped: something didn't check out",
  };
</script>

{#if room.info}
  <span class="room-status" role="status">{room.info}</span>
{:else if room.status}
  <span class="room-status" class:quiet={room.status === 'synced'} role="status" title={room.reason || undefined}>{TEXT[room.status]}</span>
{/if}

<style>
  .room-status { align-self: center; font-size: 12px; color: var(--muted); padding: 0 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 320px; }
  .room-status:not(.quiet) { color: var(--text); }
</style>
