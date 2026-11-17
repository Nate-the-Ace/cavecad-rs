"""Gimbal Watch receiver: serves index.html and appends the phone's orientation samples to samples.jsonl.
Run: python3 -I server.py [port]  (then `tailscale serve --bg <port>` so the phone gets https)."""
import http.server, json, sys, os
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "samples.jsonl")

class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = open(os.path.join(HERE, "index.html"), "rb").read()
        self.send_response(200); self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Cache-Control", "no-store"); self.end_headers(); self.wfile.write(body)
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        rows = json.loads(self.rfile.read(n) or b"[]")
        with open(OUT, "a") as f:
            for r in rows: f.write(json.dumps(r) + "\n")
        self.send_response(204); self.end_headers()
    def log_message(self, *a): pass

http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1]) if len(sys.argv) > 1 else 8765), H).serve_forever()
