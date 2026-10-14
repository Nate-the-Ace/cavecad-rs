// The drawing's scale and the line patterns, read from the engine into the state the views draw with.
import { engine } from './engine';
import { readDrawing } from './line-style.svelte';

/** Reads the drawing's scale (and, once, the linetypes) from the engine. Safe to call whenever the cave changes. */
export function syncDrawing() {
  try {
    const lt = engine.linetypes();
    readDrawing({ scale: engine.drawingScale(), types: lt.types, dotMm: lt.dot_mm });
  } catch {
    /* the engine is not running yet */
  }
}
