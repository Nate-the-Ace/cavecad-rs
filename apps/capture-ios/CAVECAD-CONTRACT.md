# What Capture relies on in CaveCAD

Capture is a separate app, so every fact below is a promise CaveCAD keeps. Each one names the CaveCAD
code that owns it. **Before changing Capture's server or cave code, re-read that code, not this file
or the plan.** (2026-11-20: the plan said `"ops":[...]`; CaveCAD writes `"ops":"[...]"`; every real
cave failed to open on the phone. The tests passed because they were written from the plan.)

| Fact | Owner (read this) | Capture code |
| --- | --- | --- |
| `GET /api/caves` gives `{caves:[{id, storage, label:{locked}, ...}]}`; only `storage == "open"` caves are plain text; an open cave's name is `JSON.parse(label.locked).name` | `apps/server/src/rooms.ts` `myCaves`; `apps/web/src/lib/room-client.ts` (listing, `storageOf`) | `CaveStore.parseCaves` |
| Every `/api/caves…` call needs `X-Cave-Device`, a device id from `POST /api/devices {publicKey}` (43-char base64url, 32 bytes); same key gives the same id | `apps/server/src/rooms-api.ts` 65-73, `rooms.ts` | `Account.device()` |
| Snapshot: `GET /api/caves/:id/snapshot` → `{snapshot:{upTo, keyVersion, locked}}` or `{snapshot:null}`; open only when `keyVersion == 0`; `locked` is JSON text `{"upTo":n,"ops":"<ops JSON text>"}` and the inner `upTo` must equal the outer | `room-client.ts` `readOpenSnapshot` | `CaveStore.download`, `cave_capture::document_from_room` |
| Entries: `GET /api/caves/:id/stream?after=n` NDJSON: `{"t":"entry","number","keyVersion","locked","authorDevice"}` lines, then `{"t":"live"}`, then pings; an open entry's `locked` is `{"c":client,"ops":"<ops JSON text>"}` | `rooms-api.ts` `stream`/`entryLine`, `streams.ts` `goLive`, `room-client.ts` `readOpenEntry` | `CaveStore.parseStream`, `document_from_room` |
| Ops are `Vec<cave_doc::Op>` (serde); CaveCAD loads a snapshot with `sync_load` (all must apply) and entries with `sync_apply` (bad ones skipped) | `crates/cave-wasm/src/lib.rs` `sync_load`, `sync_apply` | `document_from_room` (skips bad ops in both) |
| Station positions: `extract` → `resolve` → `adjust` (when it converges), labels `Field::StationLabel`, order by `Placed.seq`; the map's "adjustment off" is not followed | `crates/cave-scene/src/solved.rs` `solve` | `cave_capture::stations` |
| A shot waiting for its station (typed label, empty `station_id`) is placed by the station whose `Field::StationLabel` matches, ignoring case and spaces; the first in survey order wins a tie | `crates/cave-scene/src/solved.rs` `solve` (labels) | `cave_capture::label_key`, `StationLabel.find` |
| Card chaining: shots at neighbouring stations (survey legs when known, else labels with the same prefix and numbers one apart) are joined through their CURRENT/FORWARD/REAR cards and placed as a chain; derived each time, nothing written to the shot. CaveCAD placing shots must use the same result to agree with the phone | (none yet in CaveCAD) | `cave_capture::chain::chains`, `cc_chains`, `Core.chains` |
| Sign-in: `/signin?desktop=1&challenge&state` → `cavecad://auth?code&state` → `POST /api/desktop-token {code, verifier}` → `{token}`; then `Authorization: Bearer` | `apps/server/src/signin.ts`, `handoff.ts`, `api.ts` | `Account` |

Debugging on the phone: `apps/capture-ios/pull-log.sh <udid>` copies `Documents/capture.log` and a
screenshot to `apps/capture-ios/build/device/` and prints the log's tail.
