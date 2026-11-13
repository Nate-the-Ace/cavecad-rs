// Parts of the app that are still being built and are kept out of sight until they are ready.

/** Connecting a survey instrument (and its simulator) is future work: the Survey tab, the Survey panel and the
 * In cave layout show in dev builds, in admins' test builds of the desktop app (desktop.yml channel=dev), or where
 * `cavecad.instruments` is set to `on` in this browser. */
export const showInstruments: boolean = (() => {
  if (import.meta.env.DEV || import.meta.env.VITE_UPDATE_CHANNEL === 'dev') return true;
  try {
    return localStorage.getItem('cavecad.instruments') === 'on';
  } catch {
    return false;
  }
})();
