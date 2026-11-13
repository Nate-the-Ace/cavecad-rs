# cavecad-app MCP server

Lets Claude (or any MCP client) drive a running **test build** of the CaveCAD desktop app without touching the
mouse, keyboard or window: run script in the page, screenshot the window even when it is covered, read the log,
and press, type or read by label.

- App side: `apps/desktop/src-tauri/src/bridge.rs` (cargo feature `debug-bridge`) and `apps/web/src/lib/debug-bridge.ts`.
  Only test builds have it (`gh workflow run desktop.yml --ref <branch> -f channel=dev`). Test builds also keep running at
  full speed when covered (`tauri.testbuild.conf.json`, `backgroundThrottling: disabled`).
- It listens on 127.0.0.1 only (port 42285, or any free one) and every request needs the key in
  `~/Library/Application Support/app.cavecad.desktop/debug-bridge.json` (mode 600, rewritten at each start).
- Register once: `claude mcp add --scope user cavecad-app -- node <repo>/tools/cavecad-app-mcp/server.mjs`

Tools: `cavecad_app_status`, `cavecad_app_eval`, `cavecad_app_screenshot`, `cavecad_app_logs`, `cavecad_app_find`,
`cavecad_app_click`, `cavecad_app_type`, `cavecad_app_key`, `cavecad_app_text`.

A local test build without CI: `npx tauri build --debug --no-bundle --features debug-bridge --config
src-tauri/tauri.testbuild.conf.json` in `apps/desktop` (web built with `VITE_UPDATE_CHANNEL=dev`).
