// Hands a file the app made (an export) to the person. Where the browser can share files (a phone: iOS Safari, and
// WebViews that expose the Web Share API) it opens the share sheet, which offers Save to Files, AirDrop, Mail, ...;
// elsewhere it downloads, as the desktop always has. Returns how it went, or null when the person cancelled.

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
