#!/usr/bin/env python3
"""Detectic Relay — secure local relay for EX520 → Cloudflare Worker.

Security model:
  - EX520 sends HTTP to this relay (red local, sin internet)
  - This relay forwards to Cloudflare Worker via cloudflared tunnel
  - Only cloudflared has internet access
  - EX520 never directly accesses the internet

The relay is minimal: it receives the sensor payload, adds forwarding
headers, and POSTs to the Cloudflare Worker endpoint.

Usage:
    python3 relay.py --port 8082
    # or
    DETECTIC_RELAY_PORT=8082 python3 relay.py
"""

import argparse
import os
import json
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# The Cloudflare Worker URL (via tunnel or direct)
# When using cloudflared tunnel, this points to the local tunnel endpoint.
# When using direct HTTPS, this points to the Workers URL.
WORKER_URL = os.environ.get(
    "DETECTIC_WORKER_URL",
    "https://detectic.24hwww.workers.dev/api/v1/events"
)

SSL_CTX = None
try:
    import ssl
    SSL_CTX = ssl.create_default_context()
except ImportError:
    pass


class RelayHandler(BaseHTTPRequestHandler):
    """Forward POST /api/v1/* to the Cloudflare Worker."""

    def do_POST(self):
        if not self.path.startswith("/api/v1/"):
            return self._json(404, {"error": "not found"})

        # Read body from EX520
        length = int(self.headers.get("Content-Length") or 0)
        if length <= 0 or length > 4 * 1024 * 1024:
            return self._json(400, {"error": "bad body size"})
        body = self.rfile.read(length)

        # Forward headers (preserve HMAC auth)
        headers = {
            "Content-Type": "application/json",
            "User-Agent": "detectic-relay/0.2.0",
            "X-Detectic-Sensor": self.headers.get("X-Detectic-Sensor", ""),
            "X-Detectic-Signature": self.headers.get("X-Detectic-Signature", ""),
        }

        # Build URL: replace /api/v1/events with the path from the request
        url = WORKER_URL.replace("/api/v1/events", self.path)

        req = urllib.request.Request(url, data=body, headers=headers, method="POST")

        try:
            ctx = SSL_CTX if url.startswith("https") else None
            with urllib.request.urlopen(req, timeout=15, context=ctx) as resp:
                resp_body = resp.read()
                self.send_response(resp.status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(resp_body)))
                self.end_headers()
                self.wfile.write(resp_body)
        except urllib.error.HTTPError as e:
            resp_body = e.read() if e.fp else b"{}"
            self.send_response(e.code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(resp_body)))
            self.end_headers()
            self.wfile.write(resp_body)
        except Exception as e:
            self._json(502, {"error": f"relay failed: {e}"})

    def do_GET(self):
        if self.path == "/healthz":
            return self._json(200, {
                "status": "ok",
                "worker": WORKER_URL,
                "relay": "active"
            })
        return self._json(404, {"error": "not found"})

    def _json(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt, *args):
        pass  # Quiet


def main():
    ap = argparse.ArgumentParser(description="Detectic local relay")
    ap.add_argument("--host", default="0.0.0.0")
    ap.add_argument("--port", type=int, default=8082)
    args = ap.parse_args()

    httpd = ThreadingHTTPServer((args.host, args.port), RelayHandler)
    print(f"[relay] listening on http://{args.host}:{args.port}")
    print(f"[relay] worker → {WORKER_URL}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
