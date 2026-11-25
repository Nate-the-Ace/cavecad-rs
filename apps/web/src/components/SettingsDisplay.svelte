<script lang="ts">
  // Settings > Display (phone): dark, light or follow the system, the highlight colour (the same choice as the
  // desktop View tab's ThemeGroup, through lib/theme.ts). The drawing preferences are in Settings > Sketch.
  import { Moon, Sun, MonitorCog } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { PRESETS, choose, currentChoice, load, onTheme, type Choice, type Mode } from '../lib/theme';
  import './settings.css';

  let c = $state<Choice>(load());
  onMount(() => onTheme(() => (c = { ...currentChoice() })));
  const set = (patch: Partial<Choice>) => choose({ ...c, ...patch });
  const modes: { id: Mode; label: string; icon: typeof Moon }[] = [
    { id: 'dark', label: 'Dark', icon: Moon },
    { id: 'light', label: 'Light', icon: Sun },
    { id: 'auto', label: 'Auto', icon: MonitorCog },
  ];
</script>

<div class="set">
  <div class="seg" role="group" aria-label="Dark or light">
    {#each modes as m (m.id)}
      <button class:on={c.mode === m.id} aria-pressed={c.mode === m.id} onclick={() => set({ mode: m.id })}><m.icon size={18} strokeWidth={1.7} /> {m.label}</button>
    {/each}
  </div>
  <div class="swatches" role="group" aria-label="Color">
    {#each PRESETS as p (p.id)}
      <button class="sw" class:sel={c.id === p.id} title={p.name} aria-label={p.name} aria-pressed={c.id === p.id} onclick={() => set({ id: p.id })}><i style="--sw:{p.accent}"></i></button>
    {/each}
    <label class="sw custom" class:sel={c.id === 'custom'} title="Your own color">
      <i style="--sw:{c.custom}" class:wheel={c.id !== 'custom'}></i>
      <input type="color" value={c.custom} aria-label="Custom color" oninput={(e) => set({ id: 'custom', custom: e.currentTarget.value })} />
    </label>
  </div>
</div>

<style>
  .swatches { display: grid; grid-template-columns: repeat(6, 1fr); gap: var(--p-gap); flex: none; }
  .set .sw { position: relative; display: grid; place-items: center; min-height: var(--p-row); padding: 0; border: 2px solid transparent; border-radius: var(--p-radius); background: none; cursor: pointer; }
  .set .sw.sel { border-color: var(--text-strong, var(--text)); }
  .sw i { width: 26px; height: 26px; border-radius: 50%; background: var(--sw); box-shadow: 0 0 0 1px var(--line); display: block; }
  .sw i.wheel { background: conic-gradient(from 0deg, #e5484d, #f0a020, #2fb86a, #188cff, #8e6cef, #e5484d); }
  .custom input { position: absolute; inset: 0; opacity: 0; width: 100%; height: 100%; cursor: pointer; }
</style>
