// Builds the sheet for the cave that is open and turns it into a picture to save or print.

import { cave } from './cave.svelte';
import { engine, type Scene, type SheetPlan, type ProfileSketch3d } from './engine';
import { looks } from './ink';
import { readOnly } from './readonly';
import { drawSheet } from './sheet-draw';
import { sheet } from './sheet.svelte';

/** The words for the title block's first line: what was typed, else the file's name made readable. */
export function sheetName(): string {
  if (sheet.name.trim()) return sheet.name.trim();
  const base = cave.fileName.replace(/\.[^.]+$/, '').replace(/[-_]+/g, ' ').trim();
  return base.replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Everything the sheet needs, read fresh from the engine. */
export function currentSheet(): { plan: SheetPlan; scene: Scene; lines: ProfileSketch3d[] } {
  const scene = engine.scene(cave.adjusted);
  const b = scene.bounds;
  const [w, h] = b ? [b[3] - b[0], b[4] - b[1]] : [1, 1];
  const plan = engine.sheetPlan(w, h, { ...sheet, name: sheetName() });
  return { plan, scene, lines: engine.mapLines(cave.adjusted) };
}

/** The sheet drawn at `dpi` dots per inch on a new canvas. */
export function renderSheet(dpi: number): HTMLCanvasElement {
  const { plan, scene, lines } = currentSheet();
  const c = document.createElement('canvas');
  c.width = Math.round(plan.w * dpi);
  c.height = Math.round(plan.h * dpi);
  const g = c.getContext('2d');
  if (g) drawSheet(g, plan, scene, lines, dpi, looks());
  return c;
}

/** Saves the sheet as a PNG picture at 200 dots per inch. */
export function downloadSheet(): string {
  if (readOnly.noKeep) return 'Printing and saving are turned off while you are watching a class: the cave stays with the teacher.';
  const c = renderSheet(200);
  const a = document.createElement('a');
  a.href = c.toDataURL('image/png');
  a.download = cave.fileName.replace(/\.[^.]+$/, '') + '-sheet.png';
  a.click();
  return `Saved ${a.download}`;
}

/** Opens the print dialog for the sheet (choose "Save as PDF" there for a PDF), on the right paper size. */
export function printSheet(): string {
  if (readOnly.noKeep) return 'Printing and saving are turned off while you are watching a class: the cave stays with the teacher.';
  const { plan } = currentSheet();
  const url = renderSheet(150).toDataURL('image/png');
  const f = document.createElement('iframe');
  f.style.cssText = 'position:fixed;width:0;height:0;border:0;right:0;bottom:0';
  document.body.appendChild(f);
  const d = f.contentDocument;
  if (!d) return 'Could not open the print dialog.';
  d.open();
  d.write(`<!doctype html><title>Sheet</title><style>@page{size:${plan.w}in ${plan.h}in;margin:0}html,body{margin:0}img{width:${plan.w}in;height:${plan.h}in;display:block}</style><img src="${url}">`);
  d.close();
  const img = d.querySelector('img');
  const go = () => {
    f.contentWindow?.focus();
    f.contentWindow?.print();
    setTimeout(() => f.remove(), 60_000);
  };
  if (img && !img.complete) img.onload = go;
  else go();
  return 'Print dialog opened.';
}
