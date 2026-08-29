#!/usr/bin/env python3
"""Minimal CWMP InformResponse server to observe follow-up POST."""
import os, sys, time
from http.server import HTTPServer, BaseHTTPRequestHandler

HOST = os.environ.get("PACKAGE_HOST", "192.168.0.27")
PORT = int(os.environ.get("PACKAGE_PORT", "8080"))
ROOT = os.environ.get("PACKAGE_ROOT", os.path.dirname(os.path.abspath(__file__)))
LOG = os.path.join(ROOT, "acs_session.txt")

def log(msg):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    print(f"{ts} {msg}", flush=True)

class AcsHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        log(fmt % args)

    def end_headers(self):
        if not self.headers.get('Connection') == 'close':
            self.send_header('Connection', 'keep-alive')
        super().end_headers()

    def do_POST(self):
        length = int(self.headers.get('Content-Length') or 0)
        body = self.rfile.read(length) if length else b''
        ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
        with open(LOG, "a") as f:
            f.write(f"===== {ts} REQUEST {self.path} =====\n")
            f.write(body.decode("utf-8", "replace"))
            f.write("\n")
        # Find cwmp:ID
        import re
        m = re.search(r'<cwmp:ID[^>]*>([^<]+)</cwmp:ID>', body.decode('utf-8','replace'))
        cid = m.group(1) if m else '1009'
        # Determine if Inform
        if b'cwmp:Inform' in body:
            resp = f'''<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/" SOAP-ENV:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/" xmlns:SOAP-ENC="http://schemas.xmlsoap.org/soap/encoding/" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:cwmp="urn:dslforum-org:cwmp-1-0">
<SOAP-ENV:Header>
<cwmp:ID SOAP-ENV:mustUnderstand="1">{cid}</cwmp:ID>
</SOAP-ENV:Header>
<SOAP-ENV:Body>
<cwmp:InformResponse>
<MaxEnvelopes>1</MaxEnvelopes>
</cwmp:InformResponse>
</SOAP-ENV:Body>
</SOAP-ENV:Envelope>'''.encode('utf-8')
            self.send_response(200)
            self.send_header('Content-Type', 'text/xml; charset=utf-8')
            self.send_header('Content-Length', str(len(resp)))
            self.end_headers()
            self.wfile.write(resp)
            with open(LOG, "a") as f:
                f.write(f"===== {ts} INFORMRESPONSE =====\n")
                f.write(resp.decode('utf-8','replace'))
                f.write("\n\n")
        else:
            # For any other POST, return empty envelope
            resp = f'''<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/" xmlns:cwmp="urn:dslforum-org:cwmp-1-0">
<SOAP-ENV:Header>
<cwmp:ID SOAP-ENV:mustUnderstand="1">{cid}</cwmp:ID>
</SOAP-ENV:Header>
<SOAP-ENV:Body>
</SOAP-ENV:Body>
</SOAP-ENV:Envelope>'''.encode('utf-8')
            self.send_response(200)
            self.send_header('Content-Type', 'text/xml; charset=utf-8')
            self.send_header('Content-Length', str(len(resp)))
            self.end_headers()
            self.wfile.write(resp)
            with open(LOG, "a") as f:
                f.write(f"===== {ts} EMPTY RESPONSE =====\n")
                f.write(resp.decode('utf-8','replace'))
                f.write("\n\n")

if __name__ == "__main__":
    os.chdir(ROOT)
    log(f"ACS inform response server on {HOST}:{PORT}")
    HTTPServer.allow_reuse_address = True
    server = HTTPServer((HOST, PORT), AcsHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
