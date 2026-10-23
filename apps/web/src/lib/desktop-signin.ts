// The desktop app's sign-in (filled in by Task 11). In a browser there is no bearer token: the TinyAuth
// cookie does the work.
/** The desktop app's current sign-in token, or null in a browser. */
export function desktopBearer(): string | null {
  return null;
}
