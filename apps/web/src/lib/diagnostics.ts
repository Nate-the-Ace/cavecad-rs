// Keeps the last few errors the page itself raised (a crash in the web code, a failed promise), so a
// feedback report can include them next to the engine's log. Nothing here is sent anywhere.

const KEEP = 100;
const seen: string[] = [];

function add(kind: string, text: string) {
  seen.push(`${new Date().toISOString()} ${kind}: ${text.slice(0, 2000)}`);
  if (seen.length > KEEP) seen.shift();
}

let installed = false;
/** Starts remembering page errors (once). */
export function installDiagnostics() {
  if (installed) return;
  installed = true;
  window.addEventListener('error', (e) => add('error', `${e.message} (${e.filename?.split('/').pop() ?? ''}:${e.lineno})`));
  window.addEventListener('unhandledrejection', (e) => add('rejected promise', String(e.reason?.stack ?? e.reason)));
}

/** The page errors remembered so far, oldest first. */
export function pageErrors(): string[] {
  return [...seen];
}
