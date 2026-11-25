<script lang="ts">
  // User window > Options > Display (the dock layout: desktop, web, iPad): dark, light or follow the system and the
  // highlight colour (lib/theme.ts, the same choice the phone's Settings > Display makes), the units (a mirror of the
  // View tab's Units group: the same setUnits), the compact ribbon and resetting the panel layout. Dock-sized rows of
  // its own; the phone's `.phone .set` rules are not used here.
  import { Moon, Sun, MonitorCog } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { PRESETS, choose, currentChoice, load, onTheme, type Choice, type Mode } from '../lib/theme';
  import { cave, setCompactRibbon, setUnits } from '../lib/cave.svelte';
  import { resetLayout } from '../lib/workspace.svelte';

  let c = $state<Choice>(load());
  onMount(() => onTheme(() => (c = { ...currentChoice() })));
  const set = (patch: Partial<Choice>) => choose({ ...c, ...patch });
  const modes: { id: Mode; label: string; icon: typeof Moon }[] = [
    { id: 'dark', label: 'Dark', icon: Moon },
    { id: 'light', label: 'Light', icon: Sun },
    { id: 'auto', label: 'Auto', icon: MonitorCog },
  ];
</script>

<div class="dset">
  <div class="row"><span class="lab">Theme</span>
    <div class="seg" role="group" aria-label="Dark or light">
      {#each modes as m (m.id)}
        <button class:on={c.mode === m.id} aria-pressed={c.mode === m.id} title={m.id === 'auto' ? 'Follow the system' : m.label} onclick={() => set({ mode: m.id })}><m.icon size={14} strokeWidth={1.7} /> {m.label}</button>
      {/each}
    </div>
  </div>
  <div class="row"><span class="lab">Colour</span>
    <div class="swatches" role="group" aria-label="Colour">
      {#each PRESETS as p (p.id)}
        <button class="sw" class:on={c.id === p.id} style="--sw:{p.accent}" title={p.name} aria-label={p.name} aria-pressed={c.id === p.id} onclick={() => set({ id: p.id })}></button>
      {/each}
      <label class="sw custom" class:on={c.id === 'custom'} style="--sw:{c.custom}" title="Your own colour">
        <input type="color" value={c.custom} aria-label="Custom colour" oninput={(e) => set({ id: 'custom', custom: e.currentTarget.value })} />
      </label>
    </div>
  </div>
  <div class="row"><span class="lab">Lengths</span>
    <div class="seg" role="group" aria-label="Units">
      <button class:on={cave.units === 'metric'} aria-pressed={cave.units === 'metric'} onclick={() => setUnits('metric')}>Metric</button>
      <button class:on={cave.units === 'imperial'} aria-pressed={cave.units === 'imperial'} onclick={() => setUnits('imperial')}>Imperial</button>
    </div>
  </div>
  <label class="row check"><input type="checkbox" checked={cave.compactRibbon} onchange={(e) => setCompactRibbon(e.currentTarget.checked)} /> Compact ribbon (every group folds into a dropdown)</label>
  <div class="row"><button class="btn" onclick={resetLayout}>Reset the panel layout</button></div>
</div>

<style>
  .dset { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  .row { display: flex; align-items: center; gap: 8px; min-height: 30px; }
  .lab { min-width: 60px; color: var(--muted); }
  .check { gap: 8px; font-size: 13px; cursor: pointer; }
  .seg { display: flex; }
  .seg button { display: inline-flex; align-items: center; gap: 5px; height: 28px; padding: 0 12px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); cursor: pointer; font: inherit; font-size: 12px; }
  .seg button + button { border-left: 0; }
  .seg button:first-child { border-radius: var(--radius) 0 0 var(--radius); }
  .seg button:last-child { border-radius: 0 var(--radius) var(--radius) 0; }
  .seg button.on { background: var(--accent-bg); outline: 1px solid var(--accent); position: relative; }
  .swatches { display: flex; flex-wrap: wrap; gap: 6px; }
  .sw { width: 22px; height: 22px; padding: 0; border-radius: 50%; border: 2px solid transparent; background: var(--sw); cursor: pointer; box-shadow: 0 0 0 1px var(--line); position: relative; overflow: hidden; display: block; box-sizing: border-box; }
  .sw.on { border-color: var(--text-strong); }
  .custom { background: conic-gradient(from 0deg, #e5484d, #f0a020, #2fb86a, #188cff, #8e6cef, #e5484d); }
  .custom.on { background: var(--sw); }
  .custom input { position: absolute; inset: 0; opacity: 0; width: 100%; height: 100%; cursor: pointer; }
  .btn { display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 10px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); cursor: pointer; font: inherit; font-size: 12px; }
  .btn:hover { background: var(--accent-bg); }
  button:focus-visible, .sw:focus-within { outline: 2px solid var(--accent); outline-offset: 1px; }
</style>
