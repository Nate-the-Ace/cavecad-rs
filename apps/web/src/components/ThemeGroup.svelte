<script lang="ts">
  // The theme chooser: dark, light or follow the system, and one of ten colours (or your own).
  import { Moon, Sun, MonitorCog } from '@lucide/svelte';
  import { PRESETS, choose, currentChoice, load, onTheme, type Choice, type Mode } from '../lib/theme';
  import { onMount } from 'svelte';

  let c = $state<Choice>(load());
  onMount(() => onTheme(() => (c = { ...currentChoice() })));

  const set = (patch: Partial<Choice>) => choose({ ...c, ...patch });
  const modes: { id: Mode; label: string; icon: typeof Moon }[] = [
    { id: 'dark', label: 'Dark', icon: Moon },
    { id: 'light', label: 'Light', icon: Sun },
    { id: 'auto', label: 'Auto', icon: MonitorCog },
  ];
</script>

<div class="theme">
  <div class="modes" role="group" aria-label="Dark or light">
    {#each modes as m}
      <button class="tool" class:pressed={c.mode === m.id} aria-pressed={c.mode === m.id} title={m.id === 'auto' ? 'Follow the system' : m.label} onclick={() => set({ mode: m.id })}>
        <m.icon size={22} strokeWidth={1.6} />
        <span>{m.label}</span>
      </button>
    {/each}
  </div>
  <div class="swatches" role="group" aria-label="Colour">
    {#each PRESETS as p}
      <button class="swatch" class:on={c.id === p.id} style="--sw:{p.accent}" title={p.name} aria-label={p.name} aria-pressed={c.id === p.id} onclick={() => set({ id: p.id })}></button>
    {/each}
    <label class="swatch custom" class:on={c.id === 'custom'} style="--sw:{c.custom}" title="Your own colour">
      <input type="color" value={c.custom} aria-label="Custom colour" oninput={(e) => set({ id: 'custom', custom: e.currentTarget.value })} />
    </label>
  </div>
</div>

<style>
  .theme { display: flex; align-items: center; gap: 14px; }
  .modes { display: flex; gap: 4px; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; padding: 6px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .tool.pressed { background: var(--accent-bg); }
  .swatches { display: grid; grid-template-columns: repeat(6, 22px); gap: 6px; }
  .swatch { width: 22px; height: 22px; padding: 0; border-radius: 50%; border: 2px solid transparent; background: var(--sw); cursor: pointer; box-shadow: 0 0 0 1px var(--line); position: relative; overflow: hidden; display: block; }
  .swatch.on { border-color: var(--text-strong); }
  .custom { background: conic-gradient(from 0deg, #e5484d, #f0a020, #2fb86a, #188cff, #8e6cef, #e5484d); }
  .custom.on { background: var(--sw); }
  .custom input { position: absolute; inset: 0; opacity: 0; width: 100%; height: 100%; cursor: pointer; }
</style>
