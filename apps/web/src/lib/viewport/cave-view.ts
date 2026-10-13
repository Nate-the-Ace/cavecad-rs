// The 3D canvas: camera, orbit controls and drawing of a cave scene. Z is up, y is north.
// It knows nothing about documents: it receives a `Scene` from the Rust engine and draws it.
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { LineSegments2 } from 'three/addons/lines/LineSegments2.js';
import { LineSegmentsGeometry } from 'three/addons/lines/LineSegmentsGeometry.js';
import { LineMaterial } from 'three/addons/lines/LineMaterial.js';
import type { MapSection, PassageBuffers, Pt, ProfileSketch3d, Scene, TypeStyle } from '../engine';
import { PenGuard, isEraser, widthFor } from '../pen';
import { drawStamp, drawStationMarker, hitsStamp, isStampKind, placement } from '../symbols';
import { drawMarks, isFillKind, marksFor } from '../fills';
import { affineWarp, warpGrid, type Warp } from '../scan-warp';
import { drawHalo, type HaloLine } from '../halo';
import { isKindHidden, ornamentFor, refreshFlipped } from '../section-glyph';
import { drawCursor } from '../presence-draw';
import { defaultMapOffset, drawHandles, drawStrokes, glyphCurves, hits, leaderEnd } from '../section-glyph';
import { distanceToCurve, fitStroke, nearestPoint, refit, smooth } from '../spline';
import { PICKUP_REACH, joinFit } from '../line-editor';
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
  private halo = true;
  private haloReachM = 5;
  private haloFade = 0.5;
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
  /** The line selected for editing (a feature id), shown with a halo and, for a map line, handles. */
  private selectedLine = '';
  /** A fit point of a map line being moved: which line, which point, and the line's points now. */
  private lineEdit: { id: string; index: number; pts: [number, number, number][]; stamp: boolean } | null = null;
  /** Called when a map line's fit points have been moved (plan metres) or a line is picked or put down. */
  onLineEdited: (id: string, points: [number, number][], widths: number[] | null) => void = () => {};
  onSelectLine: (id: string) => void = () => {};
  /** Called when a right click (not a right drag, which pans) asks to end the running command. */
  onEndCommand: () => void = () => {};
  private rightDown: { x: number; y: number } | null = null;
  /** Where the next stroke would carry an open line on from, on screen, while a tool waits. */
  private pickupAt: [number, number] | null = null;
  /** How a drawn stroke is turned into a curve: the fit tolerance in pixels, and whether pen pressure counts. */
  private drawOptions = { tolerancePx: 2.5, pressure: true, close: false, finger: true, symbolSize: 1.5, label: '' };
  /** Fingers on the map now: a second one means a gesture, so a stroke the first began is dropped. */
  private fingers = new Set<number>();
  setDrawOptions(o: { tolerancePx: number; pressure: boolean; close: boolean; finger: boolean; symbolSize: number; label: string }) {
    this.drawOptions = o;
  }
  /** Called with a plan stroke (east, north metres) when the person lifts the pen. */
  /** Centres the Map on a station (keeping the camera's distance and direction) and rings it. */
  focusStation(id: string) {
    const st = this.data?.stations.find((s) => s.id === id);
    if (!st) return;
    const offset = this.camera.position.clone().sub(this.controls.target);
    this.controls.target.set(st.at[0], st.at[1], st.at[2]);
    this.camera.position.copy(this.controls.target).add(offset);
    this.controls.update();
    this.setSelected(id);
  }
  /** Fits a map line again at another curve detail (1 to 10), as one edit. */
  refitLine(id: string, detail: number): boolean {
    const line = this.mapLines.find((k) => k.id === id);
    if (!line || isStampKind(line.kind) || line.points.length < 3) return false;
    const f = refit(line.points.map((p) => [p[0], p[1]] as [number, number]), line.widths, line.closed, detail);
    if (f.pts.length === line.points.length && f.pts.every((p, i) => p[0] === line.points[i][0] && p[1] === line.points[i][1])) return false;
    this.onLineEdited(id, f.pts, f.widths);
    return true;
  }
  /** Where this pointer is on the plan, in the map's metres: told to the others when sharing. */
  onCursor: (x: number, y: number) => void = () => {};
  private remoteCursors: { x: number; y: number; name: string; color: string }[] = [];
  /** Other people's pointers on the plan. */
  setRemoteCursors(list: { x: number; y: number; name: string; color: string }[]) {
    this.remoteCursors = list;
    this.requestRender();
  }
  /** The eraser end of a pen passed over a line: delete it. */
  onLineErased: (id: string) => void = () => {};
  private guard = new PenGuard();
  /** Told why a touch was ignored (a palm), so it can be shown. */
  onPalm: (why: string) => void = () => {};
  private erasing = false;
  onLineDrawn: (points: [number, number][], widths: number[] | null, closed: boolean, text?: string) => void = () => {};
  /** Offsets being dragged right now, before they are saved. */
  private liveOffsets = new Map<string, [number, number, number]>();
  private dragging: { id: string; x: number; y: number } | null = null;
  /** Called when a section has been dropped where the person wants it. */
  onSectionPlaced: (id: string, offset: [number, number, number]) => void = () => {};
  private units: Units = 'metric';
  /** How each type looks; set once from the engine's palette. */
  private palette = new Map<string, TypeStyle>();
  private passage: THREE.Mesh | null = null;
  private showPassage = false;
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
    canvas.addEventListener(
      'pointerdown',
      (e) => {
        if (e.pointerType === 'pen') {
          this.guard.pen(e);
          if (this.fingers.size) {
            // A pen landing means what a resting hand started was never a gesture.
            this.fingers.clear();
            this.liveLine = null;
            this.lineEdit = null;
            this.controls.enabled = true;
          }
          if (isEraser(e) && this.canDrawHere) {
            e.stopImmediatePropagation();
            e.stopPropagation();
            this.erasing = true;
            this.controls.enabled = false;
            this.canvas.setPointerCapture(e.pointerId);
            this.eraseAt(e);
          }
          return;
        }
        if (e.pointerType !== 'touch') return;
        if (!this.guard.touchDown(e)) {
          this.onPalm(this.guard.lastRejected);
          // A palm or a fist: keep it from the orbit controls too.
          e.stopImmediatePropagation();
          e.stopPropagation();
          return;
        }
        this.fingers.add(e.pointerId);
        if (this.fingers.size >= 2 && (this.liveLine || this.lineEdit)) {
          // A second finger: that was the start of a pinch or pan, not a line. The gesture goes to the orbit controls.
          this.liveLine = null;
          this.lineEdit = null;
          this.controls.enabled = true;
          this.requestRender();
        }
      },
      true,
    );
    const lift = (e: PointerEvent) => {
      if (e.pointerType === 'touch') {
        this.fingers.delete(e.pointerId);
        this.guard.touchUp(e);
      }
      if (e.pointerType === 'pen') {
        this.guard.pen(e);
        if (this.erasing) {
          this.erasing = false;
          this.controls.enabled = true;
        }
      }
    };
    canvas.addEventListener('pointerup', lift);
    canvas.addEventListener('pointercancel', (e) => {
      lift(e);
      if (e.pointerType === 'touch' && this.liveLine) {
        this.liveLine = null;
        this.controls.enabled = true;
        this.requestRender();
      }
    });
    // A right click ends the running command; a right drag still pans.
    canvas.addEventListener('pointerdown', (e) => {
      if (e.button === 2) this.rightDown = { x: e.clientX, y: e.clientY };
    });
    // (Decided on release, not on the context-menu event, which some systems send on press.)
    canvas.addEventListener('pointerup', (e) => {
      const d = this.rightDown;
      if (e.button !== 2 || !d) return;
      this.rightDown = null;
      if (this.drawTool && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 5) this.onEndCommand();
    });
    canvas.addEventListener('contextmenu', (e) => e.preventDefault());
    // A section can be dragged by hand: this runs before the orbit controls see the press.
    canvas.addEventListener(
      'pointerdown',
      (e) => {
        if (e.button !== 0) return;
        if (this.drawTool) {
          // Drawing on the map: only from straight above or below, where plan is what is seen.
          if (!this.canDrawHere || this.fingers.size > 1 || (e.pointerType === 'touch' && !this.drawOptions.finger)) return;
          const p = this.planPoint(e.clientX, e.clientY);
          if (!p) return;
          e.stopImmediatePropagation();
          e.stopPropagation();
          this.controls.enabled = false;
          canvas.setPointerCapture(e.pointerId);
          if (isStampKind(this.drawTool)) {
            // A symbol: press where it goes, drag to say which way is up and how big.
            this.liveLine = { pts: [p, [p[0], p[1]]], widths: [1, 1], pressure: false, z: this.controls.target.z };
            this.requestRender();
            return;
          }
          this.liveLine = { pts: [p], widths: [e.pointerType === 'pen' && this.drawOptions.pressure ? widthFor(e.pressure) : 1], pressure: e.pointerType === 'pen' && this.drawOptions.pressure, z: this.controls.target.z };
          this.requestRender();
          return;
        }
        const id = this.sectionAt(e.clientX, e.clientY);
        if (!id) {
          this.pressLine(e);
          return;
        }
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
      if (e.pointerType !== 'touch') {
        const cp = this.planPoint(e.clientX, e.clientY);
        if (cp) this.onCursor(cp[0], cp[1]);
      }
      if (e.pointerType === 'pen') {
        this.guard.pen(e);
        if (this.erasing) {
          this.eraseAt(e);
          return;
        }
      }
      const edit = this.lineEdit;
      if (edit) {
        const p = this.planPoint(e.clientX, e.clientY);
        if (p && edit.stamp && edit.index === 0) {
          // A symbol's insertion point carries the whole symbol, its size and turn handle with it.
          const dx = p[0] - edit.pts[0][0];
          const dy = p[1] - edit.pts[0][1];
          edit.pts = edit.pts.map((q) => [q[0] + dx, q[1] + dy, q[2]] as [number, number, number]);
        } else if (p) edit.pts[edit.index] = [p[0], p[1], edit.pts[edit.index][2]];
        this.requestRender();
        return;
      }
      const line = this.liveLine;
      if (line && isStampKind(this.drawTool)) {
        const p = this.planPoint(e.clientX, e.clientY);
        if (p) line.pts[1] = p;
        this.requestRender();
        return;
      }
      if (line) {
        for (const ev of e.getCoalescedEvents?.().length ? e.getCoalescedEvents() : [e]) {
          const p = this.planPoint(ev.clientX, ev.clientY);
          const last = line.pts[line.pts.length - 1];
          if (!p || (Math.hypot(p[0] - last[0], p[1] - last[1]) * (this.pixelsPerMetreAt([p[0], p[1], line.z]) ?? 1)) < 1.5) continue;
          line.pts.push(p);
          // Pressure only changes how thick the line is drawn, never where it goes.
          line.widths.push(line.pressure ? widthFor(ev.pressure) : 1);
        }
        this.requestRender();
        return;
      }
      if (this.drawTool && this.canDrawHere && !this.dragging) {
        const r = this.canvas.getBoundingClientRect();
        const hit = this.openEndNear(e.clientX - r.left, e.clientY - r.top);
        const next = hit ? hit.at : null;
        if ((next?.[0] ?? -1) !== (this.pickupAt?.[0] ?? -1) || (next?.[1] ?? -1) !== (this.pickupAt?.[1] ?? -1)) {
          this.pickupAt = next;
          this.requestRender();
        }
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
      const edit = this.lineEdit;
      if (edit) {
        this.lineEdit = null;
        this.controls.enabled = true;
        const was = this.mapLines.find((k) => k.id === edit.id);
        this.onLineEdited(edit.id, edit.pts.map((p) => [p[0], p[1]] as [number, number]), was?.widths ?? null);
        this.requestRender();
        return;
      }
      const line = this.liveLine;
      if (line) {
        this.liveLine = null;
        this.controls.enabled = true;
        if (isStampKind(this.drawTool) && line.pts.length === 2) {
          const a = this.project([line.pts[0][0], line.pts[0][1], line.z]);
          const b = this.project([line.pts[1][0], line.pts[1][1], line.z]);
          // Just a click: a symbol of the chosen size, pointing north.
          const tap = !a || !b || Math.hypot(a[0] - b[0], a[1] - b[1]) < 6;
          const p1: [number, number] = tap ? [line.pts[0][0], line.pts[0][1] + this.drawOptions.symbolSize] : line.pts[1];
          this.onLineDrawn([line.pts[0], p1], null, false, this.drawOptions.label);
          this.requestRender();
          return;
        }
        if (line.pts.length >= 2) {
          // A freehand stroke becomes a smooth curve through a few fit points.
          const px = this.pixelsPerMetreAt([line.pts[0][0], line.pts[0][1], line.z]) ?? 1;
          // Starting on the end of an open line of the same kind carries that line on.
          const first = this.project([line.pts[0][0], line.pts[0][1], line.z]);
          const pickup = first ? this.openEndNear(first[0], first[1]) : null;
          if (pickup) {
            const fit = fitStroke(line.pts, line.pressure ? line.widths : null, this.drawOptions.tolerancePx / px, false);
            const own = { id: pickup.line.id, points: pickup.line.points.map((p) => [p[0], p[1]] as [number, number]), widths: pickup.line.widths };
            const joined = joinFit(own, fit, pickup.atStart);
            this.onLineEdited(pickup.line.id, joined.pts, joined.widths);
          } else {
            const loop = this.drawOptions.close || isFillKind(this.drawTool); // an area fill is always a loop
            const fit = fitStroke(line.pts, line.pressure ? line.widths : null, this.drawOptions.tolerancePx / px, loop);
            if (isFillKind(this.drawTool) && fit.pts.length < 3) {
              this.onLineDrawn([], null, false); // too small to enclose anything
            } else {
              this.onLineDrawn(fit.pts, fit.widths, loop && fit.pts.length >= 3);
            }
          }
        }
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

  private readonly scans = new Map<string, { mesh: THREE.Mesh; url: string }>();
  private static readonly SCAN_MAX = 4096;

  /**
   * Draws scans placed under the map. Each is a picture whose pixels are carried to the map by
   * `matrix` (pixel from the top left, y down, to east and north in metres), shown at `opacity` and
   * at height `z`. Drawn before the lines and without depth, so the passage never hides it and the
   * survey is drawn over it.
   */
  setScans(list: { key: string; url: string; matrix: number[]; z: number; opacity: number; trim?: [number, number][] | null; warp?: Warp | null; plane?: [number, number, number] | null; height?: ((x: number, y: number) => number) | null }[]) {
    const keep = new Set(list.map((l) => l.key));
    for (const [key, s] of this.scans) {
      if (!keep.has(key)) this.removeScan(key, s);
    }
    for (const item of list) {
      const have = this.scans.get(item.key);
      const sig = item.url + '|' + (item.trim ? item.trim.join(';') : '');
      if (have && have.url === sig) {
        this.placeScan(have.mesh, item);
        continue;
      }
      if (have) this.removeScan(item.key, have);
      new THREE.TextureLoader().load(item.url, (loaded) => {
        if (this.scans.has(item.key) || !keep.has(item.key)) {
          loaded.dispose();
          return;
        }
        const img = loaded.image as CanvasImageSource & { width: number; height: number };
        const full = { x0: 0, y0: 0, w: img.width as number, h: img.height as number };
        let tex: THREE.Texture = loaded;
        let box = full;
        if (item.trim && item.trim.length >= 3) {
          // Keep only the traced outline: its bounding box is drawn with everything outside it clear.
          const xs = item.trim.map((p) => p[0]);
          const ys = item.trim.map((p) => p[1]);
          const x0 = Math.max(0, Math.floor(Math.min(...xs)));
          const y0 = Math.max(0, Math.floor(Math.min(...ys)));
          const w = Math.max(1, Math.min(full.w, Math.ceil(Math.max(...xs))) - x0);
          const h = Math.max(1, Math.min(full.h, Math.ceil(Math.max(...ys))) - y0);
          const k = Math.min(1, CaveView.SCAN_MAX / Math.max(w, h));
          const canvas = document.createElement('canvas');
          canvas.width = Math.max(1, Math.round(w * k));
          canvas.height = Math.max(1, Math.round(h * k));
          const g = canvas.getContext('2d')!;
          g.scale(k, k);
          g.translate(-x0, -y0);
          g.beginPath();
          item.trim.forEach((p, i) => (i ? g.lineTo(p[0], p[1]) : g.moveTo(p[0], p[1])));
          g.closePath();
          g.clip();
          g.drawImage(img, 0, 0);
          loaded.dispose();
          tex = new THREE.CanvasTexture(canvas);
          box = { x0, y0, w, h };
        }
        tex.colorSpace = THREE.SRGBColorSpace;
        tex.anisotropy = 4;
        const { x0, y0, w, h } = box;
        const geo = new THREE.BufferGeometry();
        geo.setAttribute('position', new THREE.BufferAttribute(new Float32Array([x0, y0, 0, x0 + w, y0, 0, x0 + w, y0 + h, 0, x0, y0 + h, 0]), 3));
        geo.setAttribute('uv', new THREE.BufferAttribute(new Float32Array([0, 1, 1, 1, 1, 0, 0, 0]), 2));
        geo.setIndex([0, 1, 2, 0, 2, 3]);
        const mat = new THREE.MeshBasicMaterial({ map: tex, transparent: true, depthTest: false, depthWrite: false, side: THREE.DoubleSide });
        const mesh = new THREE.Mesh(geo, mat);
        mesh.userData.box = box;
        mesh.userData.flat = geo;
        mesh.matrixAutoUpdate = false;
        mesh.renderOrder = -10;
        mesh.frustumCulled = false;
        this.scans.set(item.key, { mesh, url: sig });
        this.placeScan(mesh, item);
        this.scene.add(mesh);
        this.requestRender();
      });
    }
    this.requestRender();
  }

  private placeScan(mesh: THREE.Mesh, item: { matrix: number[]; z: number; opacity: number; warp?: Warp | null; plane?: [number, number, number] | null; height?: ((x: number, y: number) => number) | null }) {
    const m = item.matrix;
    const flat = mesh.userData.flat as THREE.BufferGeometry;
    // The scan lies in the plane z = a + b x + c y through its stations (flat at `z` when it has no plane).
    const [pa, pb, pc] = item.plane ?? [item.z, 0, 0];
    if (item.warp || item.height) {
      // A rubber sheet, or a scan laid over ground that rises and falls: a grid over the picture, each node where the warp puts it, so the matrix is only the height.
      const box = mesh.userData.box as { x0: number; y0: number; w: number; h: number };
      const grid = item.warp ? warpGrid(item.warp, box) : warpGrid(affineWarp(m), box);
      const pos = new Float32Array(grid.dst.length * 3);
      const uv = new Float32Array(grid.dst.length * 2);
      grid.dst.forEach((d, i) => {
        pos.set([d[0], d[1], item.height ? item.height(d[0], d[1]) : pa + pb * d[0] + pc * d[1]], i * 3);
        uv.set([(grid.src[i][0] - box.x0) / box.w, 1 - (grid.src[i][1] - box.y0) / box.h], i * 2);
      });
      const index: number[] = [];
      for (let j = 0; j < grid.rows; j++) {
        for (let i = 0; i < grid.cols; i++) {
          const a = j * (grid.cols + 1) + i;
          index.push(a, a + 1, a + grid.cols + 1, a + 1, a + grid.cols + 2, a + grid.cols + 1);
        }
      }
      const g = new THREE.BufferGeometry();
      g.setAttribute('position', new THREE.BufferAttribute(pos, 3));
      g.setAttribute('uv', new THREE.BufferAttribute(uv, 2));
      g.setIndex(index);
      if (mesh.geometry !== flat) mesh.geometry.dispose();
      mesh.geometry = g;
      mesh.matrix.identity();
    } else {
      if (mesh.geometry !== flat) {
        mesh.geometry.dispose();
        mesh.geometry = flat;
      }
      mesh.matrix.set(m[0], m[1], 0, m[2], m[3], m[4], 0, m[5], pb * m[0] + pc * m[3], pb * m[1] + pc * m[4], 0, pa + pb * m[2] + pc * m[5], 0, 0, 0, 1);
    }
    mesh.matrixWorldNeedsUpdate = true;
    (mesh.material as THREE.MeshBasicMaterial).opacity = item.opacity;
    this.requestRender();
  }

  private removeScan(key: string, s: { mesh: THREE.Mesh }) {
    this.scene.remove(s.mesh);
    s.mesh.geometry.dispose();
    (s.mesh.userData.flat as THREE.BufferGeometry | undefined)?.dispose();
    const mat = s.mesh.material as THREE.MeshBasicMaterial;
    mat.map?.dispose();
    mat.dispose();
    this.scans.delete(key);
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

  /** The open map line of the running tool's kind with an end near screen (`x`, `y`): which, which end, where. */
  private openEndNear(x: number, y: number): { line: ProfileSketch3d; atStart: boolean; at: [number, number] } | null {
    let best: { line: ProfileSketch3d; atStart: boolean; at: [number, number] } | null = null;
    let bestD = PICKUP_REACH;
    for (const k of this.mapLines) {
      if (k.closed || k.kind !== this.drawTool || k.points.length < 2 || isStampKind(k.kind)) continue;
      for (const atStart of [false, true]) {
        const at = this.project(atStart ? k.points[0] : k.points[k.points.length - 1]);
        if (!at) continue;
        const d = Math.hypot(at[0] - x, at[1] - y);
        if (d <= bestD) {
          bestD = d;
          best = { line: k, atStart, at };
        }
      }
    }
    return best;
  }

  /** The line selected for editing (empty for none), from the shared selection. */
  setSelectedLine(id: string) {
    this.selectedLine = id;
    this.requestRender();
  }

  /** True when `id` is a line drawn on the map (the only kind edited here). */
  hasMapLine(id: string): boolean {
    return this.mapLines.some((k) => k.id === id);
  }

  /** The map lines to draw: the one being edited shows its live points. */
  private displayMapLines(): ProfileSketch3d[] {
    const e = this.lineEdit;
    return this.mapLines.map((k) => (e && k.id === e.id ? { ...k, points: e.pts } : k));
  }

  /** A press in the Top or Bottom view with no tool: pick up a fit point of the selected map line, or select the line there. */
  private eraseAt(e: PointerEvent) {
    const rect = this.canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    for (const k of [...this.mapLines].reverse()) {
      if (isKindHidden(k.kind)) continue;
      const pts = this.screenPoints(k).filter((p): p is [number, number] => !!p);
      if (pts.length === k.points.length && (isStampKind(k.kind) ? hitsStamp(k.kind, k.text, pts[0], pts[1], x, y) : distanceToCurve(pts, x, y, k.closed) <= 7)) {
        this.mapLines = this.mapLines.filter((m) => m.id !== k.id);
        this.onLineErased(k.id);
        this.requestRender();
        return;
      }
    }
  }

  private pressLine(e: PointerEvent) {
    if (this.drawTool || !this.canDrawHere || e.button !== 0 || this.fingers.size > 1) return;
    const rect = this.canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const screen = (k: ProfileSketch3d) => this.screenPoints(k).filter((p): p is [number, number] => !!p);
    const sel = this.displayMapLines().find((k) => k.id === this.selectedLine);
    if (sel) {
      const i = nearestPoint(this.screenPoints(sel).map((p) => p ?? [-1e9, -1e9]), x, y, 10);
      if (i >= 0) {
        e.stopImmediatePropagation();
        e.stopPropagation();
        this.controls.enabled = false;
        this.canvas.setPointerCapture(e.pointerId);
        this.lineEdit = { id: sel.id, index: i, pts: sel.points.map((p) => [...p] as [number, number, number]), stamp: isStampKind(sel.kind) };
        return;
      }
    }
    for (const k of [...this.mapLines].reverse()) {
      if (isKindHidden(k.kind)) continue;
      const pts = screen(k);
      if (pts.length === k.points.length && (isStampKind(k.kind) ? hitsStamp(k.kind, k.text, pts[0], pts[1], x, y) : distanceToCurve(pts, x, y, k.closed) <= 7)) {
        this.onSelectLine(k.id);
        return;
      }
    }
    if (this.selectedLine && this.hasMapLine(this.selectedLine)) this.onSelectLine('');
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

  /**
   * Where a map line's points are on the screen. A symbol is two points (where it sits, and which way is
   * up and how big): its second point is laid out as if the map were seen from straight above (size from
   * the metres between the points, turn from the direction relative to the view's turn), so tilting the
   * view never squashes, shrinks or spins a symbol. Turning the view still turns it with the map.
   */
  private screenPoints(k: { kind: string; points: [number, number, number][] }): ([number, number] | null)[] {
    if (!isStampKind(k.kind) || k.points.length < 2) return k.points.map((p) => this.project(p));
    const a = this.project(k.points[0]);
    const px = this.pixelsPerMetreAt(k.points[0]);
    const right = new THREE.Vector3().setFromMatrixColumn(this.camera.matrixWorld, 0);
    right.z = 0;
    if (!a || !px || right.lengthSq() < 1e-9) return [a, this.project(k.points[1])];
    right.normalize();
    const forward = { x: -right.y, y: right.x }; // up the screen when seen from above
    const vx = k.points[1][0] - k.points[0][0];
    const vy = k.points[1][1] - k.points[0][1];
    const sx = (vx * right.x + vy * right.y) * px;
    const sy = (vx * forward.x + vy * forward.y) * px;
    return [a, [a[0] + sx, a[1] - sy]];
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
    refreshFlipped();
    const stationsXY = (this.data?.stations ?? []).map((s) => [s.at[0], s.at[1]] as [number, number]);
    const base = g.globalAlpha;
    g.lineCap = 'round';
    g.lineJoin = 'round';
    for (const k of lines) {
      if (isKindHidden(k.kind)) continue; // switched off in the Show menu
      const l = look(k.kind);
      g.strokeStyle = l.color;
      g.globalAlpha = base * l.alpha;
      g.setLineDash(l.dash);
      const projected = this.screenPoints(k);
      if (isStampKind(k.kind)) {
        // A placed symbol: first point where it sits, second says which way is up and how big.
        const [a, b] = projected;
        if (a && b) {
          g.setLineDash([]);
          g.lineWidth = Math.max(1, l.weight * 0.7);
          if (k.id && k.id === this.selectedLine) {
            const pl = placement(a, b);
            g.strokeStyle = `#${this.theme.accent.getHexString()}`;
            g.globalAlpha = base * 0.9;
            g.lineWidth = 3;
            g.beginPath();
            g.arc(pl.cx, pl.cy, pl.r + 6, 0, Math.PI * 2);
            g.stroke();
            g.strokeStyle = l.color;
            g.lineWidth = Math.max(1, l.weight * 0.7);
            g.globalAlpha = base * l.alpha;
          }
          drawStamp(g, k.kind, a, b, k.text);
        }
        continue;
      }
      // A curve through the fit points (straight where part of it is behind the camera).
      const whole = projected.every((p) => !!p);
      const curve = whole ? smooth(projected as [number, number][], k.widths, 10, k.closed) : { pts: projected, widths: k.widths };
      const pts = curve.pts;
      const trace = (extra: number, alpha: number, dash: number[]) => {
        g.globalAlpha = base * alpha;
        g.setLineDash(dash);
        for (let i = 1; i < pts.length; i++) {
          const a = pts[i - 1];
          const b = pts[i];
          if (!a || !b) continue;
          g.lineWidth = l.weight * (curve.widths ? (curve.widths[i - 1] + curve.widths[i]) / 2 : 1) + extra;
          g.beginPath();
          g.moveTo(a[0], a[1]);
          g.lineTo(b[0], b[1]);
          g.stroke();
        }
      };
      if (isFillKind(k.kind) && whole && k.points.length >= 3) {
        // An area fill drawn in plan: the marks are made in plan metres and lifted to the outline's height.
        const z = k.points[0][2];
        const plan = smooth(k.points.map((p) => [p[0], p[1]] as [number, number]), null, 10, true).pts as [number, number][];
        g.setLineDash([]);
        g.lineWidth = Math.max(1, l.weight * 0.7);
        drawMarks(g, marksFor(k.kind, k.id, plan), (x, y) => this.project([x, y, z]) ?? [-1e5, -1e5], pts as [number, number][]);
      }
      // A shaped line (ledge, pit, slope, glyphs...): its marks are made from the plan curve and drawn lifted to its height.
      let spine = true;
      if (whole && k.points.length >= 2) {
        const plan = smooth(k.points.map((p) => [p[0], p[1]] as [number, number]), null, 10, k.closed).pts;
        const orn = ornamentFor(k.kind, k.id, plan, k.closed, stationsXY);
        spine = orn.spine;
        if (orn.lines.length) {
          const z = k.points[0][2];
          g.setLineDash([]);
          g.lineWidth = Math.max(1, l.weight * 0.8);
          for (const line of orn.lines) {
            g.beginPath();
            line.forEach((p, i) => {
              const q = this.project([p[0], p[1], z]);
              if (!q) return;
              if (i === 0) g.moveTo(q[0], q[1]);
              else g.lineTo(q[0], q[1]);
            });
            g.stroke();
          }
        }
      }
      if (k.id && k.id === this.selectedLine) {
        g.strokeStyle = `#${this.theme.accent.getHexString()}`;
        trace(9, 0.6, []);
        g.strokeStyle = l.color;
      }
      if (spine) trace(0, l.alpha, l.dash);
    }
    g.setLineDash([]);
    g.globalAlpha = base;
  }

  private selectedShot = '';
  /** The leg or splay (shot id) to light up on the map. */
  setSelectedShot(id: string) {
    this.selectedShot = id;
    this.requestRender();
  }

  /** The leg or splay under a screen point (within `radius` pixels), or null. A leg wins over a splay that is no closer. */
  pickShot(clientX: number, clientY: number, radius = 7): { kind: 'leg' | 'splay'; id: string } | null {
    const s = this.data;
    if (!s) return null;
    const rect = this.canvas.getBoundingClientRect();
    const x = clientX - rect.left;
    const y = clientY - rect.top;
    let best: { kind: 'leg' | 'splay'; id: string } | null = null;
    let bestD = radius;
    const near = (a: [number, number, number], b: [number, number, number], bias: number) => {
      const p = this.project(a);
      const q = this.project(b);
      if (!p || !q) return Infinity;
      const dx = q[0] - p[0];
      const dy = q[1] - p[1];
      const len2 = dx * dx + dy * dy;
      const t = len2 < 1e-9 ? 0 : Math.max(0, Math.min(1, ((x - p[0]) * dx + (y - p[1]) * dy) / len2));
      return Math.hypot(x - (p[0] + t * dx), y - (p[1] + t * dy)) + bias;
    };
    for (const l of s.legs) {
      const d = near(l.from, l.to, 0);
      if (d <= bestD) {
        bestD = d;
        best = { kind: 'leg', id: l.shot };
      }
    }
    s.splays.forEach((sp, i) => {
      const d = near([sp[0], sp[1], sp[2]], [sp[3], sp[4], sp[5]], 1.5);
      if (d <= bestD && s.splay_shots[i]) {
        bestD = d;
        best = { kind: 'splay', id: s.splay_shots[i] };
      }
    });
    return best;
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

  /** The rock halo: a soft glow from the walls out into the rock, seen when looking down on the plan. */
  setHalo(on: boolean, reachM = 5, fade = 0.5) {
    this.halo = on;
    this.haloReachM = reachM;
    this.haloFade = fade;
    this.requestRender();
  }

  /** The halo in the picture, when looking from above (it is a plan-view picture and fades out as the camera tilts). */
  private drawHaloPlan(g: CanvasRenderingContext2D, walls: Pt[][]) {
    const up = Math.abs(this.cameraOffset.clone().normalize().z);
    const strength = Math.min(1, Math.max(0, (up - 0.8) / 0.15));
    if (strength <= 0 || walls.length === 0) return;
    const z = this.controls.target.z;
    const ray = new THREE.Raycaster();
    const plane = new THREE.Plane(new THREE.Vector3(0, 0, 1), -z);
    const hit = new THREE.Vector3();
    const toWorld = (x: number, y: number): [number, number] => {
      ray.setFromCamera(new THREE.Vector2((x / this.width) * 2 - 1, -(y / this.height) * 2 + 1), this.camera);
      return ray.ray.intersectPlane(plane, hit) ? [hit.x, hit.y] : [0, 0];
    };
    const toScreen = (x: number, y: number): [number, number] => this.project([x, y, z]) ?? [-1e5, -1e5];
    const lines: HaloLine[] = [];
    for (const w of walls) {
      const pts = w.map((p) => this.project(p));
      if (pts.some((p) => !p)) continue;
      lines.push({ pts: pts as [number, number][] });
    }
    // walls drawn on the map count too, as you sketch them
    for (const k of this.displayMapLines()) {
      if (k.kind !== 'wall' || k.points.length < 2) continue;
      const pts = k.points.map((p) => this.project(p));
      if (pts.some((p) => !p)) continue;
      const curve = smooth(pts as [number, number][], null, 8, k.closed);
      lines.push({ pts: curve.pts as [number, number][], closed: k.closed });
    }
    // the passage surface, so the glow stays out of it (every triangle turned the same way so none cancels another)
    const inside: [number, number][][] = [];
    const pos = this.passage?.geometry.getAttribute('position');
    if (pos) {
      const v = new THREE.Vector3();
      const pt = (i: number): [number, number] => {
        v.fromBufferAttribute(pos, i).project(this.camera);
        return [(v.x * 0.5 + 0.5) * this.width, (-v.y * 0.5 + 0.5) * this.height];
      };
      for (let i = 0; i + 2 < pos.count; i += 3) {
        const a = pt(i);
        const b = pt(i + 1);
        const c = pt(i + 2);
        const area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if (Math.abs(area) < 1e-3) continue;
        inside.push(area > 0 ? [a, b, c] : [a, c, b]);
      }
    }
    const reach = Math.min(Math.max(this.haloReachM * (this.height / (2 * this.visibleHalfHeight())), 14), 400);
    g.save();
    g.globalAlpha = strength;
    drawHalo(g, this.width, this.height, lines, { reach, fade: this.haloFade, tint: `#${this.theme.accent.getHexString()}`, ink: this.theme.css.muted, toWorld, toScreen, inside });
    g.restore();
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

  /** Draws the view right now, so its canvas holds the picture (for a screenshot). */
  renderNow() {
    if (this.pending) cancelAnimationFrame(this.pending);
    this.pending = 0;
    this.render();
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
    if (this.halo) this.drawHaloPlan(g, s.walls);

    const zmin = s.bounds?.[2] ?? 0;
    const zspan = Math.max(1e-6, (s.bounds?.[5] ?? 0) - zmin);
    // stations: the survey point triangle (a dot in it when fixed), over the survey line
    g.lineWidth = 1.3;
    for (const st of s.stations) {
      const p = this.project(st.at);
      if (!p || p[0] < -20 || p[1] < -20 || p[0] > this.width + 20 || p[1] > this.height + 20) continue;
      g.strokeStyle = st.benchmark ? `#${this.theme.accent.getHexString()}` : this.elevationColors ? `#${elevationColor((st.at[2] - zmin) / zspan).getHexString()}` : css.text;
      drawStationMarker(g, p[0], p[1], st.benchmark, 5);
    }
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
    this.strokeLines(g, this.displayMapLines());
    for (const r of this.remoteCursors) {
      const q = this.project([r.x, r.y, this.controls.target.z]);
      if (q) drawCursor(g, q[0], q[1], r.name, r.color);
    }
    const editing = this.displayMapLines().find((k) => k.id === this.selectedLine);
    if (editing) {
      const hs = this.screenPoints(editing).filter((p): p is [number, number] => !!p);
      drawHandles(g, hs, `#${this.theme.accent.getHexString()}`, `#${this.theme.bg.getHexString()}`);
    }
    if (this.pickupAt && this.drawTool && !this.liveLine) {
      // the end of an open line the next stroke would carry on from
      g.strokeStyle = `#${this.theme.accent.getHexString()}`;
      g.lineWidth = 2;
      g.beginPath();
      g.arc(this.pickupAt[0], this.pickupAt[1], 9, 0, Math.PI * 2);
      g.stroke();
    }
    const live = this.liveLine;
    if (live && live.pts.length > 1) {
      this.strokeLines(g, [{ id: '', closed: false, kind: this.drawTool || 'sketch', run: '', points: live.pts.map((p) => [p[0], p[1], live.z] as [number, number, number]), widths: live.pressure ? live.widths : null }]);
    }
    if (this.showSections) {
      for (const sec of this.sections) {
        const st = this.project(sec.at);
        const { at, px } = this.sectionAnchor(sec);
        if (!at || !px) continue;
        const to = (u: number, v: number): [number, number] => [at[0] + u * px, at[1] - v * px];
        const shell = leaderEnd(st ?? at, glyphCurves(sec.sketches, to));
        if (st) {
          // The leader ties the drawing to its station and stops at the drawing's outer shell.
          g.strokeStyle = css.muted;
          g.lineWidth = 1;
          g.setLineDash([4, 3]);
          g.beginPath();
          g.moveTo(st[0], st[1]);
          g.lineTo(shell.end[0], shell.end[1]);
          g.stroke();
          g.setLineDash([]);
        }
        drawStrokes(g, sec.sketches, to, looks(), 1, '', [[0, 0]]);
        g.fillStyle = css.muted;
        g.textAlign = 'center';
        g.fillText(sec.label, shell.below[0], shell.below[1]);
        g.textAlign = 'left';
      }
    }
    // the picked leg or splay: a bright line in the theme's highlight colour along it
    if (this.selectedShot) {
      const leg = s.legs.find((l) => l.shot === this.selectedShot);
      const si = leg ? -1 : s.splay_shots.indexOf(this.selectedShot);
      const ends: [[number, number, number], [number, number, number]] | null = leg ? [leg.from, leg.to] : si >= 0 ? [[s.splays[si][0], s.splays[si][1], s.splays[si][2]], [s.splays[si][3], s.splays[si][4], s.splays[si][5]]] : null;
      const a = ends && this.project(ends[0]);
      const b = ends && this.project(ends[1]);
      if (a && b) {
        const hi = `#${this.theme.accent.getHexString()}`;
        g.lineCap = 'round';
        g.strokeStyle = hi;
        g.globalAlpha = 0.3;
        g.lineWidth = 11;
        g.beginPath();
        g.moveTo(a[0], a[1]);
        g.lineTo(b[0], b[1]);
        g.stroke();
        g.globalAlpha = 1;
        g.lineWidth = 3.5;
        g.stroke();
      }
    }
    const chosen = this.selected ? s.stations.find((st) => st.id === this.selected) : undefined;
    const cp = chosen ? this.project(chosen.at) : null;
    if (cp) {
      // the picked station: a filled halo and ring in the theme's highlight colour
      const hi = `#${this.theme.accent.getHexString()}`;
      g.fillStyle = hi;
      g.globalAlpha = 0.28;
      g.beginPath();
      g.arc(cp[0], cp[1], 13, 0, Math.PI * 2);
      g.fill();
      g.globalAlpha = 1;
      g.strokeStyle = hi;
      g.lineWidth = 3;
      g.beginPath();
      g.arc(cp[0], cp[1], 13, 0, Math.PI * 2);
      g.stroke();
      drawStationMarker(g, cp[0], cp[1], !!chosen?.benchmark, 7);
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
