#!/usr/bin/env python3
"""OM7 phone wedge: tips the phone (in its case) about its long axis so the gimbal's roll motor steers the camera.

    python3 make_wedge.py [--angle 45] [--case-w 86.4] [--case-l 170.2] [--case-t 15.2] [--out osmo-wedge-45.stl]

Designed to print with NO supports and NO bridges in one orientation, standing as the STL is written:
  - the body is one cross-section (clamp tongue + arm + tipped plate) extruded straight up: vertical walls only;
  - the base (the first 4 mm) is that cross-section plus the lip the phone stands on, on the bed;
  - band guides are triangular ridges with 45 deg faces on the plate's two long edges.

Standard library only. The part is a set of convex prisms that overlap; slicers merge overlapping shells.

Frame: x across the clamp, y along the roll axis away from the gimbal, z up the phone's long axis (= print up).
"""
import argparse
import math
import struct


def sub(a, b): return (a[0] - b[0], a[1] - b[1], a[2] - b[2])
def cross(a, b): return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])
def dot(a, b): return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def prism(poly, axis_lo, axis_hi, place):
    """A convex polygon `poly` (2D points, either winding) extruded from axis_lo to axis_hi; `place(p2, h)` maps a
    polygon point at extrusion height h to 3D. Returns outward-facing triangles."""
    lo = [place(p, axis_lo) for p in poly]
    hi = [place(p, axis_hi) for p in poly]
    n = len(poly)
    tris = []
    for i in range(1, n - 1):
        tris.append((lo[0], lo[i + 1], lo[i]))
        tris.append((hi[0], hi[i], hi[i + 1]))
    for i in range(n):
        j = (i + 1) % n
        tris.append((lo[i], lo[j], hi[j]))
        tris.append((lo[i], hi[j], hi[i]))
    pts = lo + hi
    mid = tuple(sum(p[k] for p in pts) / len(pts) for k in range(3))
    out = []
    for t in tris:
        nrm = cross(sub(t[1], t[0]), sub(t[2], t[0]))
        out.append(t if dot(nrm, sub(t[0], mid)) > 0 else (t[0], t[2], t[1]))
    return out


def build(angle, case_w, case_l, case_t):
    a = math.radians(angle)
    ca, sa = math.cos(a), math.sin(a)
    tongue_w, tongue_t = 78.0, 9.0
    plate_t, plate_w = 5.0, 74.0
    base_h = 4.0
    lip = case_t + 4.0
    # cradle centre far enough forward that the tipped phone clears the tongue and the clamp jaws (10 mm margin)
    d = tongue_t + 10.0 + (case_w / 2) * sa + plate_t

    def cradle(u, n):
        """Plate-local (u across, n out of the plate towards the phone) -> (x, y)."""
        return (u * ca - n * sa, d + u * sa + n * ca)

    # the phone stands on the base; its centre (where the clamp must grip for balance) is base_h + case_l / 2 up
    centre_z = base_h + case_l / 2
    tongue_top = centre_z + 35.0
    height = tongue_top                      # the whole body is this tall; the phone's top (camera) stands free above
    hw = plate_w / 2
    up = lambda p, h: (p[0], p[1], h)

    tongue = [(-tongue_w / 2, 0), (tongue_w / 2, 0), (tongue_w / 2, tongue_t), (-tongue_w / 2, tongue_t)]
    plate = [cradle(-hw, -plate_t), cradle(hw, -plate_t), cradle(hw, 0), cradle(-hw, 0)]
    arm = [(-13, tongue_t - 1), (13, tongue_t - 1), cradle(13, -plate_t + 1), cradle(-13, -plate_t + 1)]
    lip_poly = [cradle(-hw, -plate_t), cradle(hw, -plate_t), cradle(hw, lip), cradle(-hw, lip)]

    tris = []
    for poly in (tongue, plate, arm):
        tris += prism(poly, 0, height, up)
    tris += prism(lip_poly, 0, base_h, up)
    # band guides: on each long edge of the plate, two ridges 10 mm apart make a channel for a band; each ridge is a
    # triangle (45 deg below and above) sticking 4 mm out past the plate edge, through the plate's thickness
    for zc in (centre_z - case_l * 0.25, centre_z + case_l * 0.08):
        for side in (-1, 1):
            for z_mid in (zc - 7, zc + 7):
                tri = [(side * hw, z_mid - 4), (side * (hw + 4), z_mid), (side * hw, z_mid + 4)]
                tris += prism(tri, -plate_t, 0, lambda p, n: (*cradle(p[0], n), p[1]))
    return tris, d, height, centre_z


def write_stl(path, tris):
    with open(path, "wb") as f:
        f.write(b"OM7 phone wedge (CaveCAD Capture)".ljust(80, b" "))
        f.write(struct.pack("<I", len(tris)))
        for t in tris:
            n = cross(sub(t[1], t[0]), sub(t[2], t[0]))
            l = math.sqrt(dot(n, n)) or 1.0
            f.write(struct.pack("<3f", *(x / l for x in n)))
            for p in t:
                f.write(struct.pack("<3f", *p))
            f.write(b"\0\0")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--angle", type=float, default=45.0)
    ap.add_argument("--case-w", type=float, default=86.4)
    ap.add_argument("--case-l", type=float, default=170.2)
    ap.add_argument("--case-t", type=float, default=15.2)
    ap.add_argument("--out", default="osmo-wedge-45.stl")
    a = ap.parse_args()
    tris, d, h, cz = build(a.angle, a.case_w, a.case_l, a.case_t)
    write_stl(a.out, tris)
    xs = [p[0] for t in tris for p in t]
    ys = [p[1] for t in tris for p in t]
    zs = [p[2] for t in tris for p in t]
    print(f"{a.out}: {len(tris)} triangles, {max(xs)-min(xs):.1f} x {max(ys)-min(ys):.1f} x {max(zs)-min(zs):.1f} mm, "
          f"cradle {d:.1f} mm in front of the clamp, phone centre (clamp grip) {cz:.1f} mm up")
