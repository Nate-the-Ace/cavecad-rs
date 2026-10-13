// A feedback report: what the person wrote, optionally the app's log, a screenshot and a few facts about
// the app, put in one zip they can save, copy or send. Never the cave itself: no file name, no
// coordinates (the engine's log already redacts them), only counts.

import { cave } from './cave.svelte';
import { engine, loadBuildInfo } from './engine';
import { live } from './sync.svelte';
import { panelState } from './workspace.svelte';
import { pageErrors } from './diagnostics';
import { zip } from './zip';

export type FeedbackKind = 'bug' | 'idea' | 'question';
export const KIND_LABEL: Record<FeedbackKind, string> = { bug: 'Something is wrong', idea: 'An idea', question: 'A question' };

export interface Draft {
  kind: FeedbackKind;
  message: string;
  includeLog: boolean;
  includeDetails: boolean;
  /** A PNG of the screen, if one was taken or attached. */
  screenshot: Blob | null;
}

/** The engine's log and the page's own errors, as plain lines. */
export function logText(): string {
  let lines: string[] = [];
  try {
    lines = engine.events(0).map((e) => `${e.seq} ${e.level.toUpperCase()} ${e.source}: ${e.message}`);
  } catch {
    lines = ['(the engine was not running)'];
  }
  const errs = pageErrors();
  return [...lines.slice(-2000), ...(errs.length ? ['', '--- errors in the page ---', ...errs] : [])].join('\n');
}

/** Facts about the app and the machine that help to reproduce a problem. Counts only, never positions. */
export async function details(): Promise<Record<string, unknown>> {
  const info = await loadBuildInfo().catch(() => null);
  let stations = 0;
  let legs = 0;
  try {
    const s = engine.scene(cave.adjusted);
    stations = s.stations.length;
    legs = s.legs.length;
  } catch {
    // no cave open
  }
  return {
    build: info ? { git: info.git, dirty: info.dirty, builtAt: info.builtAt } : null,
    engine: (() => {
      try {
        return engine.version();
      } catch {
        return 'not loaded';
      }
    })(),
    browser: navigator.userAgent,
    language: navigator.language,
    screen: { width: window.innerWidth, height: window.innerHeight, pixelRatio: window.devicePixelRatio },
    units: cave.units,
    adjusted: cave.adjusted,
    compactRibbon: cave.compactRibbon,
    panelsOpen: Object.keys(panelState.open).filter((k) => panelState.open[k]),
    tool: cave.tool,
    pick: cave.pick.kind,
    stations,
    legs,
    live: { status: live.status, people: live.people.length },
  };
}

/** The report as a zip, and its text (for copying). */
export async function buildReport(d: Draft): Promise<{ name: string; zip: Uint8Array; text: string }> {
  const enc = new TextEncoder();
  const when = new Date();
  const facts = d.includeDetails ? await details() : null;
  const text = [`${KIND_LABEL[d.kind]}`, `Sent ${when.toISOString()}`, '', d.message.trim() || '(no message)', ...(facts ? ['', 'About the app:', JSON.stringify(facts, null, 2)] : [])].join('\n');
  // report.json is what the feedback receiver reads: an id, the kind, a one-line summary and where it came from.
  const id = Array.from(crypto.getRandomValues(new Uint8Array(3)), (b) => b.toString(16).padStart(2, '0')).join('');
  const first = d.message.trim().split('\n')[0].slice(0, 80);
  const info = await loadBuildInfo().catch(() => null);
  const attachments = ['feedback.txt', ...(d.includeLog ? ['log.txt'] : []), ...(d.screenshot ? ['screenshot.png'] : [])];
  const report = { id, type: d.kind, summary: first || KIND_LABEL[d.kind], email: '', os: navigator.platform || '', cavecad: { version: `web ${info ? info.git + (info.dirty ? '+' : '') : 'unknown'}` }, attachments };
  const files = [{ name: 'report.json', data: enc.encode(JSON.stringify(report)) }, { name: 'feedback.txt', data: enc.encode(text) }];
  if (d.includeLog) files.push({ name: 'log.txt', data: enc.encode(logText()) });
  if (d.screenshot) files.push({ name: 'screenshot.png', data: new Uint8Array(await d.screenshot.arrayBuffer()) });
  const stamp = when.toISOString().replace(/[:T]/g, '-').slice(0, 19);
  return { name: `cavecad-feedback-${stamp}.zip`, zip: zip(files, when), text };
}

/** Saves bytes as a file download. */
export function download(name: string, bytes: Uint8Array, type = 'application/zip') {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 5000);
}

const ENDPOINT = (import.meta.env.VITE_FEEDBACK_ENDPOINT as string | undefined)?.trim() ?? '';
const KEY = (import.meta.env.VITE_FEEDBACK_KEY as string | undefined)?.trim() ?? '';

/** Standard base64 of some bytes (in pieces, so a big screenshot does not overflow the call stack). */
function base64(bytes: Uint8Array): string {
  let out = '';
  for (let i = 0; i < bytes.length; i += 0x8000) out += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(out);
}

const WHY: Record<string, string> = {
  key: 'The feedback address did not accept this copy of CaveCAD.',
  'too-big': 'The report is too big to send. Leave the screenshot out, or save the report instead.',
  busy: 'Too many reports were sent this hour. Try again later, or save the report instead.',
  'bad-zip': 'The report could not be read.',
  'no-report': 'The report could not be read.',
};

/** Sends the report to CaveCAD's feedback address, when this copy has one. Returns what happened in words. */
export async function send(name: string, bytes: Uint8Array): Promise<{ ok: boolean; message: string }> {
  try {
    if (ENDPOINT && KEY) {
      // Plain text, so the browser sends it without asking permission first; the receiver unpacks it.
      const r = await fetch(`${ENDPOINT}?k=${encodeURIComponent(KEY)}`, { method: 'POST', headers: { 'content-type': 'text/plain;charset=utf-8' }, body: base64(bytes) });
      const out = (await r.json().catch(() => null)) as { ok?: boolean; error?: string } | null;
      if (out?.ok) return { ok: true, message: 'Sent. Thank you.' };
      return { ok: false, message: `${WHY[out?.error ?? ''] ?? `Not sent (${out?.error ?? r.status}).`} Save the report instead.` };
    }
    const r = await fetch(`/feedback?name=${encodeURIComponent(name)}`, { method: 'POST', headers: { 'content-type': 'application/zip' }, body: bytes as BodyInit });
    if (r.status === 404 || r.status === 405) return { ok: false, message: 'This copy of CaveCAD has no place to send reports. Save the report and pass it on.' };
    return r.ok ? { ok: true, message: 'Sent. Thank you.' } : { ok: false, message: `Not sent (${r.status}). Save the report instead.` };
  } catch {
    return { ok: false, message: 'Could not reach the feedback address. Save the report instead.' };
  }
}
