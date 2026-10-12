<script lang="ts">
  import { engine, loadEngine } from '../lib/engine';
  import { openDxf, saveDxf } from '../lib/files';
  import { cave, setUnits } from '../lib/cave.svelte';
  import DrawTools from './DrawTools.svelte';
  import ThemeGroup from './ThemeGroup.svelte';
  import ContextOptions from './ContextOptions.svelte';
  import SelectionTools from './SelectionTools.svelte';
  import CollaborateGroup from './CollaborateGroup.svelte';
  import FillTools from './FillTools.svelte';
  import Avatars from './Avatars.svelte';
  import { symbolDef } from '../lib/symbols';
  import { fillLabel } from '../lib/fills';
  import { onMount } from 'svelte';
  import { PANELS, panelState, resetLayout, showPanel, togglePanel } from '../lib/workspace.svelte';
  import { Check, Trash2, FileInput, FolderOpen, Save, Ruler, Footprints, Route, Waypoints, Scissors, Map as MapIcon, Terminal, LayoutDashboard, Activity, Shapes, Table2 } from '@lucide/svelte';
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
    /** A group that draws itself instead of a row of buttons. */
    custom?: Component<any>;
    /** What the custom component is given. */
    props?: Record<string, unknown>;
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
    ],
    Draw: [
      { name: 'Linework', items: [], custom: DrawTools, props: { group: 'Linework' } },
      { name: 'Passage lines', items: [], custom: DrawTools, props: { group: 'Passage lines' } },
      { name: 'Geology and relief', items: [], custom: DrawTools, props: { group: 'Geology and relief' } },
      { name: 'Symbols', items: [{ label: 'Symbols', icon: Shapes, run: () => showPanel('symbols'), pressed: () => !!panelState.visible.symbols }] },
      { name: 'Area fill', items: [], custom: FillTools },
    ],
    Sheets: [],
    View: [
      { name: 'Panels', items: [panelItem('map', MapIcon), panelItem('profile', Activity), panelItem('section', Scissors), panelItem('symbols', Shapes), panelItem('stations', Table2), panelItem('console', Terminal)] },
      { name: 'Units', items: [{ label: 'Metric', icon: Ruler, run: () => setUnits('metric'), pressed: () => cave.units === 'metric' }, { label: 'Imperial', icon: Footprints, run: () => setUnits('imperial'), pressed: () => cave.units === 'imperial' }] },
      { name: 'Theme', items: [], custom: ThemeGroup },
      { name: 'Live editing', items: [], custom: CollaborateGroup },
      { name: 'Layout', items: [{ label: 'Reset', icon: LayoutDashboard, run: resetLayout }] },
    ],
  };
  let active = $state('Home');

  // A running command gets its own tab, in front, with its settings and a way to finish it.
  const CONTEXT = '\u0000command';
  let toolLabels = $state<Record<string, string>>({});
  onMount(() => {
    loadEngine().then(() => (toolLabels = Object.fromEntries(engine.drawTools().map((t) => [t.key, t.label]))));
  });
  // A selected line gets one too, with what can be done to it, so it is easy to adjust.
  let loaded = $state(false);
  onMount(() => {
    loadEngine().then(() => (loaded = true));
  });
  const selection = $derived.by(() => {
    void cave.version;
    return loaded && cave.selected && !cave.tool ? engine.lineInfo(cave.selected) : null;
  });
  $effect(() => {
    // The selected line is gone (undone, deleted elsewhere): nothing is selected any more.
    if (loaded && cave.selected && !cave.tool && selection === null) cave.selected = '';
  });
  const mode = $derived(cave.tool ? 'command' : selection ? 'selection' : '');
  const contextName = $derived(
    mode === 'command' ? (toolLabels[cave.tool] ?? symbolDef(cave.tool)?.name ?? fillLabel(cave.tool) ?? cave.tool) : mode === 'selection' ? `Edit ${toolLabels[selection!.kind] ?? symbolDef(selection!.kind)?.name ?? fillLabel(selection!.kind) ?? selection!.kind}` : '',
  );
  const commandGroups: Group[] = [
    { name: 'Settings', items: [], custom: ContextOptions },
    { name: 'Command', items: [{ label: 'Done', icon: Check, run: () => (cave.tool = '') }] },
  ];
  const selectionGroups: Group[] = [
    { name: 'Line', items: [], custom: SelectionTools },
    {
      name: 'Command',
      items: [
        {
          label: 'Delete',
          icon: Trash2,
          run: () => {
            const id = cave.selected;
            cave.selected = '';
            if (id) engine.deleteLine(id);
            cave.version += 1;
          },
        },
        { label: 'Done', icon: Check, run: () => (cave.selected = '') },
      ],
    },
  ];
  let before = 'Home';
  $effect(() => {
    if (mode) {
      if (active !== CONTEXT) before = active;
      active = CONTEXT;
    } else if (active === CONTEXT) {
      active = before;
    }
  });
  const shownGroups = $derived(active === CONTEXT ? (mode === 'selection' ? selectionGroups : commandGroups) : (tabs[active] ?? []));

  function keys(e: KeyboardEvent) {
    if (e.key === 'Escape' && (cave.tool || cave.selected)) {
      if (cave.tool) cave.tool = '';
      else cave.selected = '';
      return;
    }
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
    {#if mode}
      <button class="ctx" class:on={active === CONTEXT} onclick={() => (active = CONTEXT)}>{contextName}</button>
    {/if}
    <Avatars />
  </nav>
  <div class="bar">
    {#each shownGroups as g}
      <section>
        <div class="btns">
          {#if g.custom}<g.custom {...(g.props ?? {})} />{/if}
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
  .tabs { display: flex; gap: 2px; padding: 6px 10px 0; align-items: flex-end; }
  .tabs button { background: none; border: 0; padding: 6px 14px; color: var(--muted); border-radius: var(--radius) var(--radius) 0 0; cursor: pointer; }
  .tabs button.ctx { color: var(--accent); margin-left: 10px; box-shadow: inset 0 2px 0 var(--accent); }
  .tabs button.on { color: var(--text); background: var(--accent-bg); font-weight: 500; }
  .bar { display: flex; box-sizing: border-box; height: var(--ribbon-h); overflow: hidden; padding: 6px 10px; border-top: 1px solid var(--line); }
  section { display: flex; flex-direction: column; align-items: center; height: 100%; padding: 0 12px; border-right: 1px solid var(--line); }
  .btns { display: flex; align-items: center; gap: 4px; height: var(--ribbon-body); overflow: hidden; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; }
  .tool:hover { background: var(--panel-2); }
  .tool:active, .tool.pressed { background: var(--accent-bg); }
  .gname { height: var(--ribbon-name); line-height: var(--ribbon-name); font-size: 11px; color: var(--muted); white-space: nowrap; }
  .empty { color: var(--muted); margin: 0; align-self: center; }
</style>
