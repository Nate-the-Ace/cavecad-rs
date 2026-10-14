import type { CellName, NotebookPort } from '../src/lib/survey-writer.ts';
import type { NotebookPage } from '../src/lib/notebook.ts';

export type { CellName };

export interface Row { id: string; from: string; to: string | null; cells: Partial<Record<CellName, number | null>> }

/**
 * A stand-in for the engine's notebook, with the behaviours the writer relies on: add-shot answers only
 * `ok`; a reading out of range is refused; and the page lays splays out FIRST (not last), so a writer
 * that assumed "the new shot is the last row" would pick the wrong one.
 */
export function fakeNotebook(fail?: (shot: string, which: CellName) => boolean) {
  const rows: Row[] = [];
  const walls: Record<string, number | null> = {};
  let n = 0;
  const port: NotebookPort = {
    page: () =>
      ({ rows: [...rows.filter((x) => x.to === null), ...rows.filter((x) => x.to !== null)].map((x) => ({ kind: x.to === null ? 'splay' : 'shot', shot: x.id, label: x.from })) }) as unknown as NotebookPage,
    addShot: (_t, from, to, splay) => {
      if (!from.trim()) return 'error: a shot starts at a named station';
      if (!splay && (!to.trim() || from === to)) return 'error: a shot joins two different stations';
      rows.push({ id: `s${++n}`, from, to: splay ? null : to, cells: {} });
      return 'ok';
    },
    setReading: (shot, which, value) => {
      const row = rows.find((x) => x.id === shot);
      if (!row) return 'error: no such shot';
      if (fail?.(shot, which)) return 'error: that value is out of range';
      if (value !== null && ((which === 'distance' && value < 0) || (which.startsWith('inc') && Math.abs(value) > 90))) return 'error: that value is out of range';
      row.cells[which] = value;
      return 'ok';
    },
    setWall: (_t, station, side, value) => {
      if (!station.trim()) return 'error: no station';
      walls[`${station}:${side}`] = value;
      return 'ok';
    },
    removeShot: (shot) => {
      const i = rows.findIndex((x) => x.id === shot);
      if (i < 0) return 'error: no such shot';
      rows.splice(i, 1);
      return 'ok';
    },
  };
  return { rows, walls, port };
}

