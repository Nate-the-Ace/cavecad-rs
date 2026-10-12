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
  const files = [{ name: 'feedback.txt', data: enc.encode(text) }];
  if (d.includeLog) files.push({ name: 'log.txt', data: enc.encode(logText()) });
  if (d.screenshot) files.push({ name: 'screenshot.png', data: new Uint8Array(await d.screenshot.arrayBuffer()) });
  const stamp = when.toISOString().replace(/[:T]/g, '-').slice(0, 19);
  return { name: `cavecad-feedback-${stamp}.zip`, zip: zip(files, when), text };
}

/** Takes a picture of this tab with the browser's screen capture (it asks first). Null if declined or unavailable. */
export async function captureScreen(): Promise<Blob | null> {
  const md = navigator.mediaDevices as MediaDevices & { getDisplayMedia?: (o?: object) => Promise<MediaStream> };
  if (!md?.getDisplayMedia) return null;
  let stream: MediaStream | null = null;
  try {
    stream = await md.getDisplayMedia({ video: true, preferCurrentTab: true, selfBrowserSurface: 'include' });
    const video = document.createElement('video');
    video.srcObject = stream;
    video.muted = true;
    await video.play();
    await new Promise((r) => setTimeout(r, 250)); // let the first real frame arrive
    const c = document.createElement('canvas');
    c.width = video.videoWidth;
    c.height = video.videoHeight;
    c.getContext('2d')?.drawImage(video, 0, 0);
    return await new Promise((res) => c.toBlob(res, 'image/png'));
  } catch {
    return null;
  } finally {
    stream?.getTracks().forEach((t) => t.stop());
  }
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

/** Sends the report to this site's feedback address, when it has one. Returns what happened in words. */
export async function send(name: string, bytes: Uint8Array): Promise<{ ok: boolean; message: string }> {
  try {
    const r = await fetch(`/feedback?name=${encodeURIComponent(name)}`, { method: 'POST', headers: { 'content-type': 'application/zip' }, body: bytes as BodyInit });
    if (r.status === 404 || r.status === 405) return { ok: false, message: 'This copy of CaveCAD has no place to send reports. Save the report and pass it on.' };
    return r.ok ? { ok: true, message: 'Sent. Thank you.' } : { ok: false, message: `Not sent (${r.status}). Save the report instead.` };
  } catch {
    return { ok: false, message: 'Could not reach the server. Save the report instead.' };
  }
}
