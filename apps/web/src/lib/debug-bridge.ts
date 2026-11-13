// Test builds of the desktop app only (VITE_UPDATE_CHANNEL=dev): the page half of the debug bridge
// (apps/desktop/src-tauri/src/bridge.rs, driven by tools/cavecad-app-mcp). It keeps the page's recent log, and
// offers helpers the bridge's `eval` calls to find and press things by what they say, so nobody's mouse moves.
// Nothing here runs in a browser or in a regular release.

type Entry = { t: number; level: string; text: string };

const MAX = 2000;
const log: Entry[] = [];

function keep(level: string, args: unknown[]) {
  const text = args
    .map((a) => {
      if (a instanceof Error) return a.stack || a.message;
      if (typeof a === 'string') return a;
      try {
        return JSON.stringify(a);
      } catch {
        return String(a);
      }
    })
    .join(' ');
  log.push({ t: Date.now(), level, text: text.slice(0, 4000) });
  if (log.length > MAX) log.splice(0, log.length - MAX);
}

/** Visible elements a person could press or read, with their label. */
function labelOf(el: Element): string {
  const h = el as HTMLElement;
  return (h.getAttribute('aria-label') || h.innerText || h.getAttribute('title') || (h as HTMLInputElement).value || '').trim().replace(/\s+/g, ' ');
}

function visible(el: Element): boolean {
  const r = (el as HTMLElement).getBoundingClientRect();
  if (r.width === 0 || r.height === 0) return false;
  const s = getComputedStyle(el as HTMLElement);
  return s.visibility !== 'hidden' && s.display !== 'none';
}

const PRESSABLE = 'button, a, [role=button], [role=tab], [role=menuitem], [role=radio], [role=checkbox], input, select, textarea, summary, label';

/** Elements whose label contains `text` (case-insensitive), most exact first. */
function find(text: string, selector = PRESSABLE): Element[] {
  const want = text.toLowerCase();
  const all = [...document.querySelectorAll(selector)].filter(visible);
  const hits = all.filter((el) => labelOf(el).toLowerCase().includes(want));
  return hits.sort((a, b) => labelOf(a).length - labelOf(b).length);
}

function describe(el: Element) {
  const r = (el as HTMLElement).getBoundingClientRect();
  return { tag: el.tagName.toLowerCase(), role: el.getAttribute('role'), label: labelOf(el).slice(0, 120), x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) };
}

/** Presses the element (pointer events then click, as a real press does). */
function press(el: Element) {
  const r = (el as HTMLElement).getBoundingClientRect();
  const at = { bubbles: true, cancelable: true, clientX: r.x + r.width / 2, clientY: r.y + r.height / 2, button: 0, pointerId: 1, pointerType: 'mouse', isPrimary: true };
  el.dispatchEvent(new PointerEvent('pointerdown', at));
  el.dispatchEvent(new MouseEvent('mousedown', at));
  el.dispatchEvent(new PointerEvent('pointerup', at));
  el.dispatchEvent(new MouseEvent('mouseup', at));
  (el as HTMLElement).click();
}

export function startDebugBridge(): void {
  for (const level of ['log', 'info', 'warn', 'error', 'debug'] as const) {
    const orig = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      keep(level, args);
      orig(...args);
    };
  }
  window.addEventListener('error', (e) => keep('uncaught', [e.error ?? e.message]));
  window.addEventListener('unhandledrejection', (e) => keep('unhandled', [e.reason]));
  (window as unknown as { __cavecadBridge: unknown }).__cavecadBridge = {
    /** The log since `since` (ms epoch), newest last; `level` filters ("error,warn"). */
    logs: (since = 0, level = '') => log.filter((e) => e.t > since && (!level || level.split(',').includes(e.level))),
    /** What matches `text`, described (label, kind, place on screen). */
    find: (text: string, selector?: string) => find(text, selector).slice(0, 30).map(describe),
    /** Presses the best match for `text` (the `nth` one, from 0); says what it pressed. */
    click: (text: string, nth = 0, selector?: string) => {
      const el = find(text, selector)[nth];
      if (!el) throw new Error(`nothing pressable says "${text}"`);
      press(el);
      return describe(el);
    },
    /** Types into the field whose label, placeholder or name matches, as if typed (input + change events). */
    type: (field: string, value: string) => {
      const want = field.toLowerCase();
      const el = [...document.querySelectorAll('input, textarea')].filter(visible).find((x) => {
        const i = x as HTMLInputElement;
        return [i.placeholder, i.name, i.getAttribute('aria-label'), i.id && document.querySelector(`label[for="${i.id}"]`)?.textContent, i.closest('label')?.textContent]
          .some((s) => (s ?? '').toLowerCase().includes(want));
      }) as HTMLInputElement | undefined;
      if (!el) throw new Error(`no field matches "${field}"`);
      el.focus();
      const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(proto, 'value')?.set?.call(el, value);
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return describe(el);
    },
    /** Sends a key to whatever has focus (Escape, Enter, ...). */
    key: (key: string) => {
      const t = document.activeElement ?? document.body;
      for (const type of ['keydown', 'keyup'] as const) t.dispatchEvent(new KeyboardEvent(type, { key, bubbles: true, cancelable: true }));
      return key;
    },
    /** All visible text, trimmed: a quick read of what is on screen. */
    text: (max = 20000) => document.body.innerText.slice(0, max),
  };
  keep('info', ['debug bridge ready']);
}
