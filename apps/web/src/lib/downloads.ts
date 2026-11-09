// The desktop downloads offered in the web app: which computer this is, and the installers in the order to show.
export type Platform = 'mac' | 'windows' | 'linux';
export type Manifest = { version: string; builtAt: string; files: Record<string, string> };
export type Choice = { platform: Platform; label: string; file: string; steps: string };

export function platformOf(ua: string, uaPlatform: string): Platform | null {
  const s = `${uaPlatform} ${ua}`;
  if (/Android|iPhone|iPad|iOS/i.test(s)) return null;
  if (/Mac/i.test(s)) return 'mac';
  if (/Win/i.test(s)) return 'windows';
  if (/Linux|X11/i.test(s)) return 'linux';
  return null;
}

const ALL: { platform: Platform; key: string; label: string; steps: string }[] = [
  { platform: 'mac', key: 'mac', label: 'Mac (Apple silicon and Intel)', steps: 'Open the downloaded file and drag CaveCAD to Applications. The first time, right-click CaveCAD, choose Open, then Open again. Your Mac asks only once.' },
  { platform: 'windows', key: 'windows', label: 'Windows installer', steps: 'Run the installer. If Windows says it protected your PC, click More info, then Run anyway.' },
  { platform: 'windows', key: 'windowsMsi', label: 'Windows (.msi, for managed PCs)', steps: 'Run the .msi. If Windows says it protected your PC, click More info, then Run anyway.' },
  { platform: 'linux', key: 'linuxAppImage', label: 'Linux AppImage', steps: 'Make the file executable (chmod +x, or Properties → Allow executing), then open it.' },
  { platform: 'linux', key: 'linuxDeb', label: 'Linux .deb (Ubuntu, Debian)', steps: 'Open it with your software installer, or: sudo apt install ./the-file.deb' },
  { platform: 'linux', key: 'linuxRpm', label: 'Linux .rpm (Fedora)', steps: 'Open it with your software installer, or: sudo dnf install ./the-file.rpm' },
];

export function choices(m: Manifest, mine: Platform | null): Choice[] {
  const list = ALL.filter((c) => m.files[c.key]).map((c) => ({ platform: c.platform, label: c.label, file: m.files[c.key], steps: c.steps }));
  const first = mine ?? 'mac';
  return [...list.filter((c) => c.platform === first), ...list.filter((c) => c.platform !== first)];
}
