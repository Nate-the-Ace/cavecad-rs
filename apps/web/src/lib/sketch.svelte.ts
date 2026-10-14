// The sketchpad's shared state. The Plan, Profile and Cross section panes are separate dockable panels, so what
// they share lives here: the survey they draw (fed by whoever hosts them), the strokes, and each pane's frame and
// history. What a person does is NOT kept here: the running tool is `cave.tool`, the selected line `cave.selected`,
// the section's station `cave.station`, and the settings are `option()`: the same as in every other panel, so the
// ribbon's Draw tab, its options tab and its Edit tab drive the sketchpad as they drive the rest of the app.
// The geometry is in sketch-geometry.ts and the grid in sketch-grid.ts; both are pure and tested.

import { cave, changed, option } from './cave.svelte';
import { layout, planView, profileView, sectionView, type Anchored, type DimsByStation, type Pt, type SurveyShot, type Transform } from './sketch-geometry';
import { DEFAULT_FEET_PER_INCH, pxPerMetre } from './sketch-grid';

export type Pane = 'plan' | 'profile' | 'section';
export const PANES: Pane[] = ['plan', 'profile', 'section'];
export const PANE_TITLE: Record<Pane, string> = { plan: 'Plan', profile: 'Profile', section: 'Cross section' };
/** The undo scope of each pane: the app's names for these views (the plan is its Map). */
export const PANE_SCOPE: Record<Pane, 'map' | 'profile' | 'section'> = { plan: 'map', profile: 'profile', section: 'section' };
/** What the engine calls the view a line was drawn in. */
export const VIEW_NAME: Record<Pane, 'map' | 'profile' | 'section'> = PANE_SCOPE;

/** A drawn line: fit points hung on a station (see `Anchored`), plus what the app's line tabs know about it. */
export interface Stroke extends Anchored {
  id: string;
  view: Pane;
  /** For a cross section: the station it was drawn at. */
  at: string | null;
  closed: boolean;
  flip: boolean;
  text: string;
}

/** What is remembered for each pane between visits. */
export interface PaneView {
  /** Feet to the paper inch, or `fit` to show the whole survey. */
  scale: number | 'fit';
  /** `auto`: the pane frames itself (on the survey, or the station stood at). `free`: the person moved it. */
  frame: 'auto' | 'free';
  tf: Transform;
  size: { w: number; h: number };
}

const newView = (): PaneView => ({ scale: DEFAULT_FEET_PER_INCH, frame: 'auto', tf: { scale: pxPerMetre(DEFAULT_FEET_PER_INCH), tx: 150, ty: 150 }, size: { w: 0, h: 0 } });
let counter = 0;
const newId = () => `k${(++counter).toString(36)}${Math.random().toString(36).slice(2, 8)}`;

class Sketch {
  /** Key for what is kept on this device. */
  key = 'default';
  shots = $state<SurveyShot[]>([]);
  dims = $state<DimsByStation>({});
  standing = $state<string | null>(null);
  strokes = $state<Stroke[]>([]);
  history = $state<Record<Pane, { undo: string[]; redo: string[] }>>({ plan: { undo: [], redo: [] }, profile: { undo: [], redo: [] }, section: { undo: [], redo: [] } });
  /** Per pane, kept here so a pane that is closed and opened again comes back as it was. */
  views = $state<Record<Pane, PaneView>>({ plan: newView(), profile: newView(), section: newView() });

  L = $derived(layout(this.shots));
  plan = $derived(planView(this.L, this.dims));
  profile = $derived(profileView(this.L, this.dims));
  /** The station the cross section shows: the one stood at while following, else the one picked (`cave.station`), else the last on the chain. */
  selected = $derived<string | null>(
    option('followSurvey', true) && this.standing && this.L.stations[this.standing]
      ? this.standing
      : cave.station && this.L.stations[cave.station]
        ? cave.station
        : (this.L.chain[this.L.chain.length - 1] ?? null),
  );
  section = $derived(this.selected ? sectionView(this.L, this.dims, this.selected) : null);

  /** The survey to draw (called by the host whenever the notebook changes). */
  setSource(shots: SurveyShot[], dims: DimsByStation, standing: string | null) {
    this.shots = shots;
    this.dims = dims;
    this.standing = standing;
  }

  /** The stations a pane's strokes hang on, in that pane's own metres. */
  stationsOf(pane: Pane): { label: string; p: Pt }[] {
    return pane === 'plan' ? this.plan.stations : pane === 'profile' ? this.profile.stations : this.selected ? [{ label: this.selected, p: [0, 0] as Pt }] : [];
  }

  /** The strokes a pane shows (a section shows those of the station it is at). */
  paneStrokes(pane: Pane): Stroke[] {
    return this.strokes.filter((s) => s.view === pane && (pane !== 'section' || s.at === this.selected));
  }

  find(id: string): Stroke | undefined {
    return this.strokes.find((s) => s.id === id);
  }

  // ---- keeping ----

  /** Reads what was kept on this device for `key`. */
  open(key: string) {
    this.key = key;
    try {
      const raw = JSON.parse(localStorage.getItem(`cavecad.sketch.v2.${key}`) ?? 'null');
      if (Array.isArray(raw)) this.strokes = raw as Stroke[];
      const scales = JSON.parse(localStorage.getItem(`cavecad.sketch.v2.scales.${key}`) ?? 'null');
      if (scales) for (const p of PANES) if (scales[p] === 'fit' || typeof scales[p] === 'number') this.views[p].scale = scales[p];
    } catch {
      /* nothing kept, or storage unavailable */
    }
  }

  save() {
    try {
      localStorage.setItem(`cavecad.sketch.v2.${this.key}`, JSON.stringify($state.snapshot(this.strokes)));
      localStorage.setItem(`cavecad.sketch.v2.scales.${this.key}`, JSON.stringify(Object.fromEntries(PANES.map((p) => [p, this.views[p].scale]))));
    } catch {
      /* the sketch just is not kept */
    }
  }

  // ---- editing: every change is one undoable step in its pane's own history ----

  private snapshot(pane: Pane): string {
    return JSON.stringify($state.snapshot(this.strokes.filter((s) => s.view === pane)));
  }

  private apply(pane: Pane, change: () => void) {
    const before = this.snapshot(pane);
    change();
    if (this.snapshot(pane) === before) return;
    const h = this.history[pane];
    h.undo.push(before);
    if (h.undo.length > 100) h.undo.shift();
    h.redo = [];
    this.save();
    changed();
  }

  /** Adds a drawn line, already hung on its station (see `anchorStroke`), to a pane. Returns its id. */
  add(pane: Pane, line: Anchored, closed: boolean): string {
    const id = newId();
    this.apply(pane, () => {
      this.strokes = [...this.strokes, { ...line, id, view: pane, at: pane === 'section' ? this.selected : null, closed, flip: false, text: '' }];
    });
    return id;
  }

  /** Moves a line's fit points (world metres of its pane), keeping it hung on the station it was. */
  setPoints(id: string, world: Pt[], widths: number[] | null) {
    const s = this.find(id);
    if (!s) return;
    const st = this.stationsOf(s.view).find((x) => x.label === s.anchor);
    if (!st) return;
    this.apply(s.view, () => {
      this.strokes = this.strokes.map((k) => (k.id === id ? { ...k, pts: world.map((p) => [p[0] - st.p[0], p[1] - st.p[1]] as Pt), widths } : k));
    });
  }

  /** Changes one thing about a line (what it is, closed, flip side, words). */
  patch(id: string, change: Partial<Pick<Stroke, 'kind' | 'closed' | 'flip' | 'text'>>) {
    const s = this.find(id);
    if (!s) return;
    this.apply(s.view, () => {
      this.strokes = this.strokes.map((k) => (k.id === id ? { ...k, ...change } : k));
    });
  }

  remove(id: string) {
    const s = this.find(id);
    if (!s) return;
    this.apply(s.view, () => {
      this.strokes = this.strokes.filter((k) => k.id !== id);
    });
  }

  // ---- history (what `engine.undo`, `redo` and `editState` answer for a scope) ----

  paneOf(scope: string): Pane | null {
    return PANES.find((p) => PANE_SCOPE[p] === scope) ?? null;
  }

  can(pane: Pane): { undo: boolean; redo: boolean } {
    return { undo: this.history[pane].undo.length > 0, redo: this.history[pane].redo.length > 0 };
  }

  step(pane: Pane, from: 'undo' | 'redo'): boolean {
    const stack = this.history[pane][from];
    const text = stack.pop();
    if (text === undefined) return false;
    this.history[pane][from === 'undo' ? 'redo' : 'undo'].push(this.snapshot(pane));
    const back = JSON.parse(text) as Stroke[];
    this.strokes = [...this.strokes.filter((s) => s.view !== pane), ...back];
    this.save();
    return true;
  }

  /** The cross section's neighbour along the chain, for the section's previous and next buttons. */
  neighbour(by: number): string | null {
    const chain = this.L.chain;
    if (chain.length === 0) return null;
    const i = Math.max(0, chain.indexOf(this.selected ?? ''));
    return chain[Math.min(chain.length - 1, Math.max(0, i + by))];
  }
}

export const sketch = new Sketch();
