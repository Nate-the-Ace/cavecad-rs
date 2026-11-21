#pragma once
#include <stdint.h>
#include <stddef.h>

/// Bytes owned by Rust. Release with cc_free. Every CcBuf returned by any cc_ function must be
/// passed to cc_free exactly once (an empty buffer too); never free it any other way.
typedef struct { uint8_t *ptr; size_t len; } CcBuf;

/// Stations of a cave. snapshot may be NULL; entries_json may be NULL (no entries). Both are
/// NUL-terminated UTF-8; invalid UTF-8 gives an error. entries_json is a JSON array of entry texts.
/// Returns UTF-8 JSON: [{"id","label","x","y","z"}] or {"error": "..."}.
CcBuf cc_stations(const char *snapshot, const char *entries_json);

/// Builds a .cvcap. header_json (required, NUL-terminated UTF-8): ShotHeader plus
/// "keyframes": [{"pose":[16],"intrinsics":[9],"width","height","jpeg_len"}].
/// Counts are NUMBERS OF ELEMENTS, not vertices or triangles, and each pointer must be valid for
/// that many elements:
///   positions, n_positions: number of float32 values (3 per vertex).
///   normals, n_normals: number of float32 values (3 per vertex, same as positions).
///   indices, n_indices: number of uint32 values (3 per triangle).
///   jpegs, n_jpegs: number of BYTES; all keyframe JPEGs back to back, as the header's jpeg_len say.
/// A pointer may be NULL only when its count is 0; NULL with a count > 0 returns empty.
/// Returns the bundle bytes, or an empty buffer on any error.
CcBuf cc_shot_encode(const char *header_json,
                     const float *positions, size_t n_positions,
                     const float *normals, size_t n_normals,
                     const uint32_t *indices, size_t n_indices,
                     const uint8_t *jpegs, size_t n_jpegs);

/// Reads a .cvcap and places it: a compass shot (header "compass" set) from its station alone, the
/// target half of station_and_target ignored (pass the station twice); an older tap shot from its
/// first heading tap, aimed at the target. bundle: len BYTES, valid for len.
/// station_and_target: pointer to exactly 6 doubles (station xyz then target xyz, survey frame);
/// may be NULL, in which case the shot is not placed.
/// Returns JSON {"vertices","triangles","keyframes","passes" (the header's scanning-pass count; 0 for
/// an older one-pass file),"placed":bool,"station_id","station_label","waiting":bool,"target_station_id",
/// "compass":{"true_north","accuracy_deg"}|null,"min":[3],"max":[3],"turn_deg",
/// "cards":[{"id","word","point":[3],"sightings","spread_m"}]} or {"error"}.
/// "cards" (only when placed) are the station cards the scan saw, placed like the mesh; "point" is
/// RELATIVE TO THE STATION, survey frame (x east, y north, z up), metres.
/// station_id is the station the shot was taken over; "waiting" is true when it is empty (a label typed
/// before the cave had it: station_label is that label, matched ignoring case and spaces); target_station_id the first heading tap's
/// station (null when there is no tap, as in every compass shot); compass is null for a tap shot.
/// With a NULL station_and_target only the header is read (cheap) and the shot is not placed.
/// min/max are absolute survey coordinates. "placed" is false when a tap shot has no usable tap or
/// bearing, when a number is not finite, or when the mesh is empty.
CcBuf cc_shot_summary(const uint8_t *bundle, size_t len, const double *station_and_target);

/// Survey-frame positions of a .cvcap's mesh (placed from station_and_target as cc_shot_summary
/// does: compass shots ignore the target), for the viewer:
/// little-endian float32 x,y,z per vertex, 12 bytes per vertex, RELATIVE TO THE STATION (the first
/// three doubles of station_and_target): add the station's xyz for absolute survey coordinates.
/// (Relative keeps millimetres; float32 of a large absolute coordinate does not.) bundle is len BYTES;
/// station_and_target is 6 doubles as above (NULL allowed). Empty when it cannot be placed.
CcBuf cc_shot_placed_positions(const uint8_t *bundle, size_t len, const double *station_and_target);

/// As cc_shot_placed_positions, thinned on a cubic grid of cell_m metres: one point per occupied cell,
/// at the mean of its points, in the order the cells were first entered (deterministic). Non-finite
/// points are dropped; a cell_m that is not positive and finite thins nothing. Little-endian float32
/// x y z per point, relative to the station. Empty when it cannot be placed. For overviews/thumbnails.
CcBuf cc_shot_placed_points_thin(const uint8_t *bundle, size_t len, const double *station_and_target,
                                 float cell_m);

/// Adds a scanning pass to a .cvcap (bundle, len BYTES) and returns the new bundle, or empty on any
/// error. pass_json: {"captured_at_ms","aligned_by":"same_session"|"cards","transform":[16 column-major,
/// pass frame to shot frame],"replace":bool (true: the pass mesh replaces the shot's, same session;
/// false: added),"tracking","cards":[CardSeen],"keyframes":[as cc_shot_encode]}. Mesh and jpegs as
/// cc_shot_encode (counts are elements; jpegs bytes back to back).
CcBuf cc_shot_add_pass(const uint8_t *bundle, size_t len, const char *pass_json,
                       const float *positions, size_t n_positions,
                       const float *normals, size_t n_normals,
                       const uint32_t *indices, size_t n_indices,
                       const uint8_t *jpegs, size_t n_jpegs);

/// The shot's mesh simplified on a cell_m grid, with its hole outlines (shorter than min_outline_m
/// dropped). Little-endian binary: u32 vertex_floats, u32 index_count, u32 outline_count; then
/// vertex_floats f32 (xyz per vertex, capture frame); index_count u32; then per outline u32
/// point_count and point_count*3 f32. Empty on a bad bundle.
CcBuf cc_shot_ghost(const uint8_t *bundle, size_t len, float cell_m, float min_outline_m);

/// Station cards of a new pass (cards_json: [CardSeen], the pass's capture frame) against the shot's.
/// same_session != 0: {"offsets":[{"id","m"}]} (plain distances). Else {"fit":{"transform":[16],
/// "yaw_rad","worst_m","ok","worst_ratio","misfits":[{"id","m"}],"limits":[{"id","m"}]}} or {"fit":null} with fewer than 2 matched cards.
/// "limits" is the misfit allowed per card, max(0.05, 0.015 x its farthest horizontal distance to
/// another matched card in the shot), metres; "ok" = every misfit within its limit (the line-up
/// is accepted); "worst_ratio" = max(misfit / limit), above 1 means refused.
/// {"error"} on a bad bundle or JSON.
CcBuf cc_cards_fit(const uint8_t *bundle, size_t len, const char *cards_json, uint8_t same_session);

/// Card chaining: every saved shot of a cave placed together (spec "Card chaining"). Shots at
/// neighbouring stations (survey legs when given and both stations are known, else labels: same
/// prefix, numbers one apart, A3 -> A4) are joined through their CURRENT/FORWARD/REAR cards; joined
/// shots form chains, placed as a whole through any station the cave has, or floating around the
/// chain's first shot while all their stations are waiting. Only headers are read: no mesh or photo
/// is copied.
/// stations_json: NUL-terminated UTF-8, NULL for none: either cc_stations' array [{"id","label","x",
/// "y","z"}] or {"stations":[...],"legs":[["from_id","to_id"],...]}.
/// bundles, lens: n pointers to .cvcap bytes and their lengths (BYTES); NULL only when n is 0.
/// Returns {"shots":[one per input, in order],"chains":[{"k","shots":[input indices],"labels":[..],
/// "placed":bool}],"counts":{"shots","chains","by_chain","flagged"}} or {"error"}. Each shot:
/// {"index","shot_id","label","source":"chain"|"card+compass"|"compass"|"tap"|"floating"|"waiting"|
/// "unplaced","placed":bool (in the cave),"chain":k|null,"pair":[6]|null,"fix":{"yaw_deg","shift":[3]}
/// |null,"spot":[3]|null,"turn_deg"|null,"joined":[labels],"one_point":bool,"misfit":bool,
/// "flags":[{"kind":"leg_misfit","with","lidar_m","survey_m","off_m","limit_m"}|{"kind":"cards_misfit",
/// "with","off_m","limit_m"}|{"kind":"one_point","with"}]}, or {"index","error","source":"unplaced",
/// "placed":false} for an unreadable shot.
/// To draw a shot: place it with "pair" through cc_shot_placed_positions / _thin / cc_shot_summary
/// (points relative to pair[0..3]), then move each point r to R(yaw_deg) r + shift (R turns
/// counter-clockwise seen from above, about z; survey frame). For a shot whose station is not in the
/// cave, "pair" is where its chain placed it (to the mm), or the origin when it is not placed
/// (floating chains: relative to the chain's first shot's station). "spot" is the shot's own station point after the fix, relative to pair[0..3]. A
/// leg misfit (LiDAR vs survey station-to-station beyond max(10 cm, 3%)) is not used for chaining.
CcBuf cc_chains(const char *stations_json, const uint8_t *const *bundles, const size_t *lens, size_t n);

/// The gimbal (Osmo Mobile 7 over Bluetooth): one runner on its own thread. The doors below queue a
/// command or flip a flag and return at once; poll cc_gimbal_status for what happened.
///
/// Looks for the gimbal for timeout_ms and connects (state connecting, then idle, or error with a
/// message, e.g. with no Bluetooth).
void cc_gimbal_connect(uint32_t timeout_ms);

/// Queues a plan: JSON [[{"yaw","roll","pitch","seconds"}, ...], ...], target-mode targets in degrees
/// relative to home and move seconds, each list 1 to 15 targets ending at home. Returns {"ok":true},
/// or {"error"} for a refused plan (nothing sent). A run ends in state done or error.
CcBuf cc_gimbal_run(const char *plan_json);

/// Pauses the running plan on its next reached target (state holding; target mode kept alive).
void cc_gimbal_hold(void);

/// Ends a hold.
void cc_gimbal_go_on(void);

/// Stops the running plan at its next reached target and sends the gimbal home (state homing, then done).
void cc_gimbal_stop_home(void);

/// Disconnects the gimbal (after a running plan ends) and resets the status.
void cc_gimbal_disconnect(void);

/// {"connected":bool,"state":"idle"|"connecting"|"moving"|"holding"|"homing"|"done"|"error",
/// "step":targets reached,"steps":targets in the plan,"mount_ok":bool (joints known and the roll
/// joint within 80..112: portrait upright),"joints":[pan,tilt,roll] degrees|null,"error":string|null}.
CcBuf cc_gimbal_status(void);

/// The slow sweep of the camera window (phone portrait, camera out) at speed degrees per second, as a plan for
/// cc_gimbal_run: JSON [[{"yaw","roll","pitch","seconds"}, ...], ...]; [] for a non-positive speed.
CcBuf cc_camera_sweep(double speed);

/// Plan aimed at missing directions: missing_json is [[yaw, pitch], ...] degrees relative to home. Only the
/// reachable ones are kept, every list ends home; [] when none. {"error"} for unreadable input.
CcBuf cc_camera_fill(const char *missing_json, double speed);

/// Releases a CcBuf. Call exactly once per returned buffer.
void cc_free(CcBuf buf);
