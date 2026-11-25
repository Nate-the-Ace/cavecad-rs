// The phone's exports of the open cave (Settings > Cave): the drawing as DXF and the station checklist as CSV, handed
// over by the share sheet where there is one, otherwise downloaded (lib/deliver-file.ts).
import { cave } from './cave.svelte';
import { engine } from './engine';
import { checklistCsv, checklistName } from './station-table';
import { deliverFile, type Delivery } from './deliver-file';

const said = (how: Delivery | null, name: string): string | null => (how === null ? null : `${how === 'shared' ? 'Shared' : 'Downloaded'} ${name}`);

/** The drawing as a DXF (what the desktop's Export DXF writes). What to tell the person, or null if they cancelled. */
export async function exportCaveDxf(): Promise<string | null> {
  const text = engine.saveDxf(cave.adjusted);
  if (!text) return 'No cave is open.';
  const name = `${cave.fileName.replace(/\.[^.]+$/, '')}.dxf`;
  return said(await deliverFile(name, text, 'application/dxf'), name);
}

/** Every station as the checklist CSV (the Stations table's download, unfiltered). */
export async function exportStationsCsv(): Promise<string | null> {
  const rows = engine.stationRows(cave.adjusted);
  if (rows.length === 0) return 'No stations yet.';
  const name = checklistName(cave.fileName);
  return said(await deliverFile(name, checklistCsv(rows, cave.units), 'text/csv'), name);
}
