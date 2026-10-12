import { PenGuard, PALM_HOLD_MS, isEraser, widthFor } from '../src/lib/pen.ts';

const eq = (a: unknown, b: unknown, what: string) => {
  if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(`${what}: got ${JSON.stringify(a)}, wanted ${JSON.stringify(b)}`);
};

// Pressure: monotonic, bounded, 1 at half pressure, never NaN.
eq(widthFor(0.5), 1, 'half pressure is normal weight');
eq(widthFor(0), 0.4, 'no pressure is thin');
eq(widthFor(1), 1.6, 'full pressure is thick');
eq(widthFor(7), 1.6, 'clamped high');
eq(widthFor(NaN), 1, 'unknown pressure is normal');

// Eraser: the pen's eraser button bit, or button 5; never a finger or a mouse.
eq(isEraser({ pointerType: 'pen', buttons: 32, button: 0 }), true, 'eraser bit');
eq(isEraser({ pointerType: 'pen', buttons: 1, button: 5 }), true, 'button 5');
eq(isEraser({ pointerType: 'pen', buttons: 1, button: 0 }), false, 'ordinary pen tip');
eq(isEraser({ pointerType: 'mouse', buttons: 32, button: 0 }), false, 'a mouse has no eraser');

// Palm rejection.
const g = new PenGuard();
const touch = (t: number, size = 0) => ({ timeStamp: t, width: size, height: size });
eq(g.allowTouch(touch(0)), true, 'a finger is fine with no pen about');
g.pen({ type: 'pointerdown', timeStamp: 1000, buttons: 1 });
eq(g.allowTouch(touch(1100)), false, 'ignored while the pen is down');
g.pen({ type: 'pointermove', timeStamp: 1200, buttons: 1 });
g.pen({ type: 'pointerup', timeStamp: 1300, buttons: 0 });
eq(g.allowTouch(touch(1300 + PALM_HOLD_MS - 1)), false, 'ignored just after the pen lifts');
eq(g.allowTouch(touch(1300 + PALM_HOLD_MS + 1)), true, 'fingers work again after a pause');
g.pen({ type: 'pointermove', timeStamp: 5000, buttons: 0 }); // the pen hovers near the screen
eq(g.allowTouch(touch(5100)), false, 'ignored while the pen hovers');
eq(g.allowTouch(touch(6000, 60)), false, 'a palm-sized contact is never a finger');
eq(g.allowTouch(touch(6000, 12)), true, 'a fingertip is');
eq(g.drawing, false, 'hovering is not drawing');

console.log('pen checks ok');
