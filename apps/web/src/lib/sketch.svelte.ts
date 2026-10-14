// The sketchpad's shared state. The Plan, Profile and Cross section panels (and the tools panel) are separate
// dockable panels, so what they share lives here: the survey they draw (fed by whoever hosts them), the
// strokes, the tool, which station is picked, and each pane's own scale and undo history. The geometry is
// in sketch-geometry.ts and the scales in sketch-scale.ts; both are pure and tested.

import { layout, planView, profileView, sectionView, type Anchored, type DimsByStation, type SurveyShot, type Transform } from './sketch-geometry';
import { DEFAULT_SCALE } from './sketch-scale';

export type Pane = 'plan' | 'profile' | 'section';
export type Tool = 'wall' | 'ledge' | 'note' | 'erase' | 'pan';

export const PANES: Pane[] = ['plan', 'profile', 'section'];
export const PANE_TITLE: Record<Pane, string> = { plan: 'Plan', profile: 'Profile', section: 'Cross section' };

export interface Strokes {
  plan: Anchored[];
  profile: Anchored[];
  /** Per station. */
  section: Record<string, Anchored[]>;
}

/** What is remembered for each pane between visits: which scale it was on (`fit` or a preset id). */
export interface PaneView {
  scaleId: string;
  /** `auto`: the pane frames itself on the survey (or the station stood at). `free`: the person moved it. */
  frame: 'auto' | 'free';
  tf: Transform;
  size: { w: number; h: number };
}

const blank = (): Strokes => ({ plan: [], profile: [], section: {} });

class Sketch {
  /** Key for what is kept on this device. */
  key = 'default';
  shots = $state<SurveyShot[]>([]);
  dims = $state<DimsByStation>({});
  standing = $state<string | null>(null);
  tool = $state<Tool>('wall');
  followSurvey = $state(true);
  picked = $state<string | null>(null);
  strokes = $state<Strokes>(blank());
  history = $state<Record<Pane, { undo: string[]; redo: string[] }>>({ plan: { undo: [], redo: [] }, profile: { undo: [], redo: [] }, section: { undo: [], redo: [] } });
  /** Per pane, kept here so a pane that is closed and opened again comes back as it was. */
  views = $state<Record<Pane, PaneView>>({
    plan: { scaleId: DEFAULT_SCALE.id, frame: 'auto', tf: { scale: DEFAULT_SCALE.pxPerM, tx: 150, ty: 150 }, size: { w: 0, h: 0 } },
    profile: { scaleId: DEFAULT_SCALE.id, frame: 'auto', tf: { scale: DEFAULT_SCALE.pxPerM, tx: 150, ty: 150 }, size: { w: 0, h: 0 } },
    section: { scaleId: DEFAULT_SCALE.id, frame: 'auto', tf: { scale: DEFAULT_SCALE.pxPerM, tx: 150, ty: 150 }, size: { w: 0, h: 0 } },
  });

  L = $derived(layout(this.shots));
  plan = $derived(planView(this.L, this.dims));
  profile = $derived(profileView(this.L, this.dims));
  /** The station the cross section shows: the one stood at while following, else the one picked, else the last on the chain. */
  selected = $derived<string | null>(
    this.followSurvey && this.standing && this.L.stations[this.standing]
      ? this.standing
      : this.picked && this.L.stations[this.picked]
        ? this.picked
        : (this.L.chain[this.L.chain.length - 1] ?? null),
  );
  section = $derived(this.selected ? sectionView(this.L, this.dims, this.selected) : null);

  /** The survey to draw (called by the host whenever the notebook changes). */
  setSource(shots: SurveyShot[], dims: DimsByStation, standing: string | null) {
    this.shots = shots;
    this.dims = dims;
    this.standing = standing;
  }

  /** Reads what was kept on this device for `key`. */
  open(key: string) {
    this.key = key;
    try {
      const raw = JSON.parse(localStorage.getItem(`cavecad.sketch.${key}`) ?? 'null');
      if (raw && Array.isArray(raw.plan) && Array.isArray(raw.profile) && raw.section && typeof raw.section === 'object') this.strokes = raw as Strokes;
      const scales = JSON.parse(localStorage.getItem(`cavecad.sketch.scales.${key}`) ?? 'null');
      if (scales) for (const p of PANES) if (typeof scales[p] === 'string') this.views[p].scaleId = scales[p];
    } catch {
      /* nothing kept, or storage unavailable */
    }
  }

  save() {
    try {
      localStorage.setItem(`cavecad.sketch.${this.key}`, JSON.stringify($state.snapshot(this.strokes)));
      localStorage.setItem(`cavecad.sketch.scales.${this.key}`, JSON.stringify(Object.fromEntries(PANES.map((p) => [p, this.views[p].scaleId]))));
    } catch {
      /* the sketch just is not kept */
    }
  }

  strokesOf(pane: Pane): Anchored[] {
    return pane === 'section' ? (this.selected ? (this.strokes.section[this.selected] ?? []) : []) : this.strokes[pane];
  }

  snapshot(pane: Pane): string {
    return JSON.stringify($state.snapshot(pane === 'section' ? this.strokes.section : this.strokes[pane]));
  }

  /** Records that the pane changed (`before` is its snapshot from before). */
  remember(pane: Pane, before: string) {
    const h = this.history[pane];
    h.undo.push(before);
    if (h.undo.length > 100) h.undo.shift();
    h.redo = [];
    this.save();
  }

  restore(pane: Pane, from: 'undo' | 'redo') {
    const stack = this.history[pane][from];
    const text = stack.pop();
    if (text === undefined) return;
    this.history[pane][from === 'undo' ? 'redo' : 'undo'].push(this.snapshot(pane));
    const value = JSON.parse(text);
    if (pane === 'section') this.strokes.section = value;
    else this.strokes[pane] = value;
    this.save();
  }

  can(pane: Pane, which: 'undo' | 'redo'): boolean {
    return this.history[pane][which].length > 0;
  }

  /** Steps the cross section to the previous or next station along the chain. */
  step(by: number) {
    const chain = this.L.chain;
    if (chain.length === 0) return;
    const i = Math.max(0, chain.indexOf(this.selected ?? ''));
    this.picked = chain[Math.min(chain.length - 1, Math.max(0, i + by))];
    this.followSurvey = false;
  }
}

export const sketch = new Sketch();
