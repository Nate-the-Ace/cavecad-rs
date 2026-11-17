"""Gimbal Watch receiver: serves index.html (phone gyro page), /view (live 3D of X1 shots from shots.jsonl) and appends the phone's orientation samples to samples.jsonl.
Run: python3 -I server.py [port]  (then `tailscale serve --bg <port>` so the phone gets https)."""
import http.server, json, sys, os, subprocess, threading
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "samples.jsonl")
RUN = os.path.join(os.path.dirname(HERE), "run-swift.sh")
LOG = os.path.join(HERE, "ring.log")
ring = {"proc": None}

def start_ring(step):
    """Runs the closed-loop ring (run-swift.sh … ring:<step>) in the background; one at a time."""
    if ring["proc"] and ring["proc"].poll() is None:
        return False
    log = open(LOG, "w")
    ring["proc"] = subprocess.Popen([RUN, "--shots", os.path.join(HERE, "shots.jsonl"), "steps", "wait:500", f"ring:{step}"],
                                    stdout=log, stderr=subprocess.STDOUT)
    return True

class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        path = self.path.split("?")[0]
        if path.endswith("/shots"):
            f = os.path.join(HERE, "shots.jsonl")
            body = open(f, "rb").read() if os.path.exists(f) else b""
            self.send_response(200); self.send_header("Content-Type", "text/plain")
            self.send_header("Cache-Control", "no-store"); self.end_headers(); self.wfile.write(body); return
        if path.endswith("/ringstatus"):
            running = bool(ring["proc"] and ring["proc"].poll() is None)
            tail = open(LOG).read().splitlines()[-200:] if os.path.exists(LOG) else []
            keep = [l for l in tail if l.startswith(("ring", "  phi", "  start", "  after pan", "  pan fix", "no X1", "no Osmo"))]
            self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Cache-Control", "no-store"); self.end_headers()
            self.wfile.write(json.dumps({"running": running, "lines": keep[-6:]}).encode()); return
        page = "view.html" if path.rstrip("/").endswith("view") else "index.html"
        body = open(os.path.join(HERE, page), "rb").read()
        self.send_response(200); self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Cache-Control", "no-store"); self.end_headers(); self.wfile.write(body)
    def do_POST(self):
        if self.path.split("?")[0].endswith("/ring"):
            q = dict(kv.split("=", 1) for kv in (self.path.split("?", 1) + [""])[1].split("&") if "=" in kv)
            step = max(2, min(30, int(float(q.get("step", "10")))))
            ok = start_ring(step)
            self.send_response(200 if ok else 409); self.send_header("Content-Type", "application/json"); self.end_headers()
            self.wfile.write(json.dumps({"started": ok, "step": step}).encode()); return
        n = int(self.headers.get("Content-Length", 0))
        rows = json.loads(self.rfile.read(n) or b"[]")
        with open(OUT, "a") as f:
            for r in rows: f.write(json.dumps(r) + "\n")
        self.send_response(204); self.end_headers()
    def log_message(self, *a): pass

http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1]) if len(sys.argv) > 1 else 8765), H).serve_forever()
