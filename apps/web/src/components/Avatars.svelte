<script lang="ts">
  // Who is in the room, as round avatars at the right end of the tab bar. This device is first and
  // ringed; hovering one shows the name. Nothing shows when nobody is sharing.
  import { live } from '../lib/sync.svelte';

  const initials = (name: string) => {
    const w = name.trim().split(/\s+/).filter(Boolean);
    return ((w[0]?.[0] ?? '?') + (w.length > 1 ? w[w.length - 1][0] : '')).toUpperCase();
  };
  const people = $derived([...live.people].sort((a, b) => (a.id === live.me ? -1 : b.id === live.me ? 1 : 0)));
</script>

{#if live.status === 'live' && people.length}
  <div class="avatars" role="list" aria-label="People editing this cave">
    {#each people as p (p.id)}
      <span class="avatar" class:me={p.id === live.me} role="listitem" style="--c:{p.color}" title={p.id === live.me ? `${p.name} (you)` : p.name}>{initials(p.name)}</span>
    {/each}
  </div>
{/if}

<style>
  .avatars { display: flex; align-items: center; margin-left: auto; padding-right: 4px; align-self: center; }
  .avatar { width: 26px; height: 26px; box-sizing: border-box; margin-left: -6px; display: grid; place-items: center; border-radius: 50%; background: var(--c); color: #fff; font-size: 11px; font-weight: 600; border: 2px solid var(--panel); cursor: default; user-select: none; }
  .avatar.me { box-shadow: 0 0 0 2px var(--text-strong); position: relative; z-index: 1; }
</style>
