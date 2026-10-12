// A picture of the page made by the page itself, so it needs no permission and works wherever the app
// runs (a browser's screen capture is often blocked, and absent on tablets). The page's elements are
// drawn by the browser, and the canvases (the 3D map and the drawing panels, which that step cannot
// see) are laid on top where they sit.

import { toCanvas } from 'html-to-image';

interface Renderable {
  renderNow(): void;
}
let active: Renderable | null = null;

/** Tells the snapshot which 3D view to redraw before it copies its canvas. */
export function setActiveView(v: Renderable) {
  active = v;
}

/** The page as a PNG, or null if the browser could not draw it. */
export async function snapshotPage(): Promise<Blob | null> {
  try {
    const bg = getComputedStyle(document.body).backgroundColor || '#ffffff';
    const w = document.documentElement.clientWidth;
    const h = document.documentElement.clientHeight;
    const base = await toCanvas(document.body, {
      width: w,
      height: h,
      pixelRatio: 1,
      backgroundColor: bg,
      cacheBust: false,
      filter: (n) => !(n instanceof HTMLCanvasElement) && !(n instanceof HTMLElement && n.dataset.noSnapshot !== undefined),
    });
    const out = document.createElement('canvas');
    out.width = base.width;
    out.height = base.height;
    const g = out.getContext('2d');
    if (!g) return null;
    g.drawImage(base, 0, 0);
    const k = base.width / w;
    active?.renderNow(); // a 3D canvas only holds its picture right after it is drawn, so copy it straight after
    for (const c of document.querySelectorAll('canvas')) {
      const r = c.getBoundingClientRect();
      if (r.width < 2 || r.height < 2 || r.bottom < 0 || r.right < 0 || r.left > w || r.top > h) continue;
      if (getComputedStyle(c).visibility === 'hidden' || c.closest('[inert]')) continue;
      g.drawImage(c, r.left * k, r.top * k, r.width * k, r.height * k);
    }
    return await new Promise((res) => out.toBlob(res, 'image/png'));
  } catch {
    return null;
  }
}
