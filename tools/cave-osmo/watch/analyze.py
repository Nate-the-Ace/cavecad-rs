"""Finds the still stretches in the phone's samples since <start ms> and prints each one's yaw/tilt and the change
from the one before.  python3 -I analyze.py <start_ms> [end_ms]"""
import json, sys, os
start = int(sys.argv[1]); end = int(sys.argv[2]) if len(sys.argv) > 2 else 1 << 62
rows = [json.loads(l) for l in open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "samples.jsonl"))]
rows = [r for r in rows if start <= r["t"] <= end]
def wrap(d): return (d + 180) % 360 - 180
stills, i = [], 0
while i < len(rows):
    j = i
    while j + 1 < len(rows) and abs(wrap(rows[j + 1]["yaw"] - rows[i]["yaw"])) < 0.4 and abs(rows[j + 1]["pitch"] - rows[i]["pitch"]) < 0.4:
        j += 1
    if rows[j]["t"] - rows[i]["t"] >= 700:
        seg = rows[i:j + 1]
        stills.append((seg[0]["t"], seg[-1]["t"], sum(r["yaw"] for r in seg) / len(seg), sum(r["pitch"] for r in seg) / len(seg)))
    i = j + 1
prev = None
for a, b, y, p in stills:
    d = "" if prev is None else f"  Δyaw {wrap(y - prev[0]):+7.1f}  Δtilt {p - prev[1]:+6.1f}"
    print(f"{(a - start) / 1000:6.1f}–{(b - start) / 1000:5.1f}s  yaw {y:7.1f}  tilt {p:6.1f}{d}")
    prev = (y, p)

# --- laser out of the clamp's top: at the first still, find the device axis that points most nearly up; that is the
# laser. Print the laser's direction (bearing, inclination) at every still.
import math
def mat(a, b, g):
    a, b, g = map(math.radians, (a, b, g))
    ca, sa, cb, sb, cg, sg = math.cos(a), math.sin(a), math.cos(b), math.sin(b), math.cos(g), math.sin(g)
    return [[ca*cg - sa*sb*sg, -cb*sa, ca*sg + cg*sa*sb], [cg*sa + ca*sb*sg, ca*cb, sa*sg - ca*cg*sb], [-cb*sg, sb, cb*cg]]
def world(r, v):
    m = mat(r["a"], r["b"], r["g"]); return [sum(m[i][k] * v[k] for k in range(3)) for i in range(3)]
if stills:
    first = next(r for r in rows if r["t"] >= stills[0][0])
    axes = {"+x": (1,0,0), "-x": (-1,0,0), "+y": (0,1,0), "-y": (0,-1,0), "+z": (0,0,1), "-z": (0,0,-1)}
    name, ax = max(axes.items(), key=lambda kv: world(first, kv[1])[2])
    print(f"laser = device {name} (points up at rest)")
    for a, b, _, _ in stills:
        r = next(x for x in rows if x["t"] >= a)
        v = world(r, ax)
        print(f"  {(a - start) / 1000:6.1f}s  laser bearing {math.degrees(math.atan2(v[0], v[1])) % 360:6.1f}  inclination {math.degrees(math.asin(max(-1, min(1, v[2])))):6.1f}")
