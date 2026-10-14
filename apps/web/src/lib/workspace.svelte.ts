// The dockable workspace: panels you can drag, resize, tab together, float, maximize, minimize,
// hide and bring back. Layout is remembered in the browser; a layout that no longer fits the
// current panels is discarded rather than trusted.
// The all-in-one bundle is used on purpose: it is the only build that carries dockview's styles.
import { createDockview, themeDark, themeLight } from 'dockview-core/dist/dockview-core.js';
import type { DockviewApi } from 'dockview-core';
import type { Component } from 'svelte';
import { GroupActions, SveltePanel } from './dock-parts';
import { setOption } from './cave.svelte';
import { setLayoutHost } from './layouts.svelte';
import { isDark, onTheme } from './theme';
import MapPanel from '../panels/MapPanel.svelte';
import ConsolePanel from '../components/Console.svelte';
import ProfilePanel from '../panels/ProfilePanel.svelte';
import SectionPanel from '../panels/SectionPanel.svelte';
import SymbolsPanel from '../panels/SymbolsPanel.svelte';
import StationsPanel from '../panels/StationsPanel.svelte';
import SheetPanel from '../panels/SheetPanel.svelte';
import NotebookPanel from '../panels/NotebookPanel.svelte';
import ScansPanel from '../panels/ScansPanel.svelte';
import SurveyPanel from '../panels/SurveyPanel.svelte';

const LAYOUT_KEY = 'cavecad.layout.v1';

interface PanelDef {
  title: string;
  component: Component;
  /** Height in pixels when first opened below another panel. */
  height?: number;
  /** Width in pixels when first opened beside another panel. */
  width?: number;
}

/** Every panel the workspace can show. Add new panels here. */
export const PANELS: Record<string, PanelDef> = {
  map: { title: 'Map', component: MapPanel },
  profile: { title: 'Profile', component: ProfilePanel, width: 460 },
  section: { title: 'Cross Section', component: SectionPanel, width: 460 },
  symbols: { title: 'Symbols', component: SymbolsPanel, width: 300 },
  stations: { title: 'Stations', component: StationsPanel, height: 260 },
  notebook: { title: 'Notebook', component: NotebookPanel, height: 320 },
  survey: { title: 'Survey', component: SurveyPanel, width: 380 },
  scans: { title: 'Scans', component: ScansPanel, width: 520 },
  sheet: { title: 'Sheet', component: SheetPanel, width: 520 },
  console: { title: 'Console', component: ConsolePanel, height: 260 },
};

/** Which panels are open right now (drives the View tab's toggles). */
/** Which panels are open, and which of those can be seen right now (not behind another tab). */
export const panelState = $state<{ open: Record<string, boolean>; visible: Record<string, boolean> }>({ open: {}, visible: {} });

let api: DockviewApi | null = null;

function refreshOpen() {
  for (const id of Object.keys(PANELS)) {
    const panel = api?.getPanel(id);
    panelState.open[id] = panel !== undefined;
    panelState.visible[id] = !!panel?.api.isVisible;
  }
}

function defaultLayout(a: DockviewApi) {
  a.clear();
  a.addPanel({ id: 'map', component: 'map', title: PANELS.map.title });
  a.addPanel({ id: 'profile', component: 'profile', title: PANELS.profile.title, position: { referencePanel: 'map', direction: 'right' }, initialWidth: PANELS.profile.width });
  a.addPanel({ id: 'section', component: 'section', title: PANELS.section.title, position: { referencePanel: 'profile', direction: 'within' } });
  a.panels.find((p) => p.id === 'profile')?.api.setActive();
  a.addPanel({ id: 'console', component: 'console', title: PANELS.console.title, position: { referencePanel: 'map', direction: 'below' }, initialHeight: PANELS.console.height });
  // The palette is a wide strip: tiles flow across the bottom, beside the console.
  a.addPanel({ id: 'symbols', component: 'symbols', title: PANELS.symbols.title, position: { referencePanel: 'console', direction: 'within' } });
  a.addPanel({ id: 'stations', component: 'stations', title: PANELS.stations.title, position: { referencePanel: 'console', direction: 'within' } });
  a.addPanel({ id: 'notebook', component: 'notebook', title: PANELS.notebook.title, position: { referencePanel: 'console', direction: 'within' } });
  // Below the Profile, not tabbed with it: aligning a scan needs the scan and the view side by side.
  a.addPanel({ id: 'scans', component: 'scans', title: PANELS.scans.title, position: { referencePanel: 'profile', direction: 'below' }, initialHeight: 320 });
  a.panels.find((p) => p.id === 'profile')?.api.setActive();
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
export function initWorkspace(el: HTMLElement): () => void {
  api = createDockview(el, {
    theme: isDark() ? themeDark : themeLight,
    createComponent: ({ name }) => new SveltePanel(PANELS[name].component),
    createRightHeaderActionComponent: () => new GroupActions(),
  });
  const a = api;
  if (import.meta.env.DEV) (window as unknown as { __dock: DockviewApi }).__dock = a; // for tests and debugging
  if (!restore(a)) defaultLayout(a);
  refreshOpen();
  // Saved layouts read the arrangement from this dock and put one back into it.
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
        /* storage unavailable: the layout just is not remembered */
      }
    }, 300);
  });
  // Which tab is in front decides what can be seen, so a change of tab refreshes it too.
  const activeSub = a.onDidActivePanelChange(() => refreshOpen());
  const offTheme = onTheme((dark) => a.updateOptions({ theme: dark ? themeDark : themeLight }));

  return () => {
    setLayoutHost(null);
    clearTimeout(timer);
    layoutSub.dispose();
    offTheme();
    activeSub.dispose();
    a.dispose();
    api = null;
  };
}

/** Shows a hidden panel or hides a visible one. */
export function togglePanel(id: string) {
  if (!api) return;
  const existing = api.getPanel(id);
  if (existing) {
    api.removePanel(existing);
  } else {
    const ref = api.panels[0];
    api.addPanel({ id, component: id, title: PANELS[id].title, position: ref ? { referencePanel: ref.id, direction: 'below' } : undefined, initialHeight: PANELS[id].height });
  }
  refreshOpen();
}

/** Brings a panel to the front, opening it first if it is closed. */
export function showPanel(id: string) {
  if (!api) return;
  const existing = api.getPanel(id);
  if (existing) existing.api.setActive();
  else togglePanel(id);
  api.getPanel(id)?.api.setActive();
}

/** Puts every panel back where it started. */
export function resetLayout() {
  if (!api) return;
  try {
    localStorage.removeItem(LAYOUT_KEY);
  } catch {
    /* ignore */
  }
  defaultLayout(api);
  refreshOpen();
}

/**
 * The arrangement for building a cave from its scans: the scans on the left, the notebook beside them
 * to type into, the map small underneath (with the profile and cross section behind it). Panels not
 * in it come back from the View tab.
 */
export function workLayout() {
  if (!api) return;
  const a = api;
  a.clear();
  a.addPanel({ id: 'scans', component: 'scans', title: PANELS.scans.title });
  a.addPanel({ id: 'notebook', component: 'notebook', title: PANELS.notebook.title, position: { referencePanel: 'scans', direction: 'right' } });
  a.addPanel({ id: 'map', component: 'map', title: PANELS.map.title, position: { referencePanel: 'notebook', direction: 'below' }, initialHeight: 200 });
  a.addPanel({ id: 'profile', component: 'profile', title: PANELS.profile.title, position: { referencePanel: 'map', direction: 'within' } });
  a.addPanel({ id: 'section', component: 'section', title: PANELS.section.title, position: { referencePanel: 'map', direction: 'within' } });
  a.panels.find((p) => p.id === 'map')?.api.setActive();
  // Narrower scans, wider notebook: its columns need the room.
  a.getPanel('scans')?.api.setSize({ width: Math.round(el0Width() * 0.38) });
  refreshOpen();
}

/**
 * The arrangement for surveying in the cave, made for a tablet held landscape: the Survey panel on the
 * left (with the Notebook behind it as a tab), the Map in plan in the middle, and the Profile over the
 * Cross Section on the right, so each shot shows in all three views as it is taken.
 */
export function surveyLayout() {
  if (!api) return;
  const a = api;
  a.clear();
  a.addPanel({ id: 'survey', component: 'survey', title: PANELS.survey.title });
  a.addPanel({ id: 'notebook', component: 'notebook', title: PANELS.notebook.title, position: { referencePanel: 'survey', direction: 'within' } });
  a.addPanel({ id: 'map', component: 'map', title: PANELS.map.title, position: { referencePanel: 'survey', direction: 'right' } });
  a.addPanel({ id: 'profile', component: 'profile', title: PANELS.profile.title, position: { referencePanel: 'map', direction: 'right' } });
  a.addPanel({ id: 'section', component: 'section', title: PANELS.section.title, position: { referencePanel: 'profile', direction: 'below' } });
  a.panels.find((p) => p.id === 'survey')?.api.setActive();
  a.getPanel('survey')?.api.setSize({ width: Math.max(340, Math.round(el0Width() * 0.3)) });
  setOption('mapLock', true); // sketching in the cave: the map must not turn under the pen
  a.getPanel('profile')?.api.setSize({ width: Math.round(el0Width() * 0.3) });
  refreshOpen();
}

/** The width of the workspace, for sizing panels as a share of it. */
function el0Width(): number {
  return document.querySelector('.workspace')?.clientWidth ?? window.innerWidth;
}
