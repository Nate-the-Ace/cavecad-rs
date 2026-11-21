# LiDAR captures on the server

One CaveCAD Capture shot is one `.cvcap` file. The server keeps it per cave, named by its SHA-256 (stored
once, deleted when no cave's shot uses it). Code: `src/captures.ts`, table `cave_captures`.

Who may: cave **owners and editors** upload and delete; **watchers** list and download; anyone else gets 404.
All routes need the usual sign-in (session cookie or `Authorization: Bearer`).

## Routes

| Route | Who | Answer |
|---|---|---|
| `GET /api/caves/:cave/captures` | readers | `{captures:[{shotId, sha, size, stationId, stationLabel, capturedAt, device, addedBy, addedAt}]}`, newest `capturedAt` first. `stationId` `""` = waiting for a station. `addedBy` is a member id. |
| `PUT /api/caves/:cave/captures/:sha?offset=N&total=T` | writers | one chunk; see below |
| `GET` or `HEAD /api/caves/:cave/captures/:sha/upload` | writers | `{have}`: bytes of this upload the server holds (0 if none). `HEAD` gives it in the `Upload-Offset` header. |
| `GET /api/caves/:cave/captures/:sha` | readers | the file bytes, `application/octet-stream`, `Cache-Control: private, max-age=31536000, immutable`, `Accept-Ranges: bytes`. Send `Range: bytes=a-b` (or `a-`, or `-n`) to get `206` with `Content-Range`; a range past the end gives `416`. |
| `DELETE /api/caves/:cave/captures/:shotId` | writers | `{ok:true}`; the file goes if nothing else uses it. Send `Content-Type: application/json` (every non-GET request needs it; an empty body is fine). |

`:sha` is the lowercase hex SHA-256 of the whole file. `:shotId` is the header's `shot_id`.

## Uploading a shot, step by step

1. Hash the whole `.cvcap` file: SHA-256, lowercase hex. `T` = its size in bytes.
2. Ask where to start: `GET /api/caves/:cave/captures/:sha/upload` → `{have}`. A new upload is `have: 0`.
3. Send the bytes from `have` in chunks of at most **8 MiB**:
   `PUT /api/caves/:cave/captures/:sha?offset=<have>&total=<T>`, header `Content-Type: application/octet-stream`,
   body = the chunk.
   - `200 {have}`: stored; send the next chunk from `have`.
   - `409 {error:"offset", have}`: the server holds a different amount (a chunk got lost or arrived twice).
     Carry on from that `have`.
   - `200 {capture:{...}}`: the last chunk arrived, the file was checked and kept. Done; the object is the row
     the list will show.
4. After a lost connection or an app restart, go back to step 2. The part waits on the server for 48 hours.
5. If the server already holds a file with this SHA-256, the **first** `PUT` (any offset, any chunk) finishes
   at once with `200 {capture}`. So resending a finished shot, or repeating a last chunk whose answer was lost,
   is safe and cheap.

When the last byte arrives the server checks, in order:

- SHA-256 of the whole file equals `:sha`, else `422 {error:"bad-sha"}`;
- the file starts with `CVCAP\0`, a `u16` format ≥ 1 and a `u32` header length, and the header JSON parses with a
  `header` object holding a `shot_id` (1-100 of `A-Z a-z 0-9 _ . : -`) and a finite `captured_at_ms`, else
  `422 {error:"bad-capture"}`;
- `header.cave_id` equals `:cave`, else `422 {error:"wrong-cave"}`.

On any 422 the upload is thrown away; start again from offset 0 after fixing the file.
`shotId`, `stationId`, `stationLabel`, `capturedAt` (from `captured_at_ms`) and `device` come from that header
(text fields cut at 200 characters). Uploading a file for a `shot_id` the cave already has **replaces** that
shot's row (e.g. after the shot is placed on a station); the old file is deleted if nothing else uses it.

## Limits and errors

- Whole file: at most **512 MiB** (`total` larger → `413 {error:"too-big"}`).
- One chunk: at most **8 MiB** (`413`). A chunk that would run past `total`, an empty chunk, or a missing /
  non-numeric `offset` or `total` → `400 {error:"bad-request"}`.
- Watcher uploading or deleting → `403 {error:"watcher"}`. Not in the cave, unknown shot or file → `404`.
- Any type other than `application/octet-stream` on the chunk `PUT` → `415`. Cross-site browser requests are
  refused as everywhere else in the API.

## Deployment

- `CAPTURES_DIR`: the folder for kept files. Default: `captures/` beside the database file (like `scans/`).
  Unfinished uploads live in `CAPTURES_DIR/uploads/` as `<cave>-<sha>.part`; parts untouched for 48 hours are
  deleted when a new upload starts.
- Disk: each shot is 10-30 MB (up to ~200 MB); plan for the total of all caves' shots plus in-flight parts.
- Deleting a cave deletes its rows, its unused files and its unfinished parts.
- The server never logs cave data.
