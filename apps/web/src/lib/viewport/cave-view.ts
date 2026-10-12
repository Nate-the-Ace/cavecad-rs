// The 3D canvas: camera, orbit controls and drawing of a cave scene. Z is up, y is north.
// It knows nothing about documents: it receives a `Scene` from the Rust engine and draws it.
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { LineSegments2 } from 'three/addons/lines/LineSegments2.js';
import { LineSegmentsGeometry } from 'three/addons/lines/LineSegmentsGeometry.js';
import { LineMaterial } from 'three/addons/lines/LineMaterial.js';
import type { MapSection, PassageBuffers, ProfileSketch3d, Scene, TypeStyle } from '../engine';
import { defaultMapOffset, drawStrokes, hits } from '../section-glyph';
import { looks } from '../ink';
import { elevationColor, inkColor, readTheme, type ViewTheme } from './theme';
import { metresPer, niceNumber, unitName, type Units } from '../units';

export type Projection = 'perspective' | 'orthographic';
export type Dir = [number, number, number];

const FOV = 45;
/** Default "home" view: straight down (plan), north up. Every cave opens here. */
export const HOME_DIR: Dir = [0, 0, 1];

/** A tiny tilt so straight-down views keep north at the top of the screen. */
const TILT = 1e-3;

/** Camera direction (from the cave toward the camera) for a view-cube pick. */
export function viewDirection(d: Dir): THREE.Vector3 {
  const v = new THREE.Vector3(...d);
  if (d[0] === 0 && d[1] === 0) v.y = d[2] > 0 ? -TILT : TILT;
  return v.normalize();
}

export class CaveView {
  readonly renderer: THREE.WebGLRenderer;
  readonly controls: OrbitControls;
  private readonly scene = new THREE.Scene();
  private readonly persp = new THREE.PerspectiveCamera(FOV, 1, 0.01, 1e6);
  private readonly ortho = new THREE.OrthographicCamera(-1, 1, 1, -1, -1e5, 1e5);
  private camera: THREE.PerspectiveCamera | THREE.OrthographicCamera = this.persp;
  private content = new THREE.Group();
  private materials: LineMaterial[] = [];
  private data: Scene | null = null;
  private theme: ViewTheme = readTheme();
  private width = 1;
  private height = 1;
  private pending = 0;
  private anim = 0;
  private extent = 1;
  private elevationColors = false;
  private showWalls = true;
  private showLabels = true;
  private selected = '';
  /** Cross-section sketches drawn beside their stations, and whether they are shown. */
  private sections: MapSection[] = [];
  private showSections = true;
  /** Profile sketches in place along their runs; they stand in a vertical plane, so they fade out
   *  as the camera looks along it (straight down or up) and are hidden from the top or bottom. */
  private profileSketches: ProfileSketch3d[] = [];
  private showProfileSketches = true;
  /** Linework drawn in plan, lifted to the passage's elevation. */
  private mapLines: ProfileSketch3d[] = [];
  /** The drawing tool picked on the Draw tab (a palette type), or empty. */
  private drawTool = '';
  private liveLine: { pts: [number, number][]; widths: number[]; pressure: boolean; z: number } | null = null;
  /** Called with a plan stroke (east, north metres) when the person lifts the pen. */
  onLineDrawn: (points: [number, number][], widths: number[] | null) => void = () => {};
  /** Offsets being dragged right now, before they are saved. */
  private liveOffsets = new Map<string, [number, number, number]>();
  private dragging: { id: string; x: number; y: number } | null = null;
  /** Called when a section has been dropped where the person wants it. */
  onSectionPlaced: (id: string, offset: [number, number, number]) => void = () => {};
  private units: Units = 'metric';
  /** How each type looks; set once from the engine's palette. */
  private palette = new Map<string, TypeStyle>();
  private passage: THREE.Mesh | null = null;
  private showPassage = true;
  private readonly headlamp = new THREE.DirectionalLight(0xffffff, 2.2);
  /** Called after every render so the view cube can follow the camera. */
  onRender: (camera: THREE.Camera, target: THREE.Vector3) => void = () => {};
  onProjection: (p: Projection) => void = () => {};

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly overlay: HTMLCanvasElement,
  ) {
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setPixelRatio(window.devicePixelRatio || 1);
    this.persp.up.set(0, 0, 1);
    this.ortho.up.set(0, 0, 1);
    this.persp.position.copy(viewDirection(HOME_DIR).multiplyScalar(50));
    this.scene.add(this.content);
    this.scene.add(new THREE.AmbientLight(0xffffff, 0.9));
    this.scene.add(this.headlamp);
    this.controls = new OrbitControls(this.persp, canvas);
    this.controls.screenSpacePanning = true;
    // CAD mouse: middle drag pans, wheel zooms, left drag orbits (right drag pans too).
    this.controls.mouseButtons = { LEFT: THREE.MOUSE.ROTATE, MIDDLE: THREE.MOUSE.PAN, RIGHT: THREE.MOUSE.PAN };
    // Stops the browser's middle-click autoscroll from taking over the drag.
    // A double middle-click is Zoom Everything (frames the whole cave).
    let lastMiddle = { time: 0, x: 0, y: 0 };
    canvas.addEventListener('mousedown', (e) => {
      if (e.button !== 1) return;
      e.preventDefault();
      const quick = e.timeStamp - lastMiddle.time < 350 && Math.hypot(e.clientX - lastMiddle.x, e.clientY - lastMiddle.y) < 8;
      lastMiddle = { time: quick ? 0 : e.timeStamp, x: e.clientX, y: e.clientY };
      if (quick) this.fit();
    });
    this.controls.addEventListener('change', () => {
      this.requestRender();
      // Which lines run under another passage depends on where the camera is: ask again once it settles.
      clearTimeout(this.shadingTimer);
      this.shadingTimer = window.setTimeout(() => this.reshade() && this.rebuild(), 150);
    });
    // A section can be dragged by hand: this runs before the orbit controls see the press.
    canvas.addEventListener(
      'pointerdown',
      (e) => {
        if (e.button !== 0) return;
        if (this.drawTool) {
          // Drawing on the map: only from straight above or below, where plan is what is seen.
          if (!this.canDrawHere || e.pointerType === 'touch') return;
          const p = this.planPoint(e.clientX, e.clientY);
          if (!p) return;
          e.stopImmediatePropagation();
          e.stopPropagation();
          this.controls.enabled = false;
          canvas.setPointerCapture(e.pointerId);
          this.liveLine = { pts: [p], widths: [e.pointerType === 'pen' ? 0.4 + 1.2 * e.pressure : 1], pressure: e.pointerType === 'pen', z: this.controls.target.z };
          this.requestRender();
          return;
        }
        const id = this.sectionAt(e.clientX, e.clientY);
        if (!id) return;
        e.stopImmediatePropagation();
        e.stopPropagation();
        this.dragging = { id, x: e.clientX, y: e.clientY };
        this.controls.enabled = false;
        canvas.setPointerCapture(e.pointerId);
        canvas.style.cursor = 'grabbing';
      },
      true,
    );
    canvas.addEventListener('pointermove', (e) => {
      const line = this.liveLine;
      if (line) {
        for (const ev of e.getCoalescedEvents?.().length ? e.getCoalescedEvents() : [e]) {
          const p = this.planPoint(ev.clientX, ev.clientY);
          const last = line.pts[line.pts.length - 1];
          if (!p || (Math.hypot(p[0] - last[0], p[1] - last[1]) * (this.pixelsPerMetreAt([p[0], p[1], line.z]) ?? 1)) < 1.5) continue;
          line.pts.push(p);
          // Pressure only changes how thick the line is drawn, never where it goes.
          line.widths.push(line.pressure ? 0.4 + 1.2 * ev.pressure : 1);
        }
        this.requestRender();
        return;
      }
      const d = this.dragging;
      if (d) {
        this.moveSection(d.id, e.clientX - d.x, e.clientY - d.y);
        d.x = e.clientX;
        d.y = e.clientY;
      } else {
        canvas.style.cursor = this.sectionAt(e.clientX, e.clientY) ? 'grab' : '';
      }
    });
    const drop = () => {
      const line = this.liveLine;
      if (line) {
        this.liveLine = null;
        this.controls.enabled = true;
        if (line.pts.length >= 2) this.onLineDrawn(line.pts, line.pressure ? line.widths : null);
        this.requestRender();
        return;
      }
      const d = this.dragging;
      if (!d) return;
      this.dragging = null;
      this.controls.enabled = true;
      canvas.style.cursor = '';
      const offset = this.liveOffsets.get(d.id);
      if (offset) this.onSectionPlaced(d.id, offset);
    };
    canvas.addEventListener('pointerup', drop);
    canvas.addEventListener('pointercancel', drop);
    this.applyTheme();
  }

  /** Camera position relative to the target (for tests and the view cube). */
  get cameraOffset(): THREE.Vector3 {
    return this.camera.position.clone().sub(this.controls.target);
  }

  get projection(): Projection {
    return this.camera === this.persp ? 'perspective' : 'orthographic';
  }

  // ---- sizing ------------------------------------------------------------------------------

  resize(w: number, h: number) {
    this.width = Math.max(1, w);
    this.height = Math.max(1, h);
    this.renderer.setSize(this.width, this.height, false);
    this.canvas.style.width = `${this.width}px`;
    this.canvas.style.height = `${this.height}px`;
    const dpr = window.devicePixelRatio || 1;
    this.overlay.width = Math.round(this.width * dpr);
    this.overlay.height = Math.round(this.height * dpr);
    this.overlay.style.width = `${this.width}px`;
    this.overlay.style.height = `${this.height}px`;
    for (const m of this.materials) m.resolution.set(this.width, this.height);
    this.persp.aspect = this.width / this.height;
    this.persp.updateProjectionMatrix();
    this.setOrthoHeight(this.visibleHalfHeight());
    this.requestRender();
  }

  // ---- projection --------------------------------------------------------------------------

  /** Half the height of the world slice the screen currently shows, in metres. */
  private visibleHalfHeight(): number {
    if (this.camera === this.persp) {
      return this.persp.position.distanceTo(this.controls.target) * Math.tan((FOV * Math.PI) / 360);
    }
    return (this.ortho.top - this.ortho.bottom) / 2 / this.ortho.zoom;
  }

  private setOrthoHeight(half: number) {
    const aspect = this.width / this.height;
    this.ortho.left = -half * aspect;
    this.ortho.right = half * aspect;
    this.ortho.top = half;
    this.ortho.bottom = -half;
    this.ortho.zoom = 1;
    this.ortho.updateProjectionMatrix();
  }

  setProjection(kind: Projection) {
    if (kind === this.projection) return;
    const half = this.visibleHalfHeight();
    const target = this.controls.target.clone();
    const dir = this.camera.position.clone().sub(target).normalize();
    if (kind === 'orthographic') {
      this.setOrthoHeight(half);
      this.ortho.position.copy(target).addScaledVector(dir, Math.max(half * 4, this.extent));
      this.camera = this.ortho;
    } else {
      this.persp.position.copy(target).addScaledVector(dir, half / Math.tan((FOV * Math.PI) / 360));
      this.camera = this.persp;
    }
    this.controls.object = this.camera;
    this.controls.update();
    this.onProjection(kind);
    if (this.reshade()) this.rebuild();
    this.requestRender();
  }

  // ---- views -------------------------------------------------------------------------------

  /** Frames the whole cave, keeping the current viewing direction. */
  fit() {
    const b = this.data?.bounds;
    if (!b) return;
    const center = new THREE.Vector3((b[0] + b[3]) / 2, (b[1] + b[4]) / 2, (b[2] + b[5]) / 2);
    const radius = Math.max(0.5, Math.hypot(b[3] - b[0], b[4] - b[1], b[5] - b[2]) / 2);
    this.extent = radius * 2;
    const dir = this.camera.position.clone().sub(this.controls.target).normalize();
    this.controls.target.copy(center);
    if (this.camera === this.persp) {
      this.persp.position.copy(center).addScaledVector(dir, (radius / Math.sin((FOV * Math.PI) / 360)) * 1.1);
    } else {
      this.setOrthoHeight((radius * 1.15) / Math.min(1, this.width / this.height));
      this.ortho.position.copy(center).addScaledVector(dir, radius * 4);
    }
    this.persp.far = radius * 200;
    this.persp.near = radius / 500;
    this.persp.updateProjectionMatrix();
    this.controls.update();
    this.updateDashes();
    this.requestRender();
  }

  /** Swings the camera to look from `dir` (a view-cube pick). A flat face view goes orthographic. */
  snapTo(dir: Dir) {
    const flat = dir.filter((c) => c !== 0).length === 1;
    if (flat) this.setProjection('orthographic');
    const to = viewDirection(dir);
    const target = this.controls.target.clone();
    const from = this.camera.position.clone().sub(target);
    const dist = from.length();
    const start = from.clone().normalize();
    const turn = new THREE.Quaternion().setFromUnitVectors(start, to);
    const began = performance.now();
    cancelAnimationFrame(this.anim);
    const step = (now: number) => {
      const t = Math.min(1, (now - began) / 280);
      const eased = 1 - Math.pow(1 - t, 3);
      const q = new THREE.Quaternion().slerp(turn, eased);
      this.camera.position.copy(target).addScaledVector(start.clone().applyQuaternion(q), dist);
      this.controls.update();
      this.requestRender();
      if (t < 1) this.anim = requestAnimationFrame(step);
    };
    this.anim = requestAnimationFrame(step);
  }

  /** Turns the camera around the cave: `dAzimuth` about the vertical axis, `dPolar` toward or away
   *  from straight down. Radians. Used when the view cube is dragged. */
  orbitBy(dAzimuth: number, dPolar: number) {
    cancelAnimationFrame(this.anim);
    const offset = this.cameraOffset;
    const dist = offset.length();
    const azimuth = Math.atan2(offset.y, offset.x) - dAzimuth;
    const polar = Math.min(Math.PI - 1e-3, Math.max(1e-3, Math.acos(offset.z / dist) - dPolar));
    offset.set(Math.sin(polar) * Math.cos(azimuth), Math.sin(polar) * Math.sin(azimuth), Math.cos(polar)).multiplyScalar(dist);
    this.camera.position.copy(this.controls.target).add(offset);
    this.controls.update();
    this.requestRender();
  }

  /** Jumps (no animation) to the home view (Top, north up) and frames the cave. Used when a cave opens. */
  resetView() {
    // Plan view is orthographic: a measured drawing, not a perspective picture.
    this.setProjection('orthographic');
    const dist = Math.max(this.cameraOffset.length(), 1);
    this.camera.position.copy(this.controls.target).addScaledVector(viewDirection(HOME_DIR), dist);
    this.controls.update();
    this.fit();
  }

  home() {
    this.snapTo(HOME_DIR);
  }

  // ---- content -----------------------------------------------------------------------------

  /** The type palette: the one table that decides colour, weight and linetype. */
  setPalette(styles: TypeStyle[]) {
    this.palette = new Map(styles.map((s) => [s.key, s]));
    this.rebuild();
  }

  setElevationColors(on: boolean) {
    this.elevationColors = on;
    this.rebuild();
  }

  /** Shows the passage surface (or clears it with `null`). The buffers are flat triangles. */
  setPassage(buffers: PassageBuffers | null) {
    if (this.passage) {
      this.scene.remove(this.passage);
      this.passage.geometry.dispose();
      (this.passage.material as THREE.Material).dispose();
      this.passage = null;
    }
    if (buffers && buffers.positions.length > 0) {
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.BufferAttribute(buffers.positions, 3));
      geo.setAttribute('normal', new THREE.BufferAttribute(buffers.normals, 3));
      geo.setAttribute('color', new THREE.BufferAttribute(buffers.colors, 3));
      // Pushed slightly back so the wall outlines drawn on the same edges stay visible.
      const mat = new THREE.MeshStandardMaterial({ vertexColors: true, side: THREE.DoubleSide, roughness: 0.92, metalness: 0, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1 });
      this.passage = new THREE.Mesh(geo, mat);
      this.passage.visible = this.showPassage;
      this.scene.add(this.passage);
    }
    this.requestRender();
  }

  setShowPassage(on: boolean) {
    this.showPassage = on;
    if (this.passage) this.passage.visible = on;
    this.requestRender();
  }

  /** The drawing tool picked on the Draw tab (a palette type), or empty for none. */
  setDrawTool(key: string) {
    this.drawTool = key;
    this.canvas.style.cursor = key && this.canDrawHere ? 'crosshair' : '';
    this.requestRender();
  }

  /** True when the camera looks straight down or up, so a stroke on screen is a stroke in plan. */
  get canDrawHere(): boolean {
    return Math.abs(this.cameraOffset.clone().normalize().z) > 0.985;
  }

  /** The plan position (east, north) under a screen point, on the level of what the camera looks at. */
  private planPoint(clientX: number, clientY: number): [number, number] | null {
    const rect = this.canvas.getBoundingClientRect();
    const ndc = new THREE.Vector2(((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1);
    const ray = new THREE.Raycaster();
    ray.setFromCamera(ndc, this.camera);
    const hit = ray.ray.intersectPlane(new THREE.Plane(new THREE.Vector3(0, 0, 1), -this.controls.target.z), new THREE.Vector3());
    return hit ? [hit.x, hit.y] : null;
  }

  setMapLines(list: ProfileSketch3d[]) {
    this.mapLines = list;
    this.requestRender();
  }

  /** The cross-section sketches to draw beside their stations. */
  setSections(list: MapSection[]) {
    this.sections = list;
    this.liveOffsets.clear();
    this.requestRender();
  }

  setProfileSketches(list: ProfileSketch3d[]) {
    this.profileSketches = list;
    this.requestRender();
  }

  setShowProfileSketches(on: boolean) {
    this.showProfileSketches = on;
    this.requestRender();
  }

  /** How clearly a vertical drawing reads from here: 1 from the side, 0 from straight above or below. */
  get verticalDrawingVisibility(): number {
    const up = Math.abs(this.cameraOffset.clone().normalize().z);
    const SIDE = 0.5; // up to about 30 degrees of tilt reads fully
    const TOP = 0.93; // beyond about 21 degrees from vertical it is hidden
    return up <= SIDE ? 1 : up >= TOP ? 0 : (TOP - up) / (TOP - SIDE);
  }

  setShowSections(on: boolean) {
    this.showSections = on;
    this.requestRender();
  }

  private offsetOf(s: MapSection): [number, number, number] {
    return this.liveOffsets.get(s.id) ?? s.placement ?? defaultMapOffset(s.sketches);
  }

  /** Pixels per metre across the screen at a world point (it varies with depth in perspective). */
  private pixelsPerMetreAt(p: [number, number, number]): number | null {
    const a = this.project(p);
    const right = new THREE.Vector3().setFromMatrixColumn(this.camera.matrixWorld, 0);
    const b = this.project([p[0] + right.x, p[1] + right.y, p[2] + right.z]);
    return a && b ? Math.hypot(a[0] - b[0], a[1] - b[1]) : null;
  }

  private sectionAnchor(s: MapSection): { at: [number, number] | null; px: number | null } {
    const o = this.offsetOf(s);
    const world: [number, number, number] = [s.at[0] + o[0], s.at[1] + o[1], s.at[2] + o[2]];
    return { at: this.project(world), px: this.pixelsPerMetreAt(world) };
  }

  /** The section under a screen point, or null. */
  private sectionAt(clientX: number, clientY: number): string | null {
    if (!this.showSections) return null;
    const rect = this.canvas.getBoundingClientRect();
    const x = clientX - rect.left;
    const y = clientY - rect.top;
    for (const s of [...this.sections].reverse()) {
      const { at, px } = this.sectionAnchor(s);
      if (at && px && hits(s.sketches, at, px, x, y)) return s.id;
    }
    return null;
  }

  /** Moves a section by a screen drag, in the plane the camera is looking across. */
  private moveSection(id: string, dx: number, dy: number) {
    const s = this.sections.find((k) => k.id === id);
    if (!s) return;
    const { px } = this.sectionAnchor(s);
    if (!px) return;
    const right = new THREE.Vector3().setFromMatrixColumn(this.camera.matrixWorld, 0);
    const up = new THREE.Vector3().setFromMatrixColumn(this.camera.matrixWorld, 1);
    const o = this.offsetOf(s);
    const d = right.multiplyScalar(dx / px).addScaledVector(up, -dy / px);
    this.liveOffsets.set(id, [o[0] + d.x, o[1] + d.y, o[2] + d.z]);
    this.requestRender();
  }

  /** Strokes 3D lines (projected to the screen) in the look of their kind. */
  private strokeLines(g: CanvasRenderingContext2D, lines: ProfileSketch3d[]) {
    const look = looks();
    const base = g.globalAlpha;
    g.lineCap = 'round';
    g.lineJoin = 'round';
    for (const k of lines) {
      const l = look(k.kind);
      g.strokeStyle = l.color;
      g.globalAlpha = base * l.alpha;
      g.setLineDash(l.dash);
      const pts = k.points.map((p) => this.project(p));
      for (let i = 1; i < pts.length; i++) {
        const a = pts[i - 1];
        const b = pts[i];
        if (!a || !b) continue;
        g.lineWidth = l.weight * (k.widths ? (k.widths[i - 1] + k.widths[i]) / 2 : 1);
        g.beginPath();
        g.moveTo(a[0], a[1]);
        g.lineTo(b[0], b[1]);
        g.stroke();
      }
    }
    g.setLineDash([]);
    g.globalAlpha = base;
  }

  /** The station (id) to ring on the map, e.g. the one the Cross Section panel shows. */
  setSelected(id: string) {
    this.selected = id;
    this.requestRender();
  }

  /** The station under a screen point (within `radius` pixels), or null: what a click snaps to. */
  pickStation(clientX: number, clientY: number, radius = 14): { id: string; label: string } | null {
    const s = this.data;
    if (!s) return null;
    const rect = this.canvas.getBoundingClientRect();
    const x = clientX - rect.left;
    const y = clientY - rect.top;
    let best: { id: string; label: string } | null = null;
    let bestD = radius;
    for (const st of s.stations) {
      const p = this.project(st.at);
      if (!p) continue;
      const d = Math.hypot(p[0] - x, p[1] - y);
      if (d <= bestD) {
        bestD = d;
        best = { id: st.id, label: st.label };
      }
    }
    return best;
  }

  /** Units for the scale bar. */
  setUnits(u: Units) {
    this.units = u;
    this.requestRender();
  }

  /** Station labels follow the "label" type in the view filter. */
  setShowLabels(on: boolean) {
    this.showLabels = on;
    this.requestRender();
  }

  setShowWalls(on: boolean) {
    this.showWalls = on;
    this.rebuild();
  }

  /** Asks for the over/under split of the lines as seen from the current camera (the engine does the work). */
  shadingFor: ((view: [number, number, number], perspective: boolean) => Scene['shading']) | null = null;
  private shadingTimer = 0;
  private shadedFrom = new THREE.Vector3(0, 0, 1);
  private shadedPersp = false;

  /** Re-works the over/under split if the camera has moved; true when it changed. */
  private reshade(): boolean {
    if (!this.data || !this.shadingFor) return false;
    // A perspective camera sees along diverging rays, so what covers what depends on where the eye is;
    // an orthographic one only on the direction it looks from.
    const persp = this.camera === this.persp;
    const offset = this.cameraOffset;
    const key = persp ? this.camera.position.clone() : offset.clone().normalize();
    const moved = persp ? key.distanceTo(this.shadedFrom) > 0.01 * offset.length() : key.angleTo(this.shadedFrom) < 0.02 ? false : true;
    if (persp === this.shadedPersp && !moved) return false;
    this.shadedFrom.copy(key);
    this.shadedPersp = persp;
    this.data.shading = this.shadingFor([key.x, key.y, key.z], persp);
    return true;
  }

  setScene(scene: Scene | null, refit = true) {
    this.data = scene;
    this.shadedFrom.set(0, 0, 1); // a fresh scene is shaded for the view from above
    this.shadedPersp = false;
    if (!refit || this.camera === this.persp) this.reshade();
    this.rebuild();
    if (refit) this.resetView();
  }

  applyTheme() {
    this.theme = readTheme();
    this.renderer.setClearColor(this.theme.bg);
    this.rebuild();
  }

  private rebuild() {
    for (const child of [...this.content.children]) {
      this.content.remove(child);
      (child as THREE.Mesh).geometry?.dispose();
    }
    for (const m of this.materials) m.dispose();
    this.materials = [];
    const s = this.data;
    if (!s || s.stations.length === 0) return this.requestRender();

    const zmin = s.bounds?.[2] ?? 0;
    const zspan = Math.max(1e-6, (s.bounds?.[5] ?? 0) - zmin);
    const shade = (z: number) => elevationColor((z - zmin) / zspan);
    const t = this.theme;

    // Draws segments in the look the palette gives their type; "under" lines are dim and dashed.
    const lines = (segs: number[][], type: string, under = false) => {
      if (segs.length === 0) return;
      const st = this.palette.get(type);
      if (!st) return;
      const opacity = under ? st.under_opacity : st.opacity;
      const geo = new LineSegmentsGeometry();
      geo.setPositions(segs.flatMap((p) => p));
      const mat = new LineMaterial({ color: this.elevationColors ? 0xffffff : inkColor(t, st.ink), linewidth: st.weight, dashed: under || st.linetype === 'dashed', transparent: opacity < 1, opacity, vertexColors: this.elevationColors, dashSize: 1, gapSize: 0.6 });
      mat.resolution.set(this.width, this.height);
      if (this.elevationColors) geo.setColors(segs.flatMap((p) => [...shade(p[2]).toArray(), ...shade(p[5]).toArray()]));
      const mesh = new LineSegments2(geo, mat);
      mesh.computeLineDistances();
      this.content.add(mesh);
      this.materials.push(mat);
    };
    const legs = (style: string) => s.legs.filter((l) => l.style === style).map((l) => [...l.from, ...l.to]);
    const walls = this.showWalls && s.walls.length > 0;
    // Walls are the drawing; once they are there the surveyed centreline steps back.
    // Lines under another passage are dim and dashed, so overlapping levels read apart.
    const sh = s.shading;
    const legType = walls ? 'centerline-behind' : 'centerline';
    lines(sh.splays_over, 'splay');
    lines(sh.splays_under, 'splay', true);
    lines(sh.legs_over, legType);
    lines(sh.legs_under, legType, true);
    if (walls) {
      lines(sh.walls_over, 'wall');
      lines(sh.walls_under, 'wall', true);
    }
    lines(legs('tie'), 'tie');
    lines(legs('closure'), 'closure');

    const pos = new Float32Array(s.stations.length * 3);
    const col = new Float32Array(s.stations.length * 3);
    s.stations.forEach((st, i) => {
      pos.set(st.at, i * 3);
      const c = st.anchor ? t.accent : this.elevationColors ? shade(st.at[2]) : inkColor(t, this.palette.get('station')?.ink ?? 'text');
      col.set(c.toArray(), i * 3);
    });
    const pgeo = new THREE.BufferGeometry();
    pgeo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    pgeo.setAttribute('color', new THREE.BufferAttribute(col, 3));
    this.content.add(new THREE.Points(pgeo, new THREE.PointsMaterial({ size: 6, sizeAttenuation: false, vertexColors: true, map: dotTexture(), alphaTest: 0.5, transparent: true })));
    this.updateDashes();
    this.requestRender();
  }

  private updateDashes() {
    const dash = Math.max(this.extent / 120, 0.05);
    for (const m of this.materials) {
      m.dashSize = dash;
      m.gapSize = dash * 0.6;
    }
  }

  // ---- drawing -----------------------------------------------------------------------------

  requestRender() {
    if (!this.pending) this.pending = requestAnimationFrame(() => ((this.pending = 0), this.render()));
  }

  private render() {
    // A lamp on the camera lights whatever you are looking at, from any angle.
    this.headlamp.position.copy(this.camera.position);
    this.headlamp.target.position.copy(this.controls.target);
    this.headlamp.target.updateMatrixWorld();
    this.renderer.render(this.scene, this.camera);
    this.drawOverlay();
    this.onRender(this.camera, this.controls.target);
  }

  private project(p: number[]): [number, number] | null {
    const v = new THREE.Vector3(p[0], p[1], p[2]).project(this.camera);
    if (v.z > 1 || v.z < -1) return null;
    return [(v.x * 0.5 + 0.5) * this.width, (-v.y * 0.5 + 0.5) * this.height];
  }

  /** Labels, loop-closure rings and the scale bar, drawn flat over the 3D picture. */
  private drawOverlay() {
    const g = this.overlay.getContext('2d')!;
    const dpr = window.devicePixelRatio || 1;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, this.width, this.height);
    const s = this.data;
    if (!s) return;
    const css = this.theme.css;
    const pxPerM = this.height / (2 * this.visibleHalfHeight());
    g.font = '11px system-ui, sans-serif';
    g.textAlign = 'left';

    if (pxPerM > 3 && this.showLabels) {
      g.fillStyle = css.muted;
      for (const st of s.stations) {
        const p = this.project(st.at);
        if (p && p[0] > -20 && p[1] > -20 && p[0] < this.width + 20 && p[1] < this.height + 20) g.fillText(st.label, p[0] + 6, p[1] - 5);
      }
    }
    const seen = this.verticalDrawingVisibility;
    if (this.showProfileSketches && seen > 0) {
      g.save();
      g.globalAlpha = seen;
      this.strokeLines(g, this.profileSketches);
      g.restore();
    }
    this.strokeLines(g, this.mapLines);
    const live = this.liveLine;
    if (live && live.pts.length > 1) {
      this.strokeLines(g, [{ id: '', kind: this.drawTool || 'sketch', run: '', points: live.pts.map((p) => [p[0], p[1], live.z] as [number, number, number]), widths: live.pressure ? live.widths : null }]);
    }
    if (this.showSections) {
      for (const sec of this.sections) {
        const st = this.project(sec.at);
        const { at, px } = this.sectionAnchor(sec);
        if (!at || !px) continue;
        if (st) {
          g.strokeStyle = css.muted;
          g.lineWidth = 1;
          g.setLineDash([4, 3]);
          g.beginPath();
          g.moveTo(st[0], st[1]);
          g.lineTo(at[0], at[1]);
          g.stroke();
          g.setLineDash([]);
        }
        drawStrokes(g, sec.sketches, (u, v) => [at[0] + u * px, at[1] - v * px], looks());
        g.fillStyle = css.muted;
        g.textAlign = 'left';
        g.fillText(sec.label, at[0] + 4, at[1] + 12);
      }
    }
    const chosen = this.selected ? s.stations.find((st) => st.id === this.selected) : undefined;
    const cp = chosen ? this.project(chosen.at) : null;
    if (cp) {
      g.strokeStyle = this.theme.css.text;
      g.lineWidth = 2;
      g.beginPath();
      g.arc(cp[0], cp[1], 9, 0, Math.PI * 2);
      g.stroke();
    }
    g.strokeStyle = css.danger;
    g.fillStyle = css.danger;
    g.lineWidth = 1.5;
    for (const k of s.closures) {
      const p = this.project(k.at);
      if (!p) continue;
      g.beginPath();
      g.arc(p[0], p[1], 7, 0, Math.PI * 2);
      g.stroke();
      if (k.percent !== null && pxPerM > 1.2) g.fillText(`${k.percent.toFixed(2)}%`, p[0] + 10, p[1] + 4);
    }
    if (this.projection === 'orthographic') {
      const per = metresPer(this.units); // metres in one display unit
      const length = niceNumber(110 / pxPerM / per);
      const metres = length * per;
      g.strokeStyle = css.text;
      g.fillStyle = css.text;
      g.lineWidth = 2;
      g.beginPath();
      g.moveTo(16, this.height - 18);
      g.lineTo(16 + metres * pxPerM, this.height - 18);
      g.stroke();
      g.fillText(`${length} ${unitName(this.units)}`, 16, this.height - 24);
    }
  }

  dispose() {
    cancelAnimationFrame(this.pending);
    cancelAnimationFrame(this.anim);
    this.controls.dispose();
    this.renderer.dispose();
  }
}

let dot: THREE.CanvasTexture | null = null;
function dotTexture(): THREE.CanvasTexture {
  if (dot) return dot;
  const c = document.createElement('canvas');
  c.width = c.height = 32;
  const g = c.getContext('2d')!;
  g.fillStyle = '#fff';
  g.beginPath();
  g.arc(16, 16, 15, 0, Math.PI * 2);
  g.fill();
  dot = new THREE.CanvasTexture(c);
  return dot;
}
