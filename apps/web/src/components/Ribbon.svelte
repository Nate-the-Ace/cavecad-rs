<script lang="ts">
  import { engine } from '../lib/engine';
  import { FileInput, Route, Waypoints, Pencil, Spline, PaintBucket, PanelTop, Scissors, Box } from '@lucide/svelte';

  const tabs = ['Home', 'Draw', 'Sheets', 'View'];
  let active = $state('Home');

  // Groups follow the port order; every button only logs for now.
  const groups = [
    { name: 'Survey', items: [{ label: 'Import', icon: FileInput }, { label: 'Draw', icon: Route }, { label: 'Adjust', icon: Waypoints }] },
    { name: 'Trace', items: [{ label: 'Feature', icon: Pencil }, { label: 'Shaped', icon: Spline }, { label: 'Fill', icon: PaintBucket }] },
    { name: 'Views', items: [{ label: 'Profile', icon: PanelTop }, { label: 'Section', icon: Scissors }, { label: '3D', icon: Box }] },
  ];

  const press = (label: string) => engine.log('info', 'ribbon', `${label}: not built yet`);
</script>

<header>
  <nav class="tabs">
    {#each tabs as t}
      <button class:on={t === active} onclick={() => (active = t)}>{t}</button>
    {/each}
  </nav>
  <div class="bar">
    {#each groups as g}
      <section>
        <div class="btns">
          {#each g.items as item}
            <button class="tool" onclick={() => press(item.label)}>
              <item.icon size={22} strokeWidth={1.6} />
              <span>{item.label}</span>
            </button>
          {/each}
        </div>
        <div class="gname">{g.name}</div>
      </section>
    {/each}
  </div>
</header>

<style>
  header { background: var(--panel); }
  .tabs { display: flex; gap: 2px; padding: 6px 10px 0; }
  .tabs button { background: none; border: 0; padding: 6px 14px; color: var(--muted); border-radius: var(--radius) var(--radius) 0 0; cursor: pointer; }
  .tabs button.on { color: var(--text); background: var(--accent-bg); font-weight: 500; }
  .bar { display: flex; padding: 8px 10px; border-top: 1px solid var(--line); }
  section { display: flex; flex-direction: column; align-items: center; padding: 0 12px; border-right: 1px solid var(--line); }
  .btns { display: flex; gap: 4px; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; padding: 6px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; }
  .tool:hover { background: var(--panel-2); }
  .tool:active { background: var(--accent-bg); }
  .gname { font-size: 11px; color: var(--muted); margin-top: 4px; }
</style>
