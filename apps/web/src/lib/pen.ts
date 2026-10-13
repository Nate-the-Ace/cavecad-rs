// Pen input, separated from any canvas so it can be checked on its own: telling a pen from a
// finger and a resting palm, turning pressure into line weight, and spotting the eraser end.
// Pressure only ever changes how a line is drawn, never where it is.

/** What an input event carries that this file looks at (a PointerEvent has all of it). */
export interface PenEvent {
  pointerType: string;
  pressure: number;
  buttons: number;
  button: number;
  timeStamp: number;
  /** Size of the contact in CSS pixels (0 or 1 when the device does not say). */
  width?: number;
  height?: number;
}

/** After the pen lifts, touches stay ignored this long: a resting hand lands as the pen leaves. */
export const PALM_HOLD_MS = 600;
/** A touch bigger than this (CSS pixels) across is a palm or a fist, not a fingertip. Some tablets report a fingertip as quite large, so this is generous: a palm is far bigger. */
export const PALM_SIZE = 80;

/** Line weight for a pen pressure: light touch thin, hard press thick, 1 at about half pressure. */
export function widthFor(pressure: number): number {
  const p = Number.isFinite(pressure) ? Math.min(1, Math.max(0, pressure)) : 0.5;
  return 0.4 + 1.2 * p;
}

/** True for the eraser end of a pen (or the button that stands in for it). */
export function isEraser(e: Pick<PenEvent, 'pointerType' | 'buttons' | 'button'>): boolean {
  return e.pointerType === 'pen' && ((e.buttons & 32) !== 0 || e.button === 5);
}

/**
 * Palm rejection. Feed it every pen event on a canvas; ask it about every touch. A touch is
 * ignored while the pen is touching the screen or was near it a moment ago, or when the contact is
 * too big to be a fingertip. Mouse and pen are never ignored.
 */
export class PenGuard {
  private penDown = false;
  private lastPen = -Infinity;
  private rejected = new Set<number>();
  /** Why the last touch was ignored, in words (so a device that wrongly ignores fingers can be told apart). */
  lastRejected = '';

  /** Call for every pen event (down, move, up, hover). */
  pen(e: { type: string; timeStamp: number; buttons: number }): void {
    this.lastPen = e.timeStamp;
    if (e.type === 'pointerdown') this.penDown = true;
    else if (e.type === 'pointerup' || e.type === 'pointercancel') this.penDown = false;
    else if (e.type === 'pointermove') this.penDown = e.buttons !== 0 ? this.penDown : false;
  }

  /** True when a pen is touching the screen right now. */
  get drawing(): boolean {
    return this.penDown;
  }

  /** A touch landed: true when it is a finger. A rejected touch stays rejected until it lifts, however long it rests. */
  touchDown(e: Pick<PenEvent, 'timeStamp' | 'width' | 'height'> & { pointerId: number }): boolean {
    const ok = this.allowTouch(e);
    if (!ok) this.rejected.add(e.pointerId);
    return ok;
  }

  /** True for a touch that was rejected when it landed (ignore its moves and its lift). */
  ignored(e: { pointerId: number }): boolean {
    return this.rejected.has(e.pointerId);
  }

  /** A touch lifted. */
  touchUp(e: { pointerId: number }): void {
    this.rejected.delete(e.pointerId);
  }

  /** Should this touch be treated as a finger? False means ignore it completely. */
  allowTouch(e: Pick<PenEvent, 'timeStamp' | 'width' | 'height'>): boolean {
    if (this.penDown) return (this.lastRejected = 'a pen is touching the screen'), false;
    if (e.timeStamp - this.lastPen < PALM_HOLD_MS) return (this.lastRejected = 'just after a pen was lifted'), false;
    const size = Math.max(e.width ?? 0, e.height ?? 0);
    if (size > PALM_SIZE) return (this.lastRejected = `the contact is ${Math.round(size)} px across, too big for a fingertip`), false;
    return true;
  }
}
