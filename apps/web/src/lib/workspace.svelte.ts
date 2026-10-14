// The dockable workspace: panels you can drag, resize, tab together, float, maximize, minimize,
// hide and bring back. Layout is remembered in the browser; a layout that no longer fits the
// current panels is discarded rather than trusted.
// The all-in-one bundle is used on purpose: it is the only build that carries dockview's styles.
import { createDockview, themeDark, themeLight } from 'dockview-core/dist/dockview-core.js';
import type { DockviewApi, DockviewGroupPanel, IContentRenderer, IHeaderActionsRenderer } from 'dockview-core';
import { mount, unmount, type Component } from 'svelte';
import { isDark, onTheme } from './theme';
import { frontPanels, layoutShape, registerMirror } from './classroom-mirror';
import { classroom } from './classroom.svelte';
import MapPanel from '../panels/MapPanel.svelte';
import ConsolePanel from '../components/Console.svelte';
import ProfilePanel from '../panels/ProfilePanel.svelte';
import SectionPanel from '../panels/SectionPanel.svelte';
import SymbolsPanel from '../panels/SymbolsPanel.svelte';
import StationsPanel from '../panels/StationsPanel.svelte';
import SheetPanel from '../panels/SheetPanel.svelte';
import NotebookPanel from '../panels/NotebookPanel.svelte';
import ScansPanel from '../panels/ScansPanel.svelte';
import PatternsPanel from '../panels/PatternsPanel.svelte';

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
  scans: { title: 'Scans', component: ScansPanel, width: 520 },
  sheet: { title: 'Sheet', component: SheetPanel, width: 520 },
  patterns: { title: 'Patterns', component: PatternsPanel, width: 560 },
  console: { title: 'Console', component: ConsolePanel, height: 260 },
};

/** Which panels are open right now (drives the View tab's toggles). */
/** Which panels are open, and which of those can be seen right now (not behind another tab). */
export const panelState = $state<{ open: Record<string, boolean>; visible: Record<string, boolean> }>({ open: {}, visible: {} });

let api: DockviewApi | null = null;

class SveltePanel implements IContentRenderer {
  readonly element = document.createElement('div');
  private instance: ReturnType<typeof mount> | null = null;
  constructor(private readonly component: Component) {
    this.element.style.height = '100%';
  }
  init() {
    this.instance = mount(this.component, { target: this.element });
  }
  dispose() {
    if (this.instance) unmount(this.instance);
  }
}

/** Height of a group's tab bar: what a minimized panel collapses to. */
const TAB_BAR = 35;

const ICON = {
  minimize: '<svg width="14" height="14" viewBox="0 0 14 14"><path d="M3 10h8" stroke="currentColor" stroke-width="1.5" fill="none"/></svg>',
  restore: '<svg width="14" height="14" viewBox="0 0 14 14"><path d="M3 5l4 4 4-4" stroke="currentColor" stroke-width="1.5" fill="none"/></svg>',
  maximize: '<svg width="14" height="14" viewBox="0 0 14 14"><rect x="3" y="3" width="8" height="8" stroke="currentColor" stroke-width="1.5" fill="none"/></svg>',
  float: '<svg width="14" height="14" viewBox="0 0 14 14"><path d="M5 3h6v6M11 3L5 9M3 6v5h5" stroke="currentColor" stroke-width="1.4" fill="none"/></svg>',
};

/** Buttons shown at the right of every panel group's tab bar. */
class GroupActions implements IHeaderActionsRenderer {
  readonly element = document.createElement('div');
  private minimizedFrom: { height: number; min: number; max: number } | null = null;
  constructor() {
    this.element.className = 'dv-actions';
  }
  init(p: Parameters<IHeaderActionsRenderer['init']>[0]) {
    const button = (icon: string, label: string, onClick: () => void) => {
      const b = document.createElement('button');
      b.innerHTML = icon;
      b.title = label;
      b.setAttribute('aria-label', label);
      b.onclick = onClick;
      this.element.appendChild(b);
      return b;
    };
    const min = button(ICON.minimize, 'Minimize', () => {
      if (this.minimizedFrom === null) {
        this.minimizedFrom = { height: p.api.height, min: p.group.minimumHeight, max: p.group.maximumHeight };
        p.api.setConstraints({ minimumHeight: TAB_BAR, maximumHeight: TAB_BAR });
        p.api.setSize({ height: TAB_BAR });
        min.innerHTML = ICON.restore;
        min.title = 'Restore';
      } else {
        p.api.setConstraints({ minimumHeight: this.minimizedFrom.min, maximumHeight: this.minimizedFrom.max });
        p.api.setSize({ height: this.minimizedFrom.height });
        this.minimizedFrom = null;
        min.innerHTML = ICON.minimize;
        min.title = 'Minimize';
      }
    });
    button(ICON.maximize, 'Maximize', () => (p.api.isMaximized() ? p.api.exitMaximized() : p.api.maximize()));
    button(ICON.float, 'Float', () => p.containerApi.addFloatingGroup(p.group as DockviewGroupPanel));
  }
  dispose() {}
}

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

  let timer: ReturnType<typeof setTimeout>;
  const layoutSub = a.onDidLayoutChange(() => {
    refreshOpen();
    clearTimeout(timer);
    timer = setTimeout(() => {
      if (classroom.role === 'student') return; // the teacher's arrangement is not this person's own
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
  // In a class the teacher's arrangement of panels is mirrored: a different arrangement rebuilds the workspace,
  // sizes and the tab in front are matched without rebuilding (so the views keep their place).
  const offMirror = registerMirror('layout', {
    read: () => ({ layout: a.toJSON() }),
    write: (state) => {
      const layout = (state as { layout?: unknown }).layout;
      if (!layout) return;
      if (layoutShape(layout) !== layoutShape(a.toJSON())) {
        try {
          a.fromJSON(layout as Parameters<DockviewApi['fromJSON']>[0]);
        } catch {
          return; // a layout this build cannot read: keep the one on screen
        }
        refreshOpen();
        return;
      }
      for (const id of frontPanels(layout)) a.getPanel(id)?.api.setActive();
    },
  });

  return () => {
    offMirror();
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

/** The width of the workspace, for sizing panels as a share of it. */
function el0Width(): number {
  return document.querySelector('.workspace')?.clientWidth ?? window.innerWidth;
}
