#!/usr/bin/env python3
"""Detectic Forwarder — bridges EX520 sensor to Cloudflare Worker.

The EX520 binary doesn't support TLS. This forwarder:
  1. Receives HTTP from EX520 on port 8082
  2. Forwards to https://detectic.24hwww.workers.dev via HTTPS
  3. Returns the Worker response to the EX520

Runs on the host machine (192.168.0.27) that the EX520 can reach.

Usage:
    python3 forwarder.py
    # or with config:
    BACKEND_URL=https://detectic.24hwww.workers.dev/api/v1/events \
    python3 forwarder.py --port 8082
"""

import argparse
import os
import ssl
import urllib.request
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DEFAULT_BACKEND = "https://detectic.24hwww.workers.dev/api/v1/events"
BACKEND_URL = os.environ.get("DETECTIC_BACKEND_URL") or DEFAULT_BACKEND

# SSL context that verifies Cloudflare certificates
SSL_CTX = ssl.create_default_context()


class ForwarderHandler(BaseHTTPRequestHandler):
    """Proxy all POST /api/v1/* requests to the Cloudflare Worker."""

    def do_POST(self):
        if not self.path.startswith("/api/v1/"):
            return self._json(404, {"error": "not found"})

        # Read body from EX520
        length = int(self.headers.get("Content-Length") or 0)
        if length <= 0 or length > 4 * 1024 * 1024:
            return self._json(400, {"error": "bad body size"})
        body = self.rfile.read(length)

        # Forward headers
        headers = {
            "Content-Type": "application/json",
            "User-Agent": "detectic-sensor/0.2.0",
            "X-Detectic-Sensor": self.headers.get("X-Detectic-Sensor", ""),
            "X-Detectic-Signature": self.headers.get("X-Detectic-Signature", ""),
        }

        # Forward to Cloudflare Worker
        url = BACKEND_URL.replace("/api/v1/events", self.path)
        req = urllib.request.Request(url, data=body, headers=headers, method="POST")

        try:
            with urllib.request.urlopen(req, timeout=15, context=SSL_CTX) as resp:
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
            self._json(502, {"error": f"forward failed: {e}"})

    def do_GET(self):
        """Health check for the forwarder itself."""
        if self.path == "/healthz":
            return self._json(200, {
                "status": "ok",
                "backend": BACKEND_URL,
                "forwarder": "active"
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
        # Quiet logging
        pass


def main():
    ap = argparse.ArgumentParser(description="Detectic HTTP→HTTPS forwarder")
    ap.add_argument("--host", default="0.0.0.0")
    ap.add_argument("--port", type=int, default=8082)
    args = ap.parse_args()

    httpd = ThreadingHTTPServer((args.host, args.port), ForwarderHandler)
    print(f"[forwarder] listening on http://{args.host}:{args.port}")
    print(f"[forwarder] backend → {BACKEND_URL}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
