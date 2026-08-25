#!/usr/bin/env python3
"""Detectic package + /done callback server for the EX520 autostart test.

Serves the static payload from the package directory on port 8080.

All GET requests to /done (with any query string) are logged and return
HTTP 200. The /done endpoint is intentionally not a file; it is used as a
best-effort callback by bootstart.sh and launcher.sh to report status.
"""
import os
import sys
import time
import urllib.parse
from http.server import HTTPServer, SimpleHTTPRequestHandler

HOST = os.environ.get("PACKAGE_HOST", "192.168.0.27")
PORT = int(os.environ.get("PACKAGE_PORT", "8080"))
ROOT = os.environ.get("PACKAGE_ROOT", os.path.dirname(os.path.abspath(__file__)))


def log(msg):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    print(f"{ts} {msg}", flush=True)


class PackageHandler(SimpleHTTPRequestHandler):
    def log_message(self, fmt, *args):
        # Override the default http.server logging to use our flushed format
        log(fmt % args)

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/done":
            qs = urllib.parse.parse_qs(parsed.query)
            qs_str = ", ".join(f"{k}={v[0]}" for k, v in qs.items())
            log(f"done callback: {qs_str}")
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"ok\n")
            return

        # Serve static package files
        super().do_GET()

    def do_POST(self):
        self._handle_sensor_log()

    def do_PUT(self):
        self._handle_sensor_log()

    def _handle_sensor_log(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path != "/sensor_log":
            self.send_response(404)
            self.end_headers()
            return
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b""
        ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
        try:
            with open(os.path.join(ROOT, "sensor_log.txt"), "a") as f:
                f.write(f"===== {ts} ({parsed.query}) =====\n")
                f.write(body.decode("utf-8", "replace"))
                f.write("\n")
        except Exception as e:
            log(f"sensor_log write error: {e}")
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.end_headers()
        self.wfile.write(b"ok\n")


def main():
    os.chdir(ROOT)
    log(f"package server starting on {HOST}:{PORT}, root={ROOT}")
    server = HTTPServer((HOST, PORT), PackageHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
