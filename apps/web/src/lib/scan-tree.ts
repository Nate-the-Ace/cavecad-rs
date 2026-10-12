// The scans folder as a tree: which files are scans, and the rows a tree shows. Pure, so it is
// checked without a browser. Paths are relative to the scans folder with `/` between folders.

export type ScanKind = 'image' | 'pdf';

const IMAGE = /\.(png|jpe?g|webp|gif|bmp|svg|avif)$/i;
const PDF = /\.pdf$/i;

/** What a path is as a scan, or `null` when it is not one (or is junk the system leaves about). */
export function scanKind(path: string): ScanKind | null {
  const parts = path.split('/');
  if (parts.some((p) => p.startsWith('.') || p === '__MACOSX')) return null;
  if (IMAGE.test(path)) return 'image';
  if (PDF.test(path)) return 'pdf';
  return null;
}

export interface TreeRow {
  kind: 'folder' | 'file';
  /** Folder or file path. */
  path: string;
  name: string;
  depth: number;
  /** Folders: how many scans are inside, all levels. */
  count: number;
  collapsed: boolean;
}

const natural = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' });

/**
 * The rows to show: folders first (then files) at each level, names in the order a person reads
 * them (`p2` before `p10`), a collapsed folder hiding everything under it.
 */
export function treeRows(paths: string[], collapsed: ReadonlySet<string>, filter = ''): TreeRow[] {
  const q = filter.trim().toLowerCase();
  const scans = paths.filter((p) => scanKind(p) && (!q || p.toLowerCase().includes(q)));
  type Node = { folders: Map<string, Node>; files: string[] };
  const root: Node = { folders: new Map(), files: [] };
  for (const p of scans) {
    const parts = p.split('/');
    let node = root;
    for (const d of parts.slice(0, -1)) {
      if (!node.folders.has(d)) node.folders.set(d, { folders: new Map(), files: [] });
      node = node.folders.get(d)!;
    }
    node.files.push(p);
  }
  const total = (n: Node): number => n.files.length + [...n.folders.values()].reduce((s, f) => s + total(f), 0);
  const rows: TreeRow[] = [];
  const walk = (node: Node, prefix: string, depth: number) => {
    for (const name of [...node.folders.keys()].sort(natural)) {
      const path = prefix + name;
      const child = node.folders.get(name)!;
      const closed = collapsed.has(path);
      rows.push({ kind: 'folder', path, name, depth, count: total(child), collapsed: closed });
      if (!closed) walk(child, path + '/', depth + 1);
    }
    for (const p of [...node.files].sort(natural)) rows.push({ kind: 'file', path: p, name: p.slice(p.lastIndexOf('/') + 1), depth, count: 0, collapsed: false });
  };
  walk(root, '', 0);
  return rows;
}
