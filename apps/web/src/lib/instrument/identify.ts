// Which instrument is this? One Connect button serves every supported instrument, so the one chosen has to be
// told from the others. The Cavway X1 and the SAP5 offer the very same Bluetooth service (Nordic UART), so the
// service alone cannot do it: TopoDroid goes by the name the instrument advertises, and so does this. Pure and
// tested (tests/identify.check.ts).

import { CAVE_BLE_SERVICE } from './caveble.ts';
import { X1_SERVICE } from './cavway.ts';
import { BRIC_MEASURE_SERVICE } from './bric.ts';

export type InstrumentKind = 'x1' | 'sap5' | 'sap6' | 'bric' | 'unsupported';

export interface InstrumentName {
  /** What the instrument advertises its name starts with. */
  prefix: string;
  kind: InstrumentKind;
  /** What to call it. */
  label: string;
}

/** The names the supported instruments advertise (from TopoDroid's device list). */
export const NAMES: InstrumentName[] = [
  { prefix: 'CavwayX1-', kind: 'x1', label: 'Cavway X1' },
  { prefix: 'SAP6_', kind: 'sap6', label: 'SAP6' },
  { prefix: 'SAP_', kind: 'sap6', label: 'SAP6' },
  { prefix: 'SAP5_', kind: 'sap5', label: 'SAP5' },
  { prefix: 'Shetland_', kind: 'sap5', label: 'SAP5' },
  { prefix: 'BRIC4_', kind: 'bric', label: 'BRIC4' },
  { prefix: 'BRIC5-', kind: 'bric', label: 'BRIC5' },
  { prefix: 'BRIC5_', kind: 'bric', label: 'BRIC5' },
  { prefix: 'BRIC-', kind: 'bric', label: 'BRIC' },
  // Known, and in TopoDroid, but not built here yet.
  { prefix: 'DistoXBLE-', kind: 'unsupported', label: 'DistoX BLE' },
];

export interface Identified {
  kind: InstrumentKind;
  label: string;
  /** How it was told: by the advertised name, or (a name this does not know) by what the instrument offers. */
  how: 'name' | 'service';
}

/**
 * Names the instrument from its advertised name and, failing that, the Bluetooth services it offers. A service
 * that only one instrument has settles it (the CaveBLE service is the SAP6's, the 0x58D0 service the BRIC's); the
 * Nordic UART service alone does not (X1, SAP5 and others share it), so `null` is returned and the caller reads
 * the packets to tell.
 */
export function identify(name: string | undefined, services: readonly string[]): Identified | null {
  const n = (name ?? '').trim();
  const byName = NAMES.find((x) => n.startsWith(x.prefix));
  if (byName) return { kind: byName.kind, label: byName.label, how: 'name' };
  const has = (uuid: string) => services.some((s) => s.toLowerCase() === uuid);
  if (has(CAVE_BLE_SERVICE)) return { kind: 'sap6', label: 'SAP6', how: 'service' };
  if (has(BRIC_MEASURE_SERVICE)) return { kind: 'bric', label: 'BRIC', how: 'service' };
  return null;
}

/** Whether the service list says Nordic UART is there (the X1 and SAP5 both), for a name this does not know. */
export const hasNordicUart = (services: readonly string[]): boolean => services.some((s) => s.toLowerCase() === X1_SERVICE);
