#!/usr/bin/env python3
"""Detectic package + /done callback server for the EX520 autostart test.

Serves the static payload from the package directory on port 8080.

All GET requests to /done (with any query string) are logged and return
HTTP 200. The /done endpoint is intentionally not a file; it is used as a
best-effort callback by bootstart.sh and launcher.sh to report status.
"""
import os
import re
import sys
import time
import urllib.parse
from http.server import HTTPServer, SimpleHTTPRequestHandler

# Keys whose VALUES must NEVER be persisted to any host-side log.
_SECRET_KEYS = (
    "DETECTIC_PASSWORD",
    "DETECTIC_SECRET",
    "DETECTIC_BACKEND_TOKEN",
    "DETECTIC_SMTP_PASSWORD",
    "DETECTIC_SMTP_USER",
    "DETECTIC_D1_SYNC_URL",
    "PASSWORD",
    "SECRET",
)


def redact_secrets(text):
    """Defense-in-depth: scrub secret VALUES out of any payload before it is
    written to a log. Key names are preserved, values become <REDACTED>."""
    if text is None:
        return text
    pattern = r"(?m)(?i)\b(" + "|".join(re.escape(k) for k in _SECRET_KEYS) + r")(\s*=\s*)([^\n]*)"
    return re.sub(pattern, lambda m: m.group(1) + m.group(2) + "<REDACTED>", text)

HOST = os.environ.get("PACKAGE_HOST", "192.168.0.27")
PORT = int(os.environ.get("PACKAGE_PORT", "8080"))
ROOT = os.environ.get("PACKAGE_ROOT", os.path.dirname(os.path.abspath(__file__)))


def log(msg):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    print(f"{ts} {msg}", flush=True)


class PackageHandler(SimpleHTTPRequestHandler):
    def log_message(self, fmt, *args):
        # Override the default http.server logging to use our flushed format.
        # Redact any secret values that might appear in a raw request line
        # (e.g. /env_line?d=...DETECTIC_PASSWORD=<urlencoded-value>...).
        log(redact_secrets(fmt % args))

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/version":
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            try:
                with open(os.path.join(ROOT, "version"), "rb") as _vf:
                    self.wfile.write(_vf.read())
            except Exception:
                self.wfile.write(b"unknown\n")
            return

        if parsed.path == "/done":
            qs = urllib.parse.parse_qs(parsed.query)
            qs_str = ", ".join(f"{k}={v[0]}" for k, v in qs.items())
            log(f"done callback: {redact_secrets(qs_str)}")
            # Persist the latest done status so the Edge Supervisor can read it.
            try:
                with open(os.path.join(ROOT, "done_log.txt"), "a") as f:
                    f.write(f"{time.strftime('%Y-%m-%dT%H:%M:%S%z')} {redact_secrets(qs_str)}\n")
            except Exception as e:
                log(f"done_log write error: {e}")
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"ok\n")
            return

        if parsed.path == "/env_line":
            qs = urllib.parse.parse_qs(parsed.query)
            n = qs.get("n", ["?"])[0]
            d = qs.get("d", [""])[0]
            # Decode URL-encoded data
            d = urllib.parse.unquote(d)
            d = redact_secrets(d)
            try:
                with open(os.path.join(ROOT, "env_probe_output.txt"), "a") as f:
                    f.write(d + "\n")
            except Exception as e:
                log(f"env_line write error: {e}")
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
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b""
        ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")

        if parsed.path == "/sensor_log":
            fname = "sensor_log.txt"
        elif parsed.path == "/probe_log":
            fname = "probe_log.txt"
        else:
            self.send_response(404)
            self.end_headers()
            return

        try:
            with open(os.path.join(ROOT, fname), "a") as f:
                f.write(f"===== {ts} ({redact_secrets(parsed.query)}) =====\n")
                f.write(redact_secrets(body.decode("utf-8", "replace")))
                f.write("\n")
        except Exception as e:
            log(f"{fname} write error: {e}")
        log(f"{fname} received {length} bytes ({parsed.query})")
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.end_headers()
        self.wfile.write(b"ok\n")


def main():
    os.chdir(ROOT)
    log(f"package server starting on {HOST}:{PORT}, root={ROOT}")
    HTTPServer.allow_reuse_address = True
    server = HTTPServer((HOST, PORT), PackageHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
