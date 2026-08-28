#!/usr/bin/env python3
"""Combined CWMP ACS + package server for EX520 Detectic deployment proof.

Serves:
  - CWMP SOAP on POST /acs
  - package files (bootstart.sh, launcher.sh, detectic.aa/ab/ac, manifest.json, ...)
  - callbacks from bootstart.sh/launcher.sh (/done, /env_line, /heartbeat, /sensor_log)

ACS is idempotent: it triggers the Lifemote/Phoenix bootstart path only when
no recent Detectic heartbeat has been received, or on a cold-boot Inform.
"""
import os, re, time, json
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import parse_qs, urlparse

HOST = os.environ.get("PACKAGE_HOST", "192.168.0.27")
PORT = int(os.environ.get("PACKAGE_PORT", "8080"))
ROOT = os.environ.get("PACKAGE_ROOT", os.path.dirname(os.path.abspath(__file__)))

LOG = os.path.join(ROOT, "acs_session.txt")
EVENT_LOG = os.path.join(ROOT, "detectic_events.txt")
SENSOR_LOG = os.path.join(ROOT, "sensor_log.txt")
STATE_FILE = os.path.join(ROOT, "acs_state.json")

LIFEMOTE_BASE = "Device.X_TP_LIFEMOTE_EXT.LifemoteAgent"
# The URL Lifemote/Phoenix will download and execute.
BOOTSTART_URL = f"http://{HOST}:{PORT}/bootstart.sh"
# No-op URL used when the sensor is healthy, so phoenix stays enabled but idle.
NONE_URL = f"http://{HOST}:{PORT}/none.sh"

# If no heartbeat within this many seconds, the sensor is considered missing.
HEARTBEAT_TIMEOUT = 75

STEP_INFORM = 0
STEP_GETPV_SENT = 1
STEP_SETPV_SENT = 2
STEP_REBOOT_SENT = 3
STEP_DONE = 4

cwmp_state = {
    "step": STEP_INFORM,
    "last_id": 1008,
    "session_count": 0,
    "pending_command": "execute",  # "execute" | "reboot"
    "cold_boot_expected": False,
    "setpv_phase": 0,
    "current_enable": None,
    "current_url": None,
    "target_url": None,
    "last_heartbeat": None,        # timestamp (time.time)
    "last_heartbeat_pid": None,
    "last_done_status": None,
    "last_done_pid": None,
    "last_done_time": None,
    "boot_count": 0,
    "trigger_count": 0,
    "skip_count": 0,
    "reboot_count": 0,
    "force_trigger": False,
}

def _load_state():
    if os.path.exists(STATE_FILE):
        try:
            with open(STATE_FILE) as f:
                loaded = json.load(f)
            # Only restore safe fields, not step/phase state.
            for k in ("last_heartbeat", "last_heartbeat_pid", "last_done_status",
                      "last_done_pid", "last_done_time", "boot_count", "trigger_count",
                      "skip_count", "reboot_count", "force_trigger"):
                if k in loaded:
                    cwmp_state[k] = loaded[k]
            if cwmp_state["last_heartbeat"]:
                # float epoch; keep it as-is
                pass
        except Exception:
            pass

def _save_state():
    try:
        with open(STATE_FILE, "w") as f:
            json.dump({k: cwmp_state[k] for k in (
                "last_heartbeat", "last_heartbeat_pid", "last_done_status",
                "last_done_pid", "last_done_time", "boot_count", "trigger_count",
                "skip_count", "reboot_count", "force_trigger"
            )}, f)
    except Exception:
        pass

def log(msg, path=LOG):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    line = f"{ts} {msg}"
    print(line, flush=True)
    for p in (path, EVENT_LOG):
        with open(p, "a") as f:
            f.write(line + "\n")

def event(msg):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    line = f"{ts} EVENT {msg}"
    print(line, flush=True)
    with open(EVENT_LOG, "a") as f:
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
    n = len(params)
    items = "".join(
        f"<ParameterValueStruct><Name>{name}</Name><Value xsi:type=\"{type_}\">{val}</Value></ParameterValueStruct>"
        for name, val, type_ in params
    )
    body = f'''<cwmp:SetParameterValues>
<ParameterList SOAP-ENC:arrayType="cwmp:ParameterValueStruct[{n}]">
{items}
</ParameterList>
<ParameterKey>detectic-cwmp-bootstrap</ParameterKey>
</cwmp:SetParameterValues>'''
    return build_envelope(cid, body)

def reboot_request(cid):
    body = "<cwmp:Reboot><CommandKey>detectic-cwmp-reboot</CommandKey></cwmp:Reboot>"
    return build_envelope(cid, body)

def _parse_enable_url(body_text):
    enable = None
    url = None
    m = re.search(rf'<Name>{re.escape(LIFEMOTE_BASE)}\.Enable</Name>\s*<Value[^>]*>([^<]+)</Value>', body_text)
    if m:
        enable = m.group(1).strip()
    m = re.search(rf'<Name>{re.escape(LIFEMOTE_BASE)}\.URL</Name>\s*<Value[^>]*>([^<]+)</Value>', body_text)
    if m:
        url = m.group(1).strip()
    return enable, url

def _parse_event_codes(body_text):
    codes = []
    for m in re.finditer(r'<EventCode>([^<]+)</EventCode>', body_text):
        codes.append(m.group(1).strip())
    return codes

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
        for t in sorted(tags, key=len, reverse=True):
            if re.search(rf'<cwmp:{t}(?:\s|>)', body):
                return t
        return None

    def _serve_file(self, path):
        if '..' in path or path.startswith('.'):
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
        self.send_header('Connection', 'close')
        self.end_headers()
        self.wfile.write(data)

    def _record_callback(self, kind, params):
        event(f"{kind} {params}")
        if kind == "heartbeat" and params.get("status") == "running":
            cwmp_state["last_heartbeat"] = time.time()
            cwmp_state["last_heartbeat_pid"] = params.get("pid")
            _save_state()
        if kind == "done":
            cwmp_state["last_done_status"] = params.get("status")
            cwmp_state["last_done_pid"] = params.get("pid")
            cwmp_state["last_done_time"] = time.time()
            if params.get("status") in ("ok", "running"):
                cwmp_state["last_heartbeat"] = time.time()
                cwmp_state["last_heartbeat_pid"] = params.get("pid")
            _save_state()

    def _should_trigger(self, body_text):
        if cwmp_state.pop("force_trigger", False):
            log("force_trigger active: will trigger on next Inform")
            return True, "force_trigger"
        codes = _parse_event_codes(body_text)
        is_cold = any(c in ("0 BOOTSTRAP", "1 BOOT") or c.startswith("M ") for c in codes)
        if is_cold:
            cwmp_state["boot_count"] += 1
            _save_state()
            return True, "cold_boot"
        # Periodic/scheduled: trigger only if no recent heartbeat
        if cwmp_state["last_heartbeat"] is None:
            return True, "no_heartbeat_recorded"
        age = time.time() - cwmp_state["last_heartbeat"]
        if age > HEARTBEAT_TIMEOUT:
            return True, f"heartbeat_stale age={age:.0f}s"
        return False, f"heartbeat_recent age={age:.0f}s"

    def do_GET(self):
        parsed = urlparse(self.path)
        path = parsed.path
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

        if path == 'trigger_now':
            cwmp_state["force_trigger"] = True
            _save_state()
            log("/trigger_now requested: will run bootstart on next Inform")
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain')
            self.end_headers()
            self.wfile.write(b"trigger queued\n")
            return

        if path == 'status':
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain')
            self.end_headers()
            self.wfile.write(json.dumps(cwmp_state, indent=2, default=str).encode() + b"\n")
            return

        if path in ('done', 'env_line', 'heartbeat'):
            qs = parse_qs(parsed.query, keep_blank_values=True)
            params = {k: (v[0] if v else '') for k, v in qs.items()}
            self._record_callback(path, params)
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain')
            self.end_headers()
            self.wfile.write(b"ok\n")
            return

        if not path:
            # index: show package root files
            files = sorted(os.listdir(ROOT))
            body = json.dumps({"files": files, "state": cwmp_state}, default=str, indent=2).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return

        self._serve_file(path)

    def do_PUT(self):
        parsed = urlparse(self.path)
        length = int(self.headers.get('Content-Length') or 0)
        body = self.rfile.read(length) if length else b''
        tag = 'unknown'
        filename = 'unknown'
        if parsed.query:
            qs = parse_qs(parsed.query)
            tag = qs.get('tag', ['unknown'])[0]
            filename = qs.get('f', [tag])[0]
        ts = time.strftime('%Y-%m-%dT%H:%M:%S%z')
        with open(SENSOR_LOG, 'a') as f:
            f.write(f"===== {ts} UPLOAD filename={filename} =====\n")
            f.write(body.decode('utf-8', 'replace'))
            f.write('\n')
        log(f"SENSOR LOG UPLOAD filename={filename} {len(body)} bytes")
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"ok\n")

    def do_POST_extra(self):
        # For /sensor_log POST (if bootstart.sh uses --post-file)
        parsed = urlparse(self.path)
        length = int(self.headers.get('Content-Length') or 0)
        body = self.rfile.read(length) if length else b''
        filename = 'unknown'
        if parsed.query:
            qs = parse_qs(parsed.query)
            filename = qs.get('f', ['unknown'])[0]
        ts = time.strftime('%Y-%m-%dT%H:%M:%S%z')
        with open(SENSOR_LOG, 'a') as f:
            f.write(f"===== {ts} UPLOAD filename={filename} =====\n")
            f.write(body.decode('utf-8', 'replace'))
            f.write('\n')
        log(f"SENSOR LOG UPLOAD filename={filename} {len(body)} bytes")
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"ok\n")

    def do_POST(self):
        if self.path.startswith('/acs'):
            self._do_cwmp_post()
        else:
            self.do_POST_extra()

    def _do_cwmp_post(self):
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
            cwmp_state["current_url"] = None
            if cwmp_state["cold_boot_expected"]:
                cwmp_state["cold_boot_expected"] = False
                log("cold-boot Inform received")
            need_trigger, reason = self._should_trigger(body_text)
            codes = _parse_event_codes(body_text)
            is_cold = any(c in ("0 BOOTSTRAP", "1 BOOT") or c.startswith("M ") for c in codes)
            if is_cold and cwmp_state["pending_command"] == "reboot":
                # Cold boot already satisfied the queued reboot; do not reboot again.
                cwmp_state["pending_command"] = "execute"
                cwmp_state["cold_boot_expected"] = True
                log("cold boot consumed pending reboot command")
            cwmp_state["target_url"] = BOOTSTART_URL if need_trigger else NONE_URL
            log(f"trigger_decision need={need_trigger} reason={reason} target={cwmp_state['target_url']}")
            if not need_trigger:
                cwmp_state["skip_count"] += 1
                _save_state()
                log("sensor appears healthy; setting LifemoteAgent to none if session continues")
            else:
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
                cwmp_state["reboot_count"] += 1
                _save_state()
                with open(LOG, "a") as f:
                    f.write("===== REBOOT REQUEST =====\n")
                    f.write(resp.decode("utf-8", "replace"))
                    f.write("\n")
            else:
                cwmp_state["step"] = STEP_GETPV_SENT
                cid = str(next_id())
                names = [f"{LIFEMOTE_BASE}.Enable", f"{LIFEMOTE_BASE}.URL"]
                log(f"sending GetParameterValues names={names}")
                resp = get_param_values(cid, names)
                self._write_request(resp)
                with open(LOG, "a") as f:
                    f.write("===== GETPV REQUEST =====\n")
                    f.write(resp.decode("utf-8", "replace"))
                    f.write("\n")

        elif tag == "GetParameterValuesResponse":
            cwmp_state["step"] = STEP_SETPV_SENT
            enable, url = _parse_enable_url(body_text)
            cwmp_state["current_enable"] = enable
            cwmp_state["current_url"] = url
            log(f"GetParameterValues Enable={enable} URL={url}")
            if enable == "1":
                cwmp_state["setpv_phase"] = 0
                _send_setpv(self, "0", cwmp_state["target_url"])
            else:
                cwmp_state["setpv_phase"] = 1
                _send_setpv(self, "1", cwmp_state["target_url"])

        elif tag == "SetParameterValuesResponse":
            if cwmp_state["setpv_phase"] == 0 and cwmp_state["current_enable"] == "1":
                cwmp_state["setpv_phase"] = 1
                cwmp_state["step"] = STEP_SETPV_SENT
                _send_setpv(self, "1", cwmp_state["target_url"])
            else:
                cwmp_state["step"] = STEP_DONE
                cwmp_state["trigger_count"] += 1
                _save_state()
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


def _send_setpv(handler, enable_val, url_val):
    cid = str(next_id())
    params = [
        (f"{LIFEMOTE_BASE}.Enable", enable_val, "xsd:boolean"),
        (f"{LIFEMOTE_BASE}.URL", url_val, "xsd:string"),
    ]
    log(f"sending SetParameterValues phase={cwmp_state['setpv_phase']} params={params}")
    resp = set_param_values(cid, params)
    handler._write_request(resp)
    with open(LOG, "a") as f:
        f.write(f"===== SETPV REQUEST phase={cwmp_state['setpv_phase']} =====\n")
        f.write(resp.decode("utf-8", "replace"))
        f.write("\n")

if __name__ == "__main__":
    _load_state()
    os.chdir(ROOT)
    open(LOG, "w").close()
    open(EVENT_LOG, "w").close()
    open(SENSOR_LOG, "w").close()
    log(f"EX520 Detectic ACS + package server on {HOST}:{PORT}")
    log(f"bootstart URL: {BOOTSTART_URL}")
    HTTPServer.allow_reuse_address = True
    server = HTTPServer((HOST, PORT), AcsHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
