// The dockable workspace: panels you can drag, resize, tab together, float, maximize, minimize,
// hide and bring back. Layout is remembered in the browser; a layout that no longer fits the
// current panels is discarded rather than trusted.
// The all-in-one bundle is used on purpose: it is the only build that carries dockview's styles.
import { createDockview, themeDark, themeLight } from 'dockview-core/dist/dockview-core.js';
import { snapEdge, transpose, type Grid } from './dock-tidy';
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
import PatternsPanel from '../panels/PatternsPanel.svelte';
import RoomsPanel from '../panels/RoomsPanel.svelte';
import HandbookPanel from '../panels/HandbookPanel.svelte';
import { showInstruments } from './features';

const LAYOUT_KEY = 'cavecad.layout.v1';

interface PanelDef {
  title: string;
  component: Component;
  /** Height in pixels when first opened below another panel. */
  height?: number;
  /** Width in pixels when first opened beside another panel. */
  width?: number;
  /** When closed and opened, join the group holding this panel as a tab, if it is open. */
  openWithin?: string;
}

/** Every panel the workspace can show. Add new panels here. */
export const PANELS: Record<string, PanelDef> = {
  map: { title: 'Map', component: MapPanel },
  profile: { title: 'Profile', component: ProfilePanel, width: 460 },
  section: { title: 'Cross Section', component: SectionPanel, width: 460 },
  symbols: { title: 'Symbols', component: SymbolsPanel, width: 300 },
  stations: { title: 'Stations', component: StationsPanel, height: 260 },
  notebook: { title: 'Notebook', component: NotebookPanel, height: 320 },
  // instrument work is not ready yet: a saved layout that holds the Survey panel is dropped for the default
  ...(showInstruments ? { survey: { title: 'Survey', component: SurveyPanel, width: 380 } } : {}),
  scans: { title: 'Scans', component: ScansPanel, width: 520 },
  sheet: { title: 'Sheet', component: SheetPanel, width: 520 },
  patterns: { title: 'Patterns', component: PatternsPanel, width: 560 },
  console: { title: 'Console', component: ConsolePanel, height: 260 },
  rooms: { title: 'Rooms', component: RoomsPanel, width: 460, height: 360 },
  handbook: { title: 'Handbook', component: HandbookPanel, width: 520, openWithin: 'profile' },
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

/**
 * Flips a plain grid of panels the other way round (rows to columns, or columns to rows; see dock-tidy.ts), so the
 * edge that ran right across it is split into one edge per row or column. True when it did.
 */
function flip(a: DockviewApi): boolean {
  const json = a.toJSON();
  const grid = transpose(json.grid as unknown as Grid);
  if (!grid) return false;
  try {
    a.fromJSON({ ...json, grid: grid as unknown as typeof json.grid });
    return true;
  } catch {
    try {
      a.fromJSON(json);
    } catch {
      /* the dock keeps what it has */
    }
    return false;
  }
}

/**
 * Edge snapping: while an edge between panels is dragged, a line shows when it comes near another panel edge
 * running the same way (in the column beside it, or the window's own edge); let go there and it lines up exactly.
 */
function snapEdges(el: HTMLElement, a: DockviewApi): () => void {
  const guide = document.createElement('div');
  guide.style.cssText = 'position:fixed;pointer-events:none;z-index:50;background:var(--dv-active-sash-color,#0a84ff);display:none;';
  document.body.appendChild(guide);
  let drag: { sash: HTMLElement; across: boolean } | null = null; // across: the edge runs left-right and moves up-down
  const centre = (r: DOMRect, across: boolean) => (across ? r.top + r.height / 2 : r.left + r.width / 2);
  const target = (): { at: number; to: number; rect: DOMRect } | null => {
    if (!drag) return null;
    const rect = drag.sash.getBoundingClientRect();
    const at = centre(rect, drag.across);
    const others = [...el.querySelectorAll<HTMLElement>('.dv-sash')]
      .filter((s) => s !== drag!.sash && s.closest('.dv-split-view-container')?.classList.contains('dv-horizontal') === !drag!.across)
      .map((s) => centre(s.getBoundingClientRect(), drag!.across));
    const to = snapEdge(at, others);
    return { at, to, rect };
  };
  const show = () => {
    const t = target();
    if (!t || t.to === t.at || !drag) return void (guide.style.display = 'none');
    const box = el.getBoundingClientRect();
    guide.style.display = 'block';
    if (drag.across) Object.assign(guide.style, { left: `${box.left}px`, width: `${box.width}px`, top: `${t.to - 1}px`, height: '2px' });
    else Object.assign(guide.style, { top: `${box.top}px`, height: `${box.height}px`, left: `${t.to - 1}px`, width: '2px' });
  };
  let handing = false;
  const down = (e: PointerEvent) => {
    if (handing) return;
    let sash = (e.target as HTMLElement | null)?.closest?.('.dv-sash') as HTMLElement | null;
    if (!sash || !el.contains(sash) || e.button !== 0) return;
    // An edge running right across a grid of panels (the outermost split's) would drag every row or column with it:
    // flip the grid so this spot has an edge of its own, and hand the drag to that edge.
    const outer = sash.closest('.dv-split-view-container');
    if (outer && outer.parentElement?.closest('.dv-split-view-container, .dv-grid-view')?.classList.contains('dv-grid-view') && el.contains(outer) && transpose(a.toJSON().grid as unknown as Grid)) {
      const { clientX: x, clientY: y } = e;
      e.stopPropagation();
      e.preventDefault();
      if (!flip(a)) return;
      const fresh = [...el.querySelectorAll<HTMLElement>('.dv-sash')].find((s) => {
        const r = s.getBoundingClientRect();
        return x >= r.left - 3 && x <= r.right + 3 && y >= r.top - 3 && y <= r.bottom + 3;
      });
      if (!fresh) return;
      sash = fresh;
      handing = true;
      try {
        fresh.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, buttons: 1, pointerId: e.pointerId, pointerType: e.pointerType, isPrimary: true }));
      } finally {
        handing = false;
      }
    }
    drag = { sash, across: !sash.closest('.dv-split-view-container')?.classList.contains('dv-horizontal') };
  };
  const move = () => drag && requestAnimationFrame(show);
  const up = () => {
    const t = target();
    const was = drag;
    drag = null;
    guide.style.display = 'none';
    if (!t || !was || t.to === t.at) return;
    const delta = t.to - t.at;
    // The panel just before the edge (above it, or left of it) grows or shrinks by the snap.
    const before = a.groups.find((g) => {
      const r = g.element.getBoundingClientRect();
      return was.across
        ? Math.abs(r.bottom - t.rect.top - t.rect.height / 2) <= 4 && r.left < t.rect.right && r.right > t.rect.left
        : Math.abs(r.right - t.rect.left - t.rect.width / 2) <= 4 && r.top < t.rect.bottom && r.bottom > t.rect.top;
    });
    if (!before) return;
    const r = before.element.getBoundingClientRect();
    before.api.setSize(was.across ? { height: r.height + delta } : { width: r.width + delta });
  };
  el.addEventListener('pointerdown', down, true);
  window.addEventListener('pointermove', move, true);
  window.addEventListener('pointerup', up, true);
  return () => {
    el.removeEventListener('pointerdown', down, true);
    window.removeEventListener('pointermove', move, true);
    window.removeEventListener('pointerup', up, true);
    guide.remove();
  };
}

/** Builds the workspace inside `el`. Returns a function that tears it down. */
export function initWorkspace(el: HTMLElement, help?: ConstructorParameters<typeof GroupActions>[0]): () => void {
  api = createDockview(el, {
    theme: isDark() ? themeDark : themeLight,
    createComponent: ({ name }) => new SveltePanel(PANELS[name].component),
    createRightHeaderActionComponent: () => new GroupActions(help),
  });
  const a = api;
  if (import.meta.env.DEV) (window as unknown as { __dock: DockviewApi }).__dock = a; // for tests and debugging
  if (!restore(a)) defaultLayout(a);
  refreshOpen();
  const offSnap = snapEdges(el, a);
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
    offSnap();
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
    const within = PANELS[id].openWithin;
    const ref = api.panels[0];
    if (within && api.getPanel(within)) {
      api.addPanel({ id, component: id, title: PANELS[id].title, position: { referencePanel: within, direction: 'within' } });
    } else api.addPanel({ id, component: id, title: PANELS[id].title, position: ref ? { referencePanel: ref.id, direction: 'below' } : undefined, initialHeight: PANELS[id].height });
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
  if (!api || !showInstruments) return;
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
