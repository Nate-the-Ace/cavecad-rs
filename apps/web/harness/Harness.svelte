<script lang="ts">
  // The page around the dock, built the way the app is: the real ribbon on top (here with a Draw tab and a View
  // tab), the dock below with panels the person arranges as they like (remembered on this device), and the feed
  // that keeps the sketchpad in step with the mock notebook. The ribbon, Draw tab, options tab and Edit tab are
  // the app's own components; the Survey panel and the sketchpad panes are the real ones; the notebook is a mock.
  import { onMount, untrack } from 'svelte';
  import { Activity, BookOpen, Crosshair, Footprints, LayoutDashboard, Lock, Map as MapIcon, Ruler, Scissors, Trash2 } from '@lucide/svelte';
  import Ribbon, { type Group } from '../src/components/Ribbon.svelte';
  import DrawTools from '../src/components/DrawTools.svelte';
  import LayoutGroup from '../src/components/LayoutGroup.svelte';
  import { cave, option, setOption, setUnits } from '../src/lib/cave.svelte';
  import { loadEngine } from '../src/lib/engine';
  import { survey } from '../src/lib/survey.svelte';
  import { standing } from '../src/lib/survey';
  import { sketch } from '../src/lib/sketch.svelte';
  import type { DimsByStation, SurveyShot } from '../src/lib/sketch-geometry';
  import { PANELS, initDock, panelState, resetDock, togglePanel } from './dock.svelte';

  type Shot = { id: string; from: string; to: string | null; c: Record<string, number | null> };
  type Book = { shots: Shot[]; walls: Record<string, unknown> };
  const bookOf = () => (globalThis as unknown as { __book?: Book }).__book;

  let host: HTMLDivElement;
  onMount(() => {
    // The harness opens in feet (1" = 20' is the working scale); the View tab's Units group changes it, and that is remembered.
    try {
      if (!localStorage.getItem('cavecad.harness.units')) {
        localStorage.setItem('cavecad.harness.units', 'set');
        setUnits('imperial');
      }
    } catch {
      /* storage unavailable: the app's default stays */
    }
    sketch.open('harness');
    return initDock(host);
  });
  loadEngine();

  function resetNotebook() {
    const book = bookOf();
    if (book) {
      book.shots.length = 0;
      for (const k of Object.keys(book.walls)) delete book.walls[k];
    }
    location.reload();
  }

  const icons = { survey: Crosshair, notebook: BookOpen, plan: MapIcon, profile: Activity, section: Scissors } as const;

  /** The ribbon: the app's Draw groups as they are, and a View tab of the same kind as the app's. */
  const tabs: Record<string, Group[]> = {
    Draw: ['Linework', 'Passage lines', 'Geology and relief'].map((name) => ({ name, items: [], custom: DrawTools, props: { group: name } })),
    View: [
      { name: 'Panels', items: Object.entries(PANELS).map(([id, def]) => ({ label: def.title, icon: icons[id as keyof typeof icons], run: () => togglePanel(id), pressed: () => !!panelState.open[id] })) },
      { name: 'Sketch', items: [{ label: 'Follow survey', icon: Crosshair, run: () => setOption('followSurvey', !option('followSurvey', true)), pressed: () => option('followSurvey', true) }] },
      { name: 'Units', items: [{ label: 'Metric', icon: Ruler, run: () => setUnits('metric'), pressed: () => cave.units === 'metric' }, { label: 'Imperial', icon: Footprints, run: () => setUnits('imperial'), pressed: () => cave.units === 'imperial' }] },
      { name: 'Layout', items: [{ label: 'Reset', icon: LayoutDashboard, run: resetDock }, { label: 'New notebook', icon: Trash2, run: resetNotebook }] },
      { name: 'Saved layouts', items: [], custom: LayoutGroup },
      { name: 'Map view', items: [{ label: 'Lock view', icon: Lock, run: () => setOption('mapLock', !option('mapLock', false)), pressed: () => option('mapLock', false) }] },
    ],
  };

  // The sketchpad draws what the mock notebook holds (the real app would read the notebook page the same way).
  $effect(() => {
    void cave.version;
    const here = survey.state.from ? standing(survey.state) : null;
    untrack(() => {
      const book = bookOf();
      const shots: SurveyShot[] = ($state.snapshot(book?.shots ?? []) as Shot[]).map((s) => ({
        id: s.id, from: s.from, to: s.to, distance: s.c.distance ?? null, az: s.c.az_fs ?? null, inc: s.c.inc_fs ?? null, azBack: s.c.az_bs ?? null, incBack: s.c.inc_bs ?? null,
      }));
      sketch.setSource(shots, structuredClone($state.snapshot(book?.walls ?? {})) as DimsByStation, here);
    });
  });
</script>

<div class="page">
  <Ribbon {tabs} />
  <div class="dock" bind:this={host}></div>
</div>

<style>
  .page { display: flex; flex-direction: column; height: 100vh; background: var(--bg, #111); color: var(--text); font-size: 13px; }
  .dock { flex: 1; min-height: 0; }
</style>
