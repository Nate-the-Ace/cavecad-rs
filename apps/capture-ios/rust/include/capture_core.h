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
/// an older one-pass file),"placed":bool,"station_id","target_station_id",
/// "compass":{"true_north","accuracy_deg"}|null,"min":[3],"max":[3],"turn_deg",
/// "cards":[{"id","word","point":[3],"sightings","spread_m"}]} or {"error"}.
/// "cards" (only when placed) are the station cards the scan saw, placed like the mesh; "point" is
/// RELATIVE TO THE STATION, survey frame (x east, y north, z up), metres.
/// station_id is the station the shot was taken over; target_station_id the first heading tap's
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
/// "yaw_rad","worst_m","misfits":[{"id","m"}]}} or {"fit":null} with fewer than 2 matched cards.
/// {"error"} on a bad bundle or JSON.
CcBuf cc_cards_fit(const uint8_t *bundle, size_t len, const char *cards_json, uint8_t same_session);

/// Releases a CcBuf. Call exactly once per returned buffer.
void cc_free(CcBuf buf);
