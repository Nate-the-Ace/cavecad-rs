// The 3D canvas: camera, orbit controls and drawing of a cave scene. Z is up, y is north.
// It knows nothing about documents: it receives a `Scene` from the Rust engine and draws it.
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { LineSegments2 } from 'three/addons/lines/LineSegments2.js';
import { LineSegmentsGeometry } from 'three/addons/lines/LineSegmentsGeometry.js';
import { LineMaterial } from 'three/addons/lines/LineMaterial.js';
import type { Scene } from '../engine';
import { elevationColor, readTheme, type ViewTheme } from './theme';

export type Projection = 'perspective' | 'orthographic';
export type Dir = [number, number, number];

const FOV = 45;
/** Default "home" view: from the south-east and above. */
export const HOME_DIR: Dir = [1, -1.2, 0.9];

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
    this.controls = new OrbitControls(this.persp, canvas);
    this.controls.screenSpacePanning = true;
    this.controls.addEventListener('change', () => this.requestRender());
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

  /** Jumps (no animation) to the home angle and frames the cave. Used when a cave first opens. */
  resetView() {
    const dist = Math.max(this.cameraOffset.length(), 1);
    this.camera.position.copy(this.controls.target).addScaledVector(viewDirection(HOME_DIR), dist);
    this.controls.update();
    this.fit();
  }

  home() {
    this.snapTo(HOME_DIR);
  }

  // ---- content -----------------------------------------------------------------------------

  setElevationColors(on: boolean) {
    this.elevationColors = on;
    this.rebuild();
  }

  setScene(scene: Scene | null, refit = true) {
    this.data = scene;
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

    const lines = (segs: number[][], color: THREE.Color, width: number, opts: { dashed?: boolean; opacity?: number } = {}) => {
      if (segs.length === 0) return;
      const geo = new LineSegmentsGeometry();
      geo.setPositions(segs.flatMap((p) => p));
      const mat = new LineMaterial({ color: this.elevationColors ? 0xffffff : color, linewidth: width, dashed: !!opts.dashed, transparent: opts.opacity !== undefined, opacity: opts.opacity ?? 1, vertexColors: this.elevationColors, dashSize: 1, gapSize: 0.6 });
      mat.resolution.set(this.width, this.height);
      if (this.elevationColors) geo.setColors(segs.flatMap((p) => [...shade(p[2]).toArray(), ...shade(p[5]).toArray()]));
      const mesh = new LineSegments2(geo, mat);
      mesh.computeLineDistances();
      this.content.add(mesh);
      this.materials.push(mat);
    };
    const legs = (style: string) => s.legs.filter((l) => l.style === style).map((l) => [...l.from, ...l.to]);
    lines(s.splays, t.muted, 1, { opacity: 0.55 });
    lines(legs('new'), t.text, 2.2);
    lines(legs('tie'), t.warn, 2, { dashed: true });
    lines(legs('closure'), t.accent, 2, { dashed: true });

    const pos = new Float32Array(s.stations.length * 3);
    const col = new Float32Array(s.stations.length * 3);
    s.stations.forEach((st, i) => {
      pos.set(st.at, i * 3);
      const c = st.anchor ? t.accent : this.elevationColors ? shade(st.at[2]) : t.text;
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

    if (pxPerM > 3) {
      g.fillStyle = css.muted;
      for (const st of s.stations) {
        const p = this.project(st.at);
        if (p && p[0] > -20 && p[1] > -20 && p[0] < this.width + 20 && p[1] < this.height + 20) g.fillText(st.label, p[0] + 6, p[1] - 5);
      }
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
      const metres = niceLength(110 / pxPerM);
      g.strokeStyle = css.text;
      g.fillStyle = css.text;
      g.lineWidth = 2;
      g.beginPath();
      g.moveTo(16, this.height - 18);
      g.lineTo(16 + metres * pxPerM, this.height - 18);
      g.stroke();
      g.fillText(`${metres} m`, 16, this.height - 24);
    }
  }

  dispose() {
    cancelAnimationFrame(this.pending);
    cancelAnimationFrame(this.anim);
    this.controls.dispose();
    this.renderer.dispose();
  }
}

function niceLength(target: number): number {
  const p = Math.pow(10, Math.floor(Math.log10(target)));
  const m = target / p;
  return (m >= 5 ? 5 : m >= 2 ? 2 : 1) * p;
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
