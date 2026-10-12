<script lang="ts">
  import { engine } from '../lib/engine';
  import { openDxf, saveDxf } from '../lib/files';
  import { PANELS, panelState, resetLayout, togglePanel } from '../lib/workspace.svelte';
  import { FileInput, FolderOpen, Save, Route, Waypoints, Pencil, Spline, PaintBucket, PanelTop, Scissors, Box, Map as MapIcon, Terminal, LayoutDashboard, Activity } from '@lucide/svelte';
  import type { Component } from 'svelte';

  interface Item {
    label: string;
    icon: Component<any>;
    run: () => void;
    pressed?: () => boolean;
  }
  interface Group {
    name: string;
    items: Item[];
  }

  const press = (label: string) => engine.log('info', 'ribbon', `${label}: not built yet`);
  const stub = (label: string, icon: Component<any>): Item => ({ label, icon, run: () => press(label) });
  const panelItem = (id: string, icon: Component<any>): Item => ({
    label: PANELS[id].title,
    icon,
    run: () => togglePanel(id),
    pressed: () => !!panelState.open[id],
  });

  /** Says what happened in the console (and as an error when it did not work). */
  const tell = (r: { ok: boolean; message: string } | string | null) => {
    if (r === null) return;
    const ok = typeof r === 'string' || r.ok;
    engine.log(ok ? 'info' : 'error', 'file', typeof r === 'string' ? r : r.message);
  };
  const run = (job: Promise<{ ok: boolean; message: string } | string | null>) => job.then(tell, (e) => engine.log('error', 'file', String(e)));
  const open = () => run(openDxf());
  const save = () => run(saveDxf());
  const saveAs = () => run(saveDxf(true));

  const tabs: Record<string, Group[]> = {
    Home: [
      { name: 'File', items: [{ label: 'Open', icon: FolderOpen, run: open }, { label: 'Save', icon: Save, run: save }, { label: 'Save as', icon: Save, run: saveAs }] },
      { name: 'Survey', items: [stub('Import', FileInput), stub('Draw', Route), stub('Adjust', Waypoints)] },
      { name: 'Trace', items: [stub('Feature', Pencil), stub('Shaped', Spline), stub('Fill', PaintBucket)] },
      { name: 'Views', items: [stub('Profile', PanelTop), stub('Section', Scissors), stub('3D', Box)] },
    ],
    Draw: [],
    Sheets: [],
    View: [
      { name: 'Panels', items: [panelItem('map', MapIcon), panelItem('profile', Activity), panelItem('console', Terminal)] },
      { name: 'Layout', items: [{ label: 'Reset', icon: LayoutDashboard, run: resetLayout }] },
    ],
  };
  let active = $state('Home');

  function keys(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    const k = e.key.toLowerCase();
    if (k === 'o') (e.preventDefault(), open());
    else if (k === 's') (e.preventDefault(), e.shiftKey ? saveAs() : save());
  }
</script>

<svelte:window onkeydown={keys} />

<header>
  <nav class="tabs">
    {#each Object.keys(tabs) as t}
      <button class:on={t === active} onclick={() => (active = t)}>{t}</button>
    {/each}
  </nav>
  <div class="bar">
    {#each tabs[active] as g}
      <section>
        <div class="btns">
          {#each g.items as item}
            <button class="tool" class:pressed={item.pressed?.()} onclick={item.run}>
              <item.icon size={22} strokeWidth={1.6} />
              <span>{item.label}</span>
            </button>
          {/each}
        </div>
        <div class="gname">{g.name}</div>
      </section>
    {:else}
      <p class="empty">Nothing here yet.</p>
    {/each}
  </div>
</header>

<style>
  header { background: var(--panel); }
  .tabs { display: flex; gap: 2px; padding: 6px 10px 0; }
  .tabs button { background: none; border: 0; padding: 6px 14px; color: var(--muted); border-radius: var(--radius) var(--radius) 0 0; cursor: pointer; }
  .tabs button.on { color: var(--text); background: var(--accent-bg); font-weight: 500; }
  .bar { display: flex; padding: 8px 10px; border-top: 1px solid var(--line); min-height: 76px; }
  section { display: flex; flex-direction: column; align-items: center; padding: 0 12px; border-right: 1px solid var(--line); }
  .btns { display: flex; gap: 4px; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; padding: 6px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; }
  .tool:hover { background: var(--panel-2); }
  .tool:active, .tool.pressed { background: var(--accent-bg); }
  .gname { font-size: 11px; color: var(--muted); margin-top: 4px; }
  .empty { color: var(--muted); margin: 0; align-self: center; }
</style>
