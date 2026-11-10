/** Survey files' text from their bytes. UTF-8 when the bytes are UTF-8; otherwise the encoding the
 *  file declares (Therion's `encoding iso8859-2` line), else Windows-1252 -- what Compass and Walls
 *  write. Reading every file as UTF-8 turned a team's "Müller" into "M�ller". */
export function decodeSurveyText(bytes: Uint8Array): string {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    // not UTF-8: fall through to what the file says, or the usual single-byte code page
  }
  const head = new TextDecoder('latin1').decode(bytes.subarray(0, 4096));
  const declared = /^\s*encoding\s+(\S+)/im.exec(head)?.[1];
  for (const label of [declared && therionLabel(declared), 'windows-1252']) {
    if (!label) continue;
    try {
      return new TextDecoder(label).decode(bytes);
    } catch {
      // a label this browser does not know: try the next
    }
  }
  return new TextDecoder('utf-8').decode(bytes);
}

/** Therion's encoding names (`iso8859-2`, `cp1250`) as the labels a TextDecoder knows. */
function therionLabel(name: string): string {
  const n = name.toLowerCase();
  const iso = /^iso-?8859-?(\d+)$/.exec(n);
  if (iso) return `iso-8859-${iso[1]}`;
  const cp = /^(?:cp|windows-?)(\d{3,4})$/.exec(n);
  if (cp) return `windows-${cp[1]}`;
  return n;
}
