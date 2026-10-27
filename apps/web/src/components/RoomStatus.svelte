<script lang="ts">
  // A small label beside the user icon saying whether the open cave is in step with the server.
  import { ROOM_TEXT, room } from '../lib/room.svelte';
</script>

{#if room.info}
  <span class="room-status" role="status">{room.info}</span>
{:else if room.status}
  <span class="room-status" class:quiet={room.status === 'synced'} role="status" title={room.reason || undefined}>{ROOM_TEXT[room.status]}</span>
{/if}

<style>
  .room-status { align-self: center; font-size: 12px; color: var(--muted); padding: 0 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 320px; }
  .room-status:not(.quiet) { color: var(--text); }
</style>
