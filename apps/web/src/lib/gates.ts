// Gates: reasons the phone layout offers nothing but one screen. They are checked in order and the first that holds
// wins: sign-in first (AccountGate covers the page until then), then a cave must be open (only the Caves tab works).
// A later gate is one more entry in GATES. `currentGate` is pure so it can be tested (tests/gates.check.ts).

export type GateReason = 'login' | 'cave';
export interface Gate {
  reason: GateReason;
  message: string;
}
export interface GateState {
  /** Signed in, or running offline on this device's pass (see `signedInPhase`). */
  signedIn: boolean;
  /** A cave is open. */
  caveOpen: boolean;
}

/** One check: the gate it raises, or null when it lets the person through. */
export type GateCheck = (s: GateState) => Gate | null;

/** In order: sign-in, then a cave. */
export const GATES: GateCheck[] = [
  (s) => (s.signedIn ? null : { reason: 'login', message: 'Sign in to continue' }),
  (s) => (s.caveOpen ? null : { reason: 'cave', message: 'Pick or start a cave to continue' }),
];

/** The first gate that holds, or null when everything is open. */
export function currentGate(s: GateState, checks: GateCheck[] = GATES): Gate | null {
  for (const c of checks) {
    const g = c(s);
    if (g) return g;
  }
  return null;
}

/** The account phases in which the app is usable: the same test App.svelte uses to lift the sign-in gate. */
export const signedInPhase = (phase: string): boolean => phase === 'ready' || phase === 'offline' || phase === 'offline-expired';
