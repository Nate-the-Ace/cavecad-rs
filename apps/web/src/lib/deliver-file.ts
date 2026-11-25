// Hands a file the app made (an export) to the person. Where the browser can share files (a phone: iOS Safari, and
// WebViews that expose the Web Share API) it opens the share sheet, which offers Save to Files, AirDrop, Mail, ...;
// elsewhere it downloads, as the desktop always has. Returns how it went, or null when the person cancelled.
// The dock's exports (Home > Export, the Stations panel's CSV) go through `deliverExport`: share-first on an iPad or
// iPhone only (where a download link does nothing in the app's WebView), the download as before everywhere else.

export type Delivery = 'shared' | 'downloaded';

/** Downloads a file through a temporary link (the web download path every browser has). */
export function downloadFile(name: string, data: Uint8Array | string, type: string): void {
  const url = URL.createObjectURL(new Blob([data as BlobPart], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** An iPhone or iPad, from what the browser says (iPadOS calls itself a Mac with touch). Pure, for tests. */
export function isIOSDevice(userAgent: string, platform: string, maxTouchPoints: number): boolean {
  return /iPad|iPhone|iPod/.test(userAgent) || (/Mac/.test(platform) && maxTouchPoints > 1);
}

/** This device is an iPhone or iPad. */
export const isIOS = (): boolean => typeof navigator !== 'undefined' && isIOSDevice(navigator.userAgent, navigator.platform ?? '', navigator.maxTouchPoints ?? 0);

/** An export from the dock: the share sheet on iOS (falling back to a download), a plain download elsewhere. */
export async function deliverExport(name: string, data: Uint8Array | string, type: string): Promise<Delivery | null> {
  if (isIOS()) return deliverFile(name, data, type);
  downloadFile(name, data, type);
  return 'downloaded';
}

/** The share sheet when files can be shared, otherwise a download. */
export async function deliverFile(name: string, data: Uint8Array | string, type: string): Promise<Delivery | null> {
  const nav = navigator as Navigator & { canShare?: (d: ShareData) => boolean };
  if (typeof File !== 'undefined' && nav.share && nav.canShare) {
    const file = new File([data as BlobPart], name, { type });
    if (nav.canShare({ files: [file] })) {
      try {
        await nav.share({ files: [file] });
        return 'shared';
      } catch (e) {
        if ((e as Error)?.name === 'AbortError') return null; // the person closed the sheet
        // NotAllowedError (no user gesture left) or a WebView without file sharing: fall back to a download
      }
    }
  }
  downloadFile(name, data, type);
  return 'downloaded';
}
