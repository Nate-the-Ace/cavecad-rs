#!/usr/bin/env bash
# One command to try live editing between devices (for example this laptop and an iPad).
# Run it on the laptop; it prints the address to open on every device, then serves the app.
set -e
cd "$(dirname "$0")/../apps/web"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

npm install --no-audit --no-fund # quick when nothing changed; picks up new dependencies

# The laptop's address on the local network (Mac first, then Linux).
ip=""
for nic in en0 en1; do
  ip=$(ipconfig getifaddr "$nic" 2>/dev/null || true)
  [ -n "$ip" ] && break
done
[ -n "$ip" ] || ip=$(hostname -I 2>/dev/null | awk '{print $1}')
[ -n "$ip" ] || ip="<this laptop's address>"

cat <<MSG

  ================ CaveCAD live editing test ================
   1. On the laptop AND the iPad, open:   http://$ip:5173
      (both devices must be on the same Wi-Fi)
   2. On each: View tab > Live editing > type the SAME room name
      (for example "test") > Join.
   3. Draw on one; it appears on the other. Ctrl+C here to stop.
  ===========================================================

MSG

exec npm run dev
