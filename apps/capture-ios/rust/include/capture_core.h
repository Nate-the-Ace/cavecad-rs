#pragma once
#include <stdint.h>
#include <stddef.h>

/// Bytes owned by Rust. Release with cc_free.
typedef struct { uint8_t *ptr; size_t len; } CcBuf;

/// Stations of a cave. snapshot may be NULL; entries is a JSON array of entry texts.
/// Returns UTF-8 JSON: [{"id","label","x","y","z"}] or {"error": "..."}.
CcBuf cc_stations(const char *snapshot, const char *entries_json);

/// Builds a .cvcap. header_json: ShotHeader plus "keyframes": [{"pose":[16],"intrinsics":[9],"width","height","jpeg_len"}].
/// positions/normals: float32 xyz; indices: uint32; jpegs: all keyframe JPEGs back to back.
/// Returns the bundle bytes, or empty on error.
CcBuf cc_shot_encode(const char *header_json,
                     const float *positions, size_t n_positions,
                     const float *normals, size_t n_normals,
                     const uint32_t *indices, size_t n_indices,
                     const uint8_t *jpegs, size_t n_jpegs);

/// Reads a .cvcap and places it from its first heading tap. station/target: survey xyz (6 doubles).
/// Returns JSON {"vertices","triangles","keyframes","placed":bool,"min":[3],"max":[3],"turn_deg"} or {"error"}.
CcBuf cc_shot_summary(const uint8_t *bundle, size_t len, const double *station_and_target);

/// Survey-frame float32 xyz positions of a .cvcap's mesh (placed from station_and_target), for the viewer.
CcBuf cc_shot_placed_positions(const uint8_t *bundle, size_t len, const double *station_and_target);

void cc_free(CcBuf buf);
