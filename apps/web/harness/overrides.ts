// The engine calls the harness answers itself. The sketchpad's strokes are the sketchpad's (kept on the device), not
// lines of the cave, so the calls the ribbon's Edit tab and the panels' Undo make about "the selected line" and "this
// panel's history" go to the sketch store when the line or the panel is the sketchpad's, and to the real engine for
// anything else (the Notebook panel's history, for one). An override answers `undefined` to hand the call on.
import { sketch, VIEW_NAME } from '../src/lib/sketch.svelte';

type Real = (...a: never[]) => unknown;

export const overrides: Record<string, ((real: Real, ...a: any[]) => unknown) | undefined> = {
  edit_state: (_r, scope: string) => {
    const pane = sketch.paneOf(scope);
    return pane ? JSON.stringify(sketch.can(pane)) : undefined;
  },
  undo: (_r, scope: string) => {
    const pane = sketch.paneOf(scope);
    return pane ? sketch.step(pane, 'undo') : undefined;
  },
  redo: (_r, scope: string) => {
    const pane = sketch.paneOf(scope);
    return pane ? sketch.step(pane, 'redo') : undefined;
  },
  line_info: (_r, id: string) => {
    const k = sketch.find(id);
    return k ? JSON.stringify({ kind: k.kind, view: VIEW_NAME[k.view], closed: k.closed, flip: k.flip, text: k.text }) : undefined;
  },
  set_line_kind: (_r, id: string, kind: string) => (sketch.find(id) ? (sketch.patch(id, { kind }), 'ok') : undefined),
  set_line_closed: (_r, id: string, closed: boolean) => (sketch.find(id) ? (sketch.patch(id, { closed }), 'ok') : undefined),
  set_line_flip: (_r, id: string, flip: boolean) => (sketch.find(id) ? (sketch.patch(id, { flip }), 'ok') : undefined),
  set_line_text: (_r, id: string, text: string) => (sketch.find(id) ? (sketch.patch(id, { text }), 'ok') : undefined),
  delete_line: (_r, id: string) => (sketch.find(id) ? (sketch.remove(id), 'ok') : undefined),
};
