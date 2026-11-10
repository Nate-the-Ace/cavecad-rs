// The Handbook's pages as one catalog: their order (index.json order is reading order), their groups, which panel
// a page documents, and search over titles and text. Pure, so the checks run it without the app.

export type Entry = { id: string; group: string; title: string; panel?: string };
export type Page = Entry & { text: string };

export type Catalog = {
  pages: Page[];
  byId(id: string): Page | undefined;
  forPanel(panel: string): Page | undefined;
  groups(): { name: string; pages: Page[] }[];
  search(query: string): Page[];
};

export function makeCatalog(index: Entry[], texts: Record<string, string>): Catalog {
  const pages: Page[] = index.map((e) => ({ ...e, text: texts[e.id] ?? '' }));
  return {
    pages,
    byId: (id) => pages.find((p) => p.id === id),
    forPanel: (panel) => pages.find((p) => p.panel === panel),
    groups: () => {
      const out: { name: string; pages: Page[] }[] = [];
      for (const p of pages) {
        let g = out.find((x) => x.name === p.group);
        if (!g) out.push((g = { name: p.group, pages: [] }));
        g.pages.push(p);
      }
      return out;
    },
    search: (query) => {
      const words = query.toLowerCase().split(/\s+/).filter(Boolean);
      if (!words.length) return [];
      const score = (p: Page) => {
        const title = p.title.toLowerCase();
        const text = p.text.toLowerCase();
        if (!words.every((w) => title.includes(w) || text.includes(w))) return 0;
        return words.filter((w) => title.includes(w)).length * 10 + 1;
      };
      return pages.map((p) => ({ p, s: score(p) })).filter((x) => x.s > 0).sort((a, b) => b.s - a.s).map((x) => x.p);
    },
  };
}
