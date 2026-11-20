#!/usr/bin/env python3
"""OM7 phone wedge: tips the phone (in its case) about its long axis so the gimbal's roll motor steers the camera.

    python3 make_wedge.py [--angle 45] [--case-w 86.4] [--case-l 170.2] [--case-t 15.2] [--out wedge.stl]

Writes a binary STL in print orientation: standing on its bottom end (the lip on the bed), the phone's long axis
up, so the tipped cradle, arm and tongue all rise straight from the bed: no supports (the cross bars bridge). Standard library only. The part is a set of convex blocks that overlap; slicers (Bambu,
Prusa, Orca, Cura) merge overlapping shells.

Parts (gimbal frame: x across the clamp, y along the roll axis away from the gimbal, z up the phone):
  tongue  - phone-sized plate the OM7 clamp grips (78 x 9 mm, inside the clamp's 67-84 x 6.9-10 range)
  arm     - solid block from the tongue's front to the back of the cradle
  cradle  - a frame tipped `angle` deg about z, the phone's screen against it, a lip under the phone's bottom edge,
            ears on both long edges that keep two rubber bands from sliding
"""
import argparse
import math
import struct

def rot_z(p, a):
    c, s = math.cos(a), math.sin(a)
    return (p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2])

def add(a, b): return (a[0] + b[0], a[1] + b[1], a[2] + b[2])
def sub(a, b): return (a[0] - b[0], a[1] - b[1], a[2] - b[2])
def mul(a, k): return (a[0] * k, a[1] * k, a[2] * k)
def cross(a, b): return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])
def dot(a, b): return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]

# A hexahedron from 8 corners: bottom face 0-1-2-3, top face 4-5-6-7 (4 above 0, ...), as 12 outward triangles.
FACES = [(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
         (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]

def hexa(c):
    tris = [(c[a], c[b], c[d]) for a, b, d in FACES]
    # make every triangle face away from the block's centre (robust to corner order)
    mid = mul(tuple(map(sum, zip(*c))), 1 / 8)
    out = []
    for t in tris:
        n = cross(sub(t[1], t[0]), sub(t[2], t[0]))
        out.append(t if dot(n, sub(t[0], mid)) > 0 else (t[0], t[2], t[1]))
    return out

def box(x0, x1, y0, y1, z0, z1, frame=None):
    """Axis box in local (u, n, v) coordinates, mapped by `frame` (a function) to the gimbal frame."""
    pts = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
           (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    return hexa([frame(p) if frame else p for p in pts])

def build(angle, case_w, case_l, case_t):
    a = math.radians(angle)
    tongue_w, tongue_t, tongue_h = 78.0, 9.0, 70.0
    plate_t, frame_w, rim = 5.0, 70.0, 12.0
    lip_h = case_t + 4.0              # lip deeper than the case is thick
    # cradle centre far enough forward that the tipped phone clears the tongue and clamp jaws (10 mm margin)
    half_reach = (case_w / 2) * math.sin(a) + plate_t
    d = tongue_t + 10.0 + half_reach
    centre = (0.0, d, 0.0)

    def cradle(p):                    # local (u across, n out of the plate, v up) -> gimbal frame
        return add(rot_z((p[0], p[1], p[2]), a), centre)

    tris = []
    # frame: the phone's centre sits at the cradle centre; bottom at -case_l/2 (on the lip)
    z0, z1 = -case_l / 2 - plate_t, case_l * 0.22
    # tongue: centred on the phone's middle where the clamp grips it, run down to the bed so it prints standing
    tris += box(-tongue_w / 2, tongue_w / 2, 0, tongue_t, z0, tongue_h / 2)
    hw = frame_w / 2
    tris += box(-hw, -hw + rim, -plate_t, 0, z0, z1, cradle)
    tris += box(hw - rim, hw, -plate_t, 0, z0, z1, cradle)
    tris += box(-hw, hw, -plate_t, 0, z0, z0 + rim + plate_t, cradle)
    tris += box(-hw, hw, -plate_t, 0, z1 - rim, z1, cradle)
    tris += box(-hw, hw, -plate_t, 0, -rim / 2, rim / 2, cradle)          # middle bar
    # lip under the phone's bottom edge
    tris += box(-hw, hw, 0, lip_h, z0, z0 + plate_t, cradle)
    # band ears: pairs of pegs 12 mm apart on both long edges, at two heights
    for zc in (-case_l * 0.30, case_l * 0.05):
        for side in (-1, 1):
            x_in, x_out = (hw, hw + 6) if side > 0 else (-hw - 6, -hw)
            for dz in (-9, 6):
                tris += box(x_in, x_out, -plate_t, 0, zc + dz, zc + dz + 3, cradle)
    # arm: tongue front face to the cradle's back face (a hexahedron, front face on the cradle plane)
    aw, a_lo, a_hi = 26.0, z0, 30.0   # arm also stands on the bed
    back = [(-aw / 2, tongue_t - 1, a_lo), (aw / 2, tongue_t - 1, a_lo),
            (aw / 2, tongue_t - 1, a_hi), (-aw / 2, tongue_t - 1, a_hi)]
    front = [cradle((-aw / 2, -plate_t + 1, a_lo)), cradle((aw / 2, -plate_t + 1, a_lo)),
             cradle((aw / 2, -plate_t + 1, a_hi)), cradle((-aw / 2, -plate_t + 1, a_hi))]
    c = [back[0], back[1], front[1], front[0], back[3], back[2], front[2], front[3]]
    tris += hexa(c)
    # print orientation: the gimbal frame as is, z (the phone's long axis) up, bottom end on the bed
    zmin = min(p[2] for t in tris for p in t)
    return [tuple((p[0], p[1], p[2] - zmin) for p in t) for t in tris], d

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
    tris, d = build(a.angle, a.case_w, a.case_l, a.case_t)
    write_stl(a.out, tris)
    xs = [p[i] for t in tris for p in t for i in (0,)]
    ys = [p[1] for t in tris for p in t]
    zs = [p[2] for t in tris for p in t]
    print(f"{a.out}: {len(tris)} triangles, size {max(xs)-min(xs):.1f} x {max(ys)-min(ys):.1f} x {max(zs)-min(zs):.1f} mm, "
          f"cradle {d:.1f} mm in front of the clamp")
