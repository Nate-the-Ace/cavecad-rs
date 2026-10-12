// The view cube: a small cube that follows the camera. Click a face, an edge or a corner to
// snap the main view to that direction (26 targets, like a CAD view cube).
import * as THREE from 'three';
import type { Dir } from './cave-view';
import { readTheme } from './theme';

const SIZE = 132;
/** Outer third of a face counts as its edge or corner; the middle third is the face itself. */
const EDGE = 0.5 / 3;
/** Pixels a press may move and still count as a click. */
const DRAG_PIXELS = 4;
/** Radians the view turns per pixel dragged on the cube. */
const TURN_PER_PIXEL = 0.012;

const LABELS = ['RIGHT', 'LEFT', 'TOP', 'BOTTOM', 'FRONT', 'BACK'];

function faceTexture(label: string, fg: string, bg: string, line: string): THREE.CanvasTexture {
  const c = document.createElement('canvas');
  c.width = c.height = 128;
  const g = c.getContext('2d')!;
  g.fillStyle = bg;
  g.fillRect(0, 0, 128, 128);
  g.strokeStyle = line;
  g.lineWidth = 6;
  g.strokeRect(3, 3, 122, 122);
  g.fillStyle = fg;
  g.font = '600 24px system-ui, sans-serif';
  g.textAlign = 'center';
  g.textBaseline = 'middle';
  g.fillText(label, 64, 66);
  const t = new THREE.CanvasTexture(c);
  t.colorSpace = THREE.SRGBColorSpace;
  return t;
}

/** Which of the 26 directions a hit point on the cube surface belongs to. */
export function regionOf(p: THREE.Vector3): Dir {
  const q = (v: number): number => (Math.abs(v) > EDGE ? Math.sign(v) : 0);
  return [q(p.x), q(p.y), q(p.z)];
}

export class ViewCube {
  private readonly renderer: THREE.WebGLRenderer;
  private readonly scene = new THREE.Scene();
  private readonly camera = new THREE.PerspectiveCamera(30, 1, 0.1, 20);
  private readonly cube: THREE.Group;
  private readonly faces: THREE.Mesh;
  private readonly marker: THREE.Mesh;
  private readonly ray = new THREE.Raycaster();
  private readonly canvas: HTMLCanvasElement;

  constructor(
    host: HTMLElement,
    private readonly onPick: (dir: Dir) => void,
    private readonly onDrag: (dAzimuth: number, dPolar: number) => void = () => {},
  ) {
    this.canvas = document.createElement('canvas');
    this.canvas.style.cssText = `width:${SIZE}px;height:${SIZE}px;display:block;cursor:pointer`;
    host.appendChild(this.canvas);
    this.renderer = new THREE.WebGLRenderer({ canvas: this.canvas, antialias: true, alpha: true });
    this.renderer.setPixelRatio(window.devicePixelRatio || 1);
    this.renderer.setSize(SIZE, SIZE, false);
    this.camera.up.set(0, 0, 1);

    const t = readTheme();
    const mats = LABELS.map((l) => new THREE.MeshBasicMaterial({ map: faceTexture(l, t.css.text, t.css.panel, t.css.line) }));
    // Built in a y-up box then turned so its +y face is world up (z): the faces then read
    // RIGHT(+x) LEFT(-x) TOP(+z) BOTTOM(-z) FRONT(-y, south) BACK(+y, north) with upright text.
    const box = new THREE.Mesh(new THREE.BoxGeometry(1, 1, 1), mats);
    const edges = new THREE.LineSegments(new THREE.EdgesGeometry(box.geometry), new THREE.LineBasicMaterial({ color: t.accent }));
    this.faces = box;
    this.cube = new THREE.Group();
    this.cube.add(box, edges);
    this.cube.rotation.x = Math.PI / 2;
    this.scene.add(this.cube);

    this.marker = new THREE.Mesh(new THREE.SphereGeometry(0.07, 12, 12), new THREE.MeshBasicMaterial({ color: t.accent }));
    this.marker.visible = false;
    this.scene.add(this.marker);

    // Drag to turn the view; a press that barely moves is a click on a face, edge or corner.
    let press: { x: number; y: number; dragged: boolean } | null = null;
    this.canvas.addEventListener('pointerdown', (e) => {
      press = { x: e.clientX, y: e.clientY, dragged: false };
      this.canvas.setPointerCapture(e.pointerId);
    });
    this.canvas.addEventListener('pointermove', (e) => {
      if (press) {
        const dx = e.clientX - press.x;
        const dy = e.clientY - press.y;
        if (press.dragged || Math.hypot(dx, dy) > DRAG_PIXELS) {
          press.dragged = true;
          this.marker.visible = false;
          this.canvas.style.cursor = 'grabbing';
          this.onDrag(dx * TURN_PER_PIXEL, dy * TURN_PER_PIXEL);
          press.x = e.clientX;
          press.y = e.clientY;
        }
        return;
      }
      this.hover(e);
    });
    const release = (e: PointerEvent) => {
      const wasDrag = press?.dragged ?? false;
      press = null;
      this.canvas.style.cursor = 'pointer';
      if (this.canvas.hasPointerCapture(e.pointerId)) this.canvas.releasePointerCapture(e.pointerId);
      if (!wasDrag && e.type === 'pointerup') {
        const hit = this.pick(e);
        if (hit) this.onPick(regionOf(hit));
      }
    };
    this.canvas.addEventListener('pointerup', release);
    this.canvas.addEventListener('pointercancel', release);
    this.canvas.addEventListener('pointerleave', () => {
      this.marker.visible = false;
      this.draw();
    });
  }

  private pick(e: MouseEvent): THREE.Vector3 | null {
    const r = this.canvas.getBoundingClientRect();
    const ndc = new THREE.Vector2(((e.clientX - r.left) / r.width) * 2 - 1, -(((e.clientY - r.top) / r.height) * 2 - 1));
    this.ray.setFromCamera(ndc, this.camera);
    // Faces only: the outline lines have a wide hit margin and would swallow every click.
    const hit = this.ray.intersectObject(this.faces, false)[0];
    return hit ? hit.point.clone() : null;
  }

  private hover(e: PointerEvent) {
    const p = this.pick(e);
    this.marker.visible = !!p;
    if (p) {
      const d = regionOf(p);
      this.marker.position.set(d[0], d[1], d[2]).multiplyScalar(0.5).multiplyScalar(1.02);
    }
    this.draw();
  }

  /** Points the cube's camera the same way the main camera looks. */
  sync(camera: THREE.Camera, target: THREE.Vector3) {
    const dir = camera.position.clone().sub(target).normalize();
    this.camera.position.copy(dir.multiplyScalar(3.6));
    this.camera.up.copy(camera.up);
    this.camera.lookAt(0, 0, 0);
    this.draw();
  }

  private draw() {
    this.renderer.render(this.scene, this.camera);
  }

  dispose() {
    this.renderer.dispose();
    this.canvas.remove();
  }
}
