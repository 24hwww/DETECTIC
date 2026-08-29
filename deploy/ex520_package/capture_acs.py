#!/usr/bin/env python3
"""Temporary CWMP Inform capture server for EX520."""
import os, sys, time
from http.server import HTTPServer, BaseHTTPRequestHandler

HOST = os.environ.get("PACKAGE_HOST", "192.168.0.27")
PORT = int(os.environ.get("PACKAGE_PORT", "8080"))
ROOT = os.environ.get("PACKAGE_ROOT", os.path.dirname(os.path.abspath(__file__)))
LOG = os.path.join(ROOT, "acs_capture.txt")

def log(msg):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    print(f"{ts} {msg}", flush=True)

class CaptureHandler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        log(fmt % args)

    def do_GET(self):
        # Simple file serving for package files
        path = self.path
        if path.startswith('/'):
            path = path[1:]
        if '?' in path:
            path = path.split('?')[0]
        # security: keep inside ROOT
        if '..' in path:
            self.send_response(403); self.end_headers(); return
        fpath = os.path.normpath(os.path.join(ROOT, path))
        if not fpath.startswith(ROOT):
            self.send_response(403); self.end_headers(); return
        if fpath == ROOT or os.path.isdir(fpath):
            fpath = os.path.join(fpath, 'index.html')
        if not os.path.exists(fpath):
            self.send_response(404); self.end_headers(); return
        with open(fpath, 'rb') as f:
            data = f.read()
        self.send_response(200)
        if fpath.endswith('.sh'):
            self.send_header('Content-Type', 'text/plain')
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        parsed = __import__('urllib.parse').parse.urlparse(self.path)
        length = int(self.headers.get('Content-Length') or 0)
        body = self.rfile.read(length) if length else b''
        ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
        with open(LOG, "a") as f:
            f.write(f"===== {ts} {self.path} ({dict(self.headers)}) =====\n")
            f.write(body.decode("utf-8", "replace"))
            f.write("\n\n")
        log(f"captured {length} bytes to {LOG}")
        # Return a generic 200 with empty body so cwmp continues
        self.send_response(200)
        self.send_header('Content-Type', 'text/plain')
        self.end_headers()
        self.wfile.write(b"ok\n")

if __name__ == "__main__":
    os.chdir(ROOT)
    log(f"capture server on {HOST}:{PORT}")
    HTTPServer.allow_reuse_address = True
    server = HTTPServer((HOST, PORT), CaptureHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
