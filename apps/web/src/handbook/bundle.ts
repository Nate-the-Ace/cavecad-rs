// The Handbook as the app ships it: every page and screenshot bundled at build time.
import index from './index.json';
import { makeCatalog, type Entry } from './catalog';

const raw = import.meta.glob('./pages/*.md', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
const shots = import.meta.glob('./shots/*.png', { query: '?url', import: 'default', eager: true }) as Record<string, string>;

const texts = Object.fromEntries(Object.entries(raw).map(([path, text]) => [path.replace(/^.*\/(.*)\.md$/, '$1'), text]));
export const catalog = makeCatalog(index as Entry[], texts);
/** A screenshot's address by file name, or null. */
export const shotUrl = (file: string): string | null => shots[`./shots/${file}`] ?? null;
