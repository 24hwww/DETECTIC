#!/usr/bin/env python3
import os, time
from http.server import HTTPServer, BaseHTTPRequestHandler

ROOT = os.environ.get('PACKAGE_ROOT', os.path.dirname(os.path.abspath(__file__)))
LOG = os.path.join(ROOT, 'proof_log.txt')

class H(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        print(f"{time.strftime('%Y-%m-%dT%H:%M:%S%z')} {fmt % args}", flush=True)
    def do_GET(self):
        path = self.path.split('?')[0].lstrip('/')
        if '..' in path or path.startswith('.'):
            self.send_error(403)
            return
        fpath = os.path.normpath(os.path.join(ROOT, path))
        if not fpath.startswith(ROOT) or not os.path.isfile(fpath):
            self.send_error(404)
            return
        with open(fpath, 'rb') as f:
            data = f.read()
        self.send_response(200)
        if fpath.endswith('.sh'):
            self.send_header('Content-Type', 'text/plain')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def do_PUT(self):
        length = int(self.headers.get('Content-Length') or 0)
        body = self.rfile.read(length) if length else b''
        tag = 'unknown'
        if '?' in self.path:
            q = self.path.split('?', 1)[1]
            for kv in q.split('&'):
                if kv.startswith('tag='):
                    tag = kv.split('=', 1)[1]
        ts = time.strftime('%Y-%m-%dT%H:%M:%S%z')
        with open(LOG, 'a') as f:
            f.write(f'===== {ts} PROOF tag={tag} =====\n')
            f.write(body.decode('utf-8', 'replace'))
            f.write('\n')
        self.log_message(f'PUT {self.path} tag={tag} {length} bytes')
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'ok\n')

if __name__ == '__main__':
    open(LOG, 'w').close()
    HTTPServer.allow_reuse_address = True
    HTTPServer(('192.168.0.27', 8080), H).serve_forever()
