// Number entry on phones and tablets. Every field that only takes a number uses `use:numpad`, so the
// on-screen keyboard opens on numbers instead of letters. The rule lives here, in one place:
//   - a decimal opens the decimal pad, a whole number the number pad (`inputmode`);
//   - a field that can be below zero (clino, declination, offsets) gets a small ± key beside it on a
//     touch screen, because the iPhone's number pads have no minus key;
//   - a field may ask for extra keys (the notebook's passage cells take P for open passage).
// The keys show only while their field is being typed in, so tables do not grow a column of them.
// With a mouse nothing shows and nothing changes: the attributes only steer touch keyboards.
// The fields stay type="text" (or whatever they were): type="number" would break the unit parsing.

export interface NumpadOptions {
  /** The value can be below zero: shows the ± key on touch screens. */
  signed?: boolean;
  /** Whole numbers only: the number pad instead of the decimal pad. */
  integer?: boolean;
  /** What the keyboard's Enter key says, when Enter does something in this field. */
  enter?: 'next' | 'done';
  /** Extra keys on touch screens; each one puts its text in the field (pressing it again clears it). */
  keys?: string[];
}

/** The attributes a number field gets. */
export function numpadAttrs(o: NumpadOptions = {}): Record<string, string> {
  const a: Record<string, string> = { inputmode: o.integer ? 'numeric' : 'decimal' };
  if (o.enter) a.enterkeyhint = o.enter;
  return a;
}

/** The text with its sign turned over: blank becomes "-" (so the number is typed after it), "-" becomes blank. */
/** A touch that lifted within 10 px of where it landed is a tap; anything further was a swipe or a scroll. */
export function isTap(from: { x: number; y: number }, to: { x: number; y: number }): boolean {
  return Math.hypot(to.x - from.x, to.y - from.y) < 10;
}

export function flipSign(text: string): string {
  const m = /^(\s*)([+-]?)([\s\S]*)$/.exec(text)!;
  return m[1] + (m[2] === '-' ? '' : '-') + m[3];
}

/** What an extra key puts in the field: its text, or blank when the field already holds it. */
export function keyText(current: string, key: string): string {
  return current.trim().toLowerCase() === key.toLowerCase() ? '' : key;
}

/** The touch keys a field shows (none with a mouse). */
export function touchKeys(o: NumpadOptions, coarse: boolean): string[] {
  if (!coarse) return [];
  return [...(o.signed ? ['±'] : []), ...(o.keys ?? [])];
}

/** Svelte action: `<input use:numpad={{ signed: true }} />`. */
export function numpad(node: HTMLInputElement, opts: NumpadOptions = {}) {
  let o = opts;
  let buttons: HTMLButtonElement[] = [];
  /** A key changed the value and no change event has gone out since. */
  let pending = false;
  const mq = typeof matchMedia === 'function' ? matchMedia('(pointer: coarse)') : null;

  function setValue(v: string) {
    node.value = v;
    try {
      node.setSelectionRange(v.length, v.length);
    } catch {
      // type="number" has no selection
    }
    pending = true;
    node.dispatchEvent(new Event('input', { bubbles: true })); // what typing fires; bind:value listens to it
  }

  function press(label: string) {
    if (node.disabled || node.readOnly) return;
    setValue(label === '±' ? flipSign(node.value) : keyText(node.value, label));
    if (document.activeElement !== node) {
      // Not being typed in: the edit is finished, so send the commit a typed edit gets on leaving the field.
      pending = false;
      node.dispatchEvent(new Event('change', { bubbles: true }));
    }
  }

  function build() {
    for (const b of buttons) b.remove();
    buttons = [];
    for (const [k, v] of Object.entries(numpadAttrs(o))) node.setAttribute(k, v);
    if (!o.enter) node.removeAttribute('enterkeyhint');
    let after: Element = node;
    for (const label of touchKeys(o, !!mq?.matches)) {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'numpad-key';
      b.tabIndex = -1;
      b.textContent = label;
      b.setAttribute('aria-label', label === '±' ? 'Change the sign' : `Enter ${label}`);
      // Keep the cursor (and the keyboard) in the field: the key must not take the focus.
      b.hidden = document.activeElement !== node;
      b.addEventListener('pointerdown', (e) => e.preventDefault());
      b.addEventListener('mousedown', (e) => e.preventDefault());
      // iOS: acting on touchend (and cancelling it) stops the tap turning into a click that moves the focus. Only a
      // tap counts: touchend comes back to the key even when a swipe that began on it scrolled the notebook, and
      // that must not flip a clino reading's sign or overwrite a wall with P.
      let start: { x: number; y: number } | null = null;
      b.addEventListener('touchstart', (e) => {
        const t = e.changedTouches[0];
        start = t ? { x: t.clientX, y: t.clientY } : null;
      });
      b.addEventListener('touchcancel', () => (start = null));
      b.addEventListener('touchend', (e) => {
        const t = e.changedTouches[0];
        const from = start;
        start = null;
        e.stopPropagation();
        if (!t || !from || !isTap(from, { x: t.clientX, y: t.clientY })) return;
        if (document.elementFromPoint?.(t.clientX, t.clientY) !== b) return;
        e.preventDefault();
        press(label);
      });
      b.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation(); // a row under it would take the tap as a pick
        press(label);
      });
      after.after(b);
      after = b;
      buttons.push(b);
    }
  }

  const onChange = () => (pending = false);
  const onFocus = () => {
    for (const b of buttons) b.hidden = false;
  };
  // Leaving the field: browsers do not always send change for a value set by a key, so send it then.
  const onBlur = () => {
    for (const b of buttons) b.hidden = true;
    if (!pending) return;
    pending = false;
    node.dispatchEvent(new Event('change', { bubbles: true }));
  };
  node.addEventListener('change', onChange);
  node.addEventListener('focus', onFocus);
  node.addEventListener('blur', onBlur);
  mq?.addEventListener?.('change', build);
  build();

  return {
    update(next: NumpadOptions = {}) {
      o = next;
      build();
    },
    destroy() {
      for (const b of buttons) b.remove();
      buttons = [];
      node.removeEventListener('change', onChange);
      node.removeEventListener('focus', onFocus);
      node.removeEventListener('blur', onBlur);
      mq?.removeEventListener?.('change', build);
    },
  };
}
