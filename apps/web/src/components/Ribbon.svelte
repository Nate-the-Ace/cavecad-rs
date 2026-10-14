<script lang="ts">
  import { engine, loadEngine } from '../lib/engine';
  import { exportDxf, exportProject, openCave, saveCave } from '../lib/files';
  import { shelf } from '../lib/shelf.svelte';
  import { leaveP2p, p2p } from '../lib/p2p.svelte';
  import { leave, live } from '../lib/sync.svelte';
  import { classroom } from '../lib/classroom.svelte';
  import { registerMirror } from '../lib/classroom-mirror';
  import { cave, clearPick, selectedIds, selectIds, setCompactRibbon, setPick, setUnits, option, setOption } from '../lib/cave.svelte';
  import DrawTools from './DrawTools.svelte';
  import ThemeGroup from './ThemeGroup.svelte';
  import ContextOptions from './ContextOptions.svelte';
  import SelectionTools from './SelectionTools.svelte';
  import CopyIcon from './CopyIcon.svelte';
  import { copyOp, endCopy, startCopy } from '../lib/view-clip.svelte';
  import CollaborateGroup from './CollaborateGroup.svelte';
  import FillTools from './FillTools.svelte';
  import SheetGroup from './SheetGroup.svelte';
  import { INSPECTORS } from '../lib/inspectors';
  import DrawingScale from './DrawingScale.svelte';
  import type { PickKind } from '../lib/cave.svelte';
  import UserMenu from './UserMenu.svelte';
  import { isStampKind, symbolDef } from '../lib/symbols';
  import { fillLabel, isFillKind } from '../lib/fills';
  import { onMount } from 'svelte';
  import { PANELS, panelState, resetLayout, showPanel, togglePanel } from '../lib/workspace.svelte';
  import { GraduationCap, Check, Trash2, LogIn, LogOut, Users, Library, FilePlus, FileInput, FolderOpen, Save, Ruler, Footprints, Route, Waypoints, Scissors, Map as MapIcon, Terminal, LayoutDashboard, Activity, Shapes, Table2, BookOpen, ScanLine, FileText, ChevronDown, PanelTop, FileOutput, Package, Mountain, SlidersHorizontal } from '@lucide/svelte';
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
  const open = () => run(openCave());
  const save = () => run(saveCave());
  const saveAs = () => run(saveCave(true));

  const tabs: Record<string, Group[]> = {
    Home: [
      { name: 'File', items: [{ label: 'Caves', icon: Library, run: () => (shelf.open = true) }, { label: 'New', icon: FilePlus, run: () => (cave.newDialog = true) }, { label: 'Open', icon: FolderOpen, run: open }, { label: 'Save', icon: Save, run: save }, { label: 'Save as', icon: Save, run: saveAs }, { label: 'Scans in file', icon: ScanLine, run: () => setOption('saveScans', !option('saveScans', true)), pressed: () => option('saveScans', true) }] },
      { name: 'Export', items: [{ label: 'DXF', icon: FileOutput, run: () => run(exportDxf()) }, { label: 'Project', icon: Package, run: () => run(exportProject()) }] },
      { name: 'Survey', items: [stub('Import', FileInput), stub('Draw', Route), stub('Adjust', Waypoints)] },
      {
        name: 'Live',
        items: [
          { label: 'Share', icon: Users, run: () => (p2p.dialog = true), pressed: () => live.status === 'live' && p2p.role === 'host' },
          { label: 'Join', icon: LogIn, run: () => { shelf.join = true; shelf.open = true; }, pressed: () => live.status === 'live' && p2p.role === 'guest' },
          { label: 'Classroom', icon: GraduationCap, run: () => (classroom.dialog = true), pressed: () => classroom.role === 'teacher' },
          { label: 'Leave', icon: LogOut, run: () => { leaveP2p(); leave(); } },
        ],
      },
    ],
    Draw: [
      { name: 'Linework', items: [], custom: DrawTools, props: { group: 'Linework' } },
      { name: 'Passage lines', items: [], custom: DrawTools, props: { group: 'Passage lines' } },
      { name: 'Geology and relief', items: [], custom: DrawTools, props: { group: 'Geology and relief' } },
      { name: 'Symbols', items: [{ label: 'Symbols', icon: Shapes, run: () => showPanel('symbols'), pressed: () => !!panelState.visible.symbols }] },
      { name: 'Area fill', items: [], custom: FillTools },
    ],
    Annotation: [
      { name: 'Notes and leaders', items: [], custom: DrawTools, props: { group: 'Annotation' } },
    ],
    Sheets: [
      { name: 'Paper and scale', items: [], custom: SheetGroup, props: { part: 'paper' } },
      { name: 'Title block', items: [], custom: SheetGroup, props: { part: 'title' } },
      { name: 'Show', items: [], custom: SheetGroup, props: { part: 'pieces' } },
      { name: 'Save', items: [], custom: SheetGroup, props: { part: 'save' } },
    ],
    View: [
      { name: 'Panels', items: [panelItem('map', MapIcon), panelItem('profile', Activity), panelItem('section', Scissors), panelItem('symbols', Shapes), panelItem('patterns', Shapes), panelItem('stations', Table2), panelItem('notebook', BookOpen), panelItem('scans', ScanLine), panelItem('sheet', FileText), panelItem('console', Terminal)] },
      { name: 'Units', items: [{ label: 'Metric', icon: Ruler, run: () => setUnits('metric'), pressed: () => cave.units === 'metric' }, { label: 'Imperial', icon: Footprints, run: () => setUnits('imperial'), pressed: () => cave.units === 'imperial' }] },
      { name: 'Theme', items: [], custom: ThemeGroup },
      { name: 'Drawing scale', items: [], custom: DrawingScale },
      { name: 'Rock halo', items: [{ label: 'Rock halo', icon: Mountain, run: () => setOption('halo', !option('halo', true)), pressed: () => option('halo', true) }, { label: 'Settings', icon: SlidersHorizontal, run: () => setPick('halo', 'halo'), pressed: () => cave.pick.kind === 'halo' }] },
      { name: 'Live editing', items: [], custom: CollaborateGroup },
      { name: 'Layout', items: [{ label: 'Reset', icon: LayoutDashboard, run: resetLayout }] },
      { name: 'Ribbon settings', items: [{ label: 'Compact layout', icon: PanelTop, run: () => setCompactRibbon(!cave.compactRibbon), pressed: () => cave.compactRibbon }] },
    ],
  };
  let active = $state('Home');

  // A running command gets its own tab, in front, with its settings and a way to finish it.
  const CONTEXT = '\u0000command';
  // In a class the teacher's tab is mirrored (a contextual tab is each person's own, from the tool they have).
  $effect(() =>
    registerMirror('ribbon', {
      read: () => (active === CONTEXT ? undefined : active),
      write: (tab) => {
        if (typeof tab === 'string' && tab in tabs) active = tab;
      },
    })
  );
  let toolLabels = $state<Record<string, string>>({});
  onMount(() => {
    loadEngine().then(() => (toolLabels = Object.fromEntries(engine.drawTools().map((t) => [t.key, t.label]))));
  });
  // A selected line gets one too, with what can be done to it, so it is easy to adjust.
  let loaded = $state(false);
  onMount(() => {
    loadEngine().then(() => (loaded = true));
  });
  const selections = $derived.by(() => {
    void cave.version;
    void cave.also;
    return loaded && !cave.tool ? selectedIds().map((id) => ({ id, info: engine.lineInfo(id) })) : [];
  });
  const selection = $derived(selections[0]?.info ?? null);
  /** More than one line is selected. */
  const several = $derived(selections.length > 1);
  /** Every selected line is of one kind (so its settings can be edited together). */
  const alike = $derived(selections.every((s) => s.info && s.info.kind === selection?.kind));
  $effect(() => {
    // Selected lines that are gone (undone, deleted elsewhere) are dropped from the selection.
    if (loaded && !cave.tool && selections.some((s) => s.info === null)) selectIds(selections.filter((s) => s.info).map((s) => s.id));
  });
  $effect(() => {
    if (copyOp.on && !selections.length) endCopy();
  });
  const mode = $derived(cave.tool ? 'command' : selection ? 'selection' : cave.pick.kind ? 'pick' : '');
  // one thing is picked at a time: a line, a station, or a tool
  $effect(() => {
    if (cave.tool || cave.selected) clearPick();
  });
  const contextName = $derived(
    mode === 'command' ? (toolLabels[cave.tool] ?? symbolDef(cave.tool)?.name ?? fillLabel(cave.tool) ?? cave.tool) : mode === 'pick' ? INSPECTORS[cave.pick.kind as Exclude<PickKind, ''>].title : mode === 'selection' ? (several && !alike ? 'MULTIPLE' : `Edit ${toolLabels[selection!.kind] ?? symbolDef(selection!.kind)?.name ?? fillLabel(selection!.kind) ?? selection!.kind}${several ? ` (${selections.length})` : ''}`) : '',
  );
  const commandGroups: Group[] = [
    { name: 'Settings', items: [], custom: ContextOptions },
    { name: 'Command', items: [{ label: 'Done', icon: Check, run: () => (cave.tool = '') }] },
  ];
  const commandGroup: Group = {
    name: 'Command',
    items: [
      {
        label: 'Delete',
        icon: Trash2,
        run: () => {
          const ids = selectedIds();
          cave.selected = '';
          for (const id of ids) engine.deleteLine(id);
          cave.version += 1;
        },
      },
      { label: 'Copy', icon: CopyIcon, run: () => (copyOp.on ? endCopy() : startCopy()), pressed: () => copyOp.on },
      { label: 'Done', icon: Check, run: () => { endCopy(); cave.selected = ''; } },
    ],
  };
  // A selection's tab is several groups (what it is, its text format, its leader, its line style), so a narrow window folds one
  // of them, not all of its settings at once.
  const selectionGroups = $derived.by<Group[]>(() => {
    const kind = selection?.kind ?? '';
    const infos = selections.map((s) => s.info).filter((i): i is NonNullable<typeof i> => !!i);
    const stamps = infos.length > 0 && infos.every((i) => isStampKind(i.kind));
    const words = infos.some((i) => i.kind === 'text' || i.kind === 'callout' || !!i.text);
    const part = (name: string, p: string): Group => ({ name, items: [], custom: SelectionTools, props: { part: p } });
    const out: Group[] = [part(stamps ? 'Annotation' : 'Line', 'type')];
    if (stamps && words) out.push(part('Text format', 'format'));
    if (alike && kind === 'callout') out.push(part('Leader', 'leader'));
    if (alike && !isStampKind(kind) && kind !== 'fill-custom') out.push(part(isFillKind(kind) ? 'Area' : 'Line style', 'style'));
    out.push(commandGroup);
    return out;
  });
  // what a picked thing's tab holds comes from the inspector registered for its kind
  const pickGroups = $derived<Group[]>(cave.pick.kind ? INSPECTORS[cave.pick.kind as Exclude<PickKind, ''>].groups.map((g) => ({ name: g.name, items: [], custom: INSPECTORS[cave.pick.kind as Exclude<PickKind, ''>].component, props: { part: g.part, kind: cave.pick.kind } })) : []);
  let before = 'Home';
  $effect(() => {
    if (mode) {
      if (active !== CONTEXT) before = active;
      active = CONTEXT;
    } else if (active === CONTEXT) {
      active = before;
    }
  });
  const shownGroups = $derived(active === CONTEXT ? (mode === 'selection' ? selectionGroups : mode === 'pick' ? pickGroups : commandGroups) : (tabs[active] ?? []));

  // Groups that do not fit the width fold into one button each (the rightmost first); the button opens
  // a popup under it with the group's buttons, two to a row.
  const FOLDED = 96; // width of a folded group
  let barW = $state(0);
  // how wide each group wants to be, by name (a selection's groups come and go, so an index would go stale)
  let widths = $state<Record<string, number>>({});
  let openAt = $state<{ i: number; x: number; y: number } | null>(null);
  const collapsed = $derived.by(() => {
    const out = new Set<number>();
    const w = (i: number) => widths[shownGroups[i].name] ?? 0;
    const foldable = (i: number) => !out.has(i) && w(i) > FOLDED;
    if (cave.compactRibbon) {
      shownGroups.forEach((_, i) => foldable(i) && out.add(i));
      return out;
    }
    // Fold as few groups as will fit, and among those the ones furthest right (the leftmost groups are the ones used most).
    const need = shownGroups.reduce((n, _, i) => n + w(i), 0) - (barW - 20);
    if (need <= 0) return out;
    const can = shownGroups.map((_, i) => i).filter(foldable).slice(-14);
    let best = -1;
    let bestCount = Infinity;
    let bestRight = -1;
    for (let mask = 1; mask < 1 << can.length; mask++) {
      let saved = 0;
      let count = 0;
      let right = 0;
      for (let b = 0; b < can.length; b++) {
        if (mask & (1 << b)) {
          saved += w(can[b]) - FOLDED;
          count++;
          right += can[b];
        }
      }
      if (saved >= need && (count < bestCount || (count === bestCount && right > bestRight))) {
        best = mask;
        bestCount = count;
        bestRight = right;
      }
    }
    // not even folding everything fits: fold everything that can be
    const chosen = best > 0 ? can.filter((_, b) => best & (1 << b)) : can;
    for (const i of chosen) out.add(i);
    return out;
  });
  $effect(() => {
    void active;
    void collapsed;
    if (openAt && !collapsed.has(openAt.i)) openAt = null;
  });
  function toggleMore(i: number, button: HTMLElement) {
    if (openAt?.i === i) {
      openAt = null;
      return;
    }
    const r = button.closest('section')!.getBoundingClientRect();
    openAt = { i, x: Math.max(4, Math.min(r.left, window.innerWidth - 340)), y: r.bottom + 2 };
  }
  function outside(e: PointerEvent) {
    if (openAt && !(e.target as Element).closest('.more, section.collapsed')) openAt = null;
  }

  function keys(e: KeyboardEvent) {
    if (e.key === 'Escape' && copyOp.on) {
      endCopy(); // Escape finishes the Copy command first; the selection stays
      return;
    }
    if (e.key === 'Escape' && openAt) {
      openAt = null;
      return;
    }
    if (e.key === 'Escape' && !cave.tool && !cave.selected && cave.pick.kind && !(e.target as Element | null)?.closest?.('input, select, textarea')) {
      clearPick();
      return;
    }
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

{#snippet full(g: Group)}
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
{/snippet}

<svelte:window onkeydown={keys} onpointerdown={outside} onresize={() => (openAt = null)} />

<header>
  <nav class="tabs">
    {#each Object.keys(tabs) as t}
      <button class:on={t === active} onclick={() => (active = t)}>{t}</button>
    {/each}
    {#if mode}
      <button class="ctx" class:on={active === CONTEXT} onclick={() => (active = CONTEXT)}>{contextName}</button>
    {/if}
    <UserMenu />
  </nav>
  <div class="bar" bind:clientWidth={barW}>
    {#each shownGroups as g, i (g.name)}
      {#if collapsed.has(i)}
        <section class="collapsed">
          <div class="btns">
            <button class="tool" class:pressed={openAt?.i === i} aria-haspopup="true" aria-expanded={openAt?.i === i} title={`${g.name}: show the buttons`} onclick={(e) => toggleMore(i, e.currentTarget)}>
              <ChevronDown size={22} strokeWidth={1.6} />
              <span>{g.name}</span>
            </button>
          </div>
          <div class="gname">{g.name}</div>
        </section>
      {:else}
        {@render full(g)}
      {/if}
    {:else}
      <p class="empty">Nothing here yet.</p>
    {/each}
    <!-- every group at full size, unseen, so we know how wide each wants to be -->
    <div class="measure" aria-hidden="true" inert>
      {#each shownGroups as g, i (g.name)}
        <section bind:offsetWidth={widths[g.name]}>
          <div class="btns">
            {#if g.custom}<g.custom {...(g.props ?? {})} measure={true} />{/if}
            {#each g.items as item}
              <button class="tool" tabindex="-1"><item.icon size={22} strokeWidth={1.6} /><span>{item.label}</span></button>
            {/each}
          </div>
          <div class="gname">{g.name}</div>
        </section>
      {/each}
    </div>
  </div>
  {#if openAt && shownGroups[openAt.i]}
    {@const g = shownGroups[openAt.i]}
    <div class="more" role="menu" style:left="{openAt.x}px" style:top="{openAt.y}px">
      <div class="grid">
        {#if g.custom}<g.custom {...(g.props ?? {})} />{/if}
        {#each g.items as item}
          <button class="tool" class:pressed={item.pressed?.()} role="menuitem" onclick={() => { item.run(); openAt = null; }}>
            <item.icon size={16} strokeWidth={1.6} />
            <span>{item.label}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</header>

<style>
  header { background: var(--panel); }
  .tabs { display: flex; gap: 2px; padding: 6px 10px 0; align-items: flex-end; }
  .tabs button { background: none; border: 0; padding: 6px 14px; color: var(--muted); border-radius: var(--radius) var(--radius) 0 0; cursor: pointer; }
  .tabs button.ctx { color: var(--accent); margin-left: 10px; box-shadow: inset 0 2px 0 var(--accent); }
  .tabs button.on { color: var(--text); background: var(--accent-bg); font-weight: 500; }
  .bar { position: relative; display: flex; box-sizing: border-box; height: var(--ribbon-h); overflow: hidden; padding: 6px 10px; border-top: 1px solid var(--line); }
  section { display: flex; flex-direction: column; align-items: center; height: 100%; padding: 0 12px; border-right: 1px solid var(--line); }
  .btns { display: flex; align-items: center; gap: 4px; height: var(--ribbon-body); overflow: hidden; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; }
  .tool:hover { background: var(--panel-2); }
  .tool:active, .tool.pressed { background: var(--accent-bg); }
  .gname { height: var(--ribbon-name); line-height: var(--ribbon-name); font-size: 11px; color: var(--muted); white-space: nowrap; }
  section.collapsed { width: 96px; box-sizing: border-box; padding: 0 8px; }
  .measure { position: absolute; left: 0; top: 0; display: flex; visibility: hidden; pointer-events: none; white-space: nowrap; height: 100%; padding: 6px 10px; box-sizing: border-box; }
  .more { position: fixed; z-index: 50; width: 330px; box-sizing: border-box; padding: 6px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 6px 20px rgb(0 0 0 / 0.3); max-height: 70vh; overflow: auto; }
  .more .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 2px; }
  .more .grid > .tool, .more :global(.tools) > :global(.tool) { flex-direction: row; justify-content: flex-start; gap: 8px; min-width: 0; height: 30px; padding: 4px 10px; font-size: 13px; text-align: left; }
  .more :global(.tools) { display: grid; grid-template-columns: 1fr 1fr; gap: 2px; height: auto; grid-column: 1 / -1; }
  .more .grid > :global(div) { grid-column: 1 / -1; }
  .empty { color: var(--muted); margin: 0; align-self: center; }
</style>
