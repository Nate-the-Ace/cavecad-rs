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

/// Reads a .cvcap and places it from its first heading tap. bundle: len BYTES, valid for len.
/// station_and_target: pointer to exactly 6 doubles (station xyz then target xyz, survey frame);
/// may be NULL, in which case the shot is not placed.
/// Returns JSON {"vertices","triangles","keyframes","placed":bool,"min":[3],"max":[3],"turn_deg"} or {"error"}.
/// "placed" is false when there is no usable tap, no bearing, or the mesh is empty.
CcBuf cc_shot_summary(const uint8_t *bundle, size_t len, const double *station_and_target);

/// Survey-frame positions of a .cvcap's mesh (placed from station_and_target), for the viewer:
/// little-endian float32 x,y,z per vertex, 12 bytes per vertex. bundle is len BYTES;
/// station_and_target is 6 doubles as above (NULL allowed). Empty when it cannot be placed.
CcBuf cc_shot_placed_positions(const uint8_t *bundle, size_t len, const double *station_and_target);

/// Releases a CcBuf. Call exactly once per returned buffer.
void cc_free(CcBuf buf);
