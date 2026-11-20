"""Laser-out-the-top view of the phone samples since <start ms>: roll of the laser round the camera axis (0 = up),
laser inclination, camera tilt, every 0.5 s.  python3 -I roll.py <start_ms> [step_s]"""
import json, math, os, sys
start = int(sys.argv[1]); step = float(sys.argv[2]) if len(sys.argv) > 2 else 0.5
rows = [json.loads(l) for l in open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "samples.jsonl"))]
rows = [r for r in rows if r["t"] >= start]
def mat(a, b, g):
    a, b, g = map(math.radians, (a, b, g)); ca, sa, cb, sb, cg, sg = math.cos(a), math.sin(a), math.cos(b), math.sin(b), math.cos(g), math.sin(g)
    return [[ca*cg-sa*sb*sg, -cb*sa, ca*sg+cg*sa*sb], [cg*sa+ca*sb*sg, ca*cb, sa*sg-ca*cg*sb], [-cb*sg, sb, cb*cg]]
AXES = [(0, 1), (0, -1), (1, 1), (1, -1)]   # (device column, sign): +x, -x, +y, -y
def axis(r, ax):
    m = mat(r["a"], r["b"], r["g"]); return [ax[1] * m[i][ax[0]] for i in range(3)]
LASER = max(AXES, key=lambda ax: axis(rows[0], ax)[2]) if rows else (1, 1)
def view(r):
    m = mat(r["a"], r["b"], r["g"]); y = axis(r, LASER); z = [-m[i][2] for i in range(3)]
    d = z[2]; up = [-d*z[0], -d*z[1], 1 - d*z[2]]; n = math.sqrt(sum(c*c for c in up)); up = [c/n for c in up]
    cr = [up[1]*y[2]-up[2]*y[1], up[2]*y[0]-up[0]*y[2], up[0]*y[1]-up[1]*y[0]]
    roll = math.degrees(math.atan2(sum(cr[i]*z[i] for i in range(3)), sum(up[i]*y[i] for i in range(3))))
    return roll, math.degrees(math.asin(max(-1, min(1, y[2])))), r["pitch"]
nxt = start
for r in rows:
    if r["t"] >= nxt:
        roll, inc, tilt = view(r)
        print(f"{(r['t']-start)/1000:5.1f}s  roll {roll:7.1f}  laser incl {inc:6.1f}  camera tilt {tilt:5.1f}")
        nxt = r["t"] + step * 1000
