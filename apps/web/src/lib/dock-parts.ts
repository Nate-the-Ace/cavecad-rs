// The two small pieces every dockable workspace needs: how a Svelte component is mounted into a dockview
// panel, and the minimize, maximize and float buttons at the right of each group's tab bar. Shared by the
// app's workspace (lib/workspace.svelte.ts) and the survey harness, so panels behave the same in both.
import type { DockviewGroupPanel, IContentRenderer, IHeaderActionsRenderer } from 'dockview-core';
import { mount, unmount, type Component } from 'svelte';

export class SveltePanel implements IContentRenderer {
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
  help: '<svg width="14" height="14" viewBox="0 0 14 14"><circle cx="7" cy="7" r="5.5" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M5.5 5.6a1.6 1.6 0 1 1 2.2 1.5c-.5.2-.7.5-.7 1v.4" stroke="currentColor" stroke-width="1.3" fill="none"/><circle cx="7" cy="10.2" r=".7" fill="currentColor"/></svg>',
  float: '<svg width="14" height="14" viewBox="0 0 14 14"><path d="M5 3h6v6M11 3L5 9M3 6v5h5" stroke="currentColor" stroke-width="1.4" fill="none"/></svg>',
};

/** Buttons shown at the right of every panel group's tab bar. */
export class GroupActions implements IHeaderActionsRenderer {
  readonly element = document.createElement('div');
  private minimizedFrom: { height: number; min: number; max: number } | null = null;
  private sub: { dispose(): void } | null = null;
  constructor(private readonly help?: { pageFor: (panelId: string) => string | null; open: (pageId: string) => void }) {
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
    if (this.help) {
      const help = this.help;
      let page: string | null = null;
      const h = button(ICON.help, 'Handbook page for this panel', () => page && help.open(page));
      const sync = () => {
        const id = p.group.activePanel?.id;
        page = id ? help.pageFor(id) : null;
        h.style.display = page ? '' : 'none';
      };
      sync();
      this.sub = p.group.api.onDidActivePanelChange(sync);
    }
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
  dispose() {
    this.sub?.dispose();
  }
}
