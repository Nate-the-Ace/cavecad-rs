// The harness's workspace: the same dockview the app uses (panels you can drag, resize, tab together, float,
// maximize, minimize, close and bring back), with the Survey panel, a mock notebook and the sketchpad's
// panes as its panels. The layout is remembered; a layout that no longer fits is dropped, not trusted.
import { createDockview, themeDark, themeLight } from 'dockview-core/dist/dockview-core.js';
import type { DockviewApi } from 'dockview-core';
import type { Component } from 'svelte';
import { GroupActions, SveltePanel } from '../src/lib/dock-parts';
import { isDark, onTheme } from '../src/lib/theme';
import { setLayoutHost } from '../src/lib/layouts.svelte';
import SurveyPanel from '../src/panels/SurveyPanel.svelte';
import SketchPlanPanel from '../src/panels/SketchPlanPanel.svelte';
import SketchProfilePanel from '../src/panels/SketchProfilePanel.svelte';
import SketchSectionPanel from '../src/panels/SketchSectionPanel.svelte';
import MockNotebook from './MockNotebook.svelte';

const LAYOUT_KEY = 'cavecad.harness.layout.v1';

interface PanelDef {
  title: string;
  component: Component;
  height?: number;
  width?: number;
}

export const PANELS: Record<string, PanelDef> = {
  survey: { title: 'Survey', component: SurveyPanel, width: 400 },
  notebook: { title: 'Notebook', component: MockNotebook },
  plan: { title: 'Plan', component: SketchPlanPanel },
  profile: { title: 'Profile', component: SketchProfilePanel, width: 430 },
  section: { title: 'Cross section', component: SketchSectionPanel, height: 330 },
};

/** Which panels are open (drives the toggle buttons). */
export const panelState = $state<{ open: Record<string, boolean> }>({ open: {} });

let api: DockviewApi | null = null;
let hostEl: HTMLElement | null = null;

function refreshOpen() {
  for (const id of Object.keys(PANELS)) panelState.open[id] = api?.getPanel(id) !== undefined;
}

function defaultLayout(a: DockviewApi) {
  a.clear();
  // Sizes are shares of the window, so the plan keeps most of the room on a small screen too.
  const W = hostEl?.clientWidth || 1200;
  const H = hostEl?.clientHeight || 700;
  const surveyW = Math.min(440, Math.max(320, Math.round(W * 0.3)));
  const profileW = Math.round(W * 0.27);
  const sectionH = Math.round(H * 0.42);
  const add = (id: string, position?: Parameters<DockviewApi['addPanel']>[0]['position'], size?: { initialWidth?: number; initialHeight?: number }) =>
    a.addPanel({ id, component: id, title: PANELS[id].title, position, ...size });
  add('survey');
  add('notebook', { referencePanel: 'survey', direction: 'within' });
  add('plan', { referencePanel: 'survey', direction: 'right' });
  add('profile', { referencePanel: 'plan', direction: 'right' }, { initialWidth: profileW });
  add('section', { referencePanel: 'profile', direction: 'below' }, { initialHeight: sectionH });
  a.getPanel('survey')?.api.setActive();
  a.getPanel('survey')?.api.setSize({ width: surveyW });
}

function restore(a: DockviewApi): boolean {
  try {
    const saved = localStorage.getItem(LAYOUT_KEY);
    if (!saved) return false;
    a.fromJSON(JSON.parse(saved));
    return a.panels.length > 0 && a.panels.every((p) => p.id in PANELS);
  } catch {
    return false;
  }
}

/** Builds the workspace inside `el`. Returns a function that tears it down. */
export function initDock(el: HTMLElement): () => void {
  hostEl = el;
  api = createDockview(el, {
    theme: isDark() ? themeDark : themeLight,
    createComponent: ({ name }) => new SveltePanel(PANELS[name].component),
    createRightHeaderActionComponent: () => new GroupActions(),
  });
  const a = api;
  if (!restore(a)) defaultLayout(a);
  refreshOpen();
  setLayoutHost({
    capture: () => a.toJSON(),
    has: (id) => id in PANELS,
    apply: (layout) => {
      const before = a.toJSON();
      try {
        a.fromJSON(layout as Parameters<DockviewApi['fromJSON']>[0]);
      } catch {
        try {
          a.fromJSON(before);
        } catch {
          /* the dock keeps what it has */
        }
        return false;
      }
      refreshOpen();
      return true;
    },
  });
  let timer: ReturnType<typeof setTimeout>;
  const layoutSub = a.onDidLayoutChange(() => {
    refreshOpen();
    clearTimeout(timer);
    timer = setTimeout(() => {
      try {
        localStorage.setItem(LAYOUT_KEY, JSON.stringify(a.toJSON()));
      } catch {
        /* the layout just is not remembered */
      }
    }, 300);
  });
  const offTheme = onTheme((dark) => a.updateOptions({ theme: dark ? themeDark : themeLight }));
  return () => {
    setLayoutHost(null);
    clearTimeout(timer);
    layoutSub.dispose();
    offTheme();
    a.dispose();
    api = null;
  };
}

/** Shows a hidden panel or hides a visible one. */
export function togglePanel(id: string) {
  if (!api) return;
  const existing = api.getPanel(id);
  if (existing) api.removePanel(existing);
  else {
    const ref = api.panels[0];
    api.addPanel({ id, component: id, title: PANELS[id].title, position: ref ? { referencePanel: ref.id, direction: 'below' } : undefined, initialHeight: PANELS[id].height });
  }
  refreshOpen();
}

/** Puts every panel back where it started. */
export function resetDock() {
  if (!api) return;
  try {
    localStorage.removeItem(LAYOUT_KEY);
  } catch {
    /* ignore */
  }
  defaultLayout(api);
  refreshOpen();
}
