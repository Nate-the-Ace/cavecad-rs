#!/usr/bin/env bash
# Builds the web app for the private claude.ai artifact and leaves the files to publish in <out>
# (default /tmp/cavecad-artifact): index.html plus assets/. Relative URLs (--base=./) because the page
# is served from a sub-path: an absolute /assets/ URL made the engine fail with a 404. The local-only
# data folder (real caves, never committed) is removed from the build.
set -euo pipefail
out="${1:-/tmp/cavecad-artifact}"
cd "$(dirname "$0")/../apps/web"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
npx vite build --base=./ >/dev/null
rm -rf dist/local
rm -rf "$out" && mkdir -p "$out/assets"
cp dist/assets/* "$out/assets/"
js=$(cd "$out/assets" && ls index-*.js)
css=$(cd "$out/assets" && ls index-*.css)
icon=$(base64 < ../web/public/favicon.svg | tr -d '\n')
cat > "$out/index.html" <<HTML
<!doctype html><html><head><meta charset=utf8><meta name=viewport content="width=device-width,initial-scale=1,viewport-fit=cover"><style>:root{color-scheme:light;box-sizing:border-box;padding-top:env(safe-area-inset-top,0px);padding-bottom:env(safe-area-inset-bottom,0px)}html{scroll-padding-top:env(safe-area-inset-top,0px)}body{margin:0;padding:0;font:14px -apple-system,BlinkMacSystemFont,sans-serif;background:#fff;color:#000}img{max-width:100%}[hidden]:not([hidden=until-found i]){display:none!important}</style></head><body>
<title>CaveCAD</title>
<link rel="icon" type="image/svg+xml" href="data:image/svg+xml;base64,$icon">
<link rel="stylesheet" href="./assets/$css">
<style>html,body{height:100%;margin:0}#app{height:100%}</style>
<div id="app"></div>
<script type="module" src="./assets/$js"></script>

</body></html>
HTML
if grep -rIl "truitt" -i "$out" >/dev/null 2>&1; then echo "refusing: real cave data is in the build" >&2; exit 1; fi
echo "built $out: $(ls "$out/assets" | tr '\n' ' ')"
