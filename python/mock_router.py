#!/usr/bin/env python3
"""Mock TP-Link GDPR router for testing the Detectic Rust client without hardware.

Implements the minimal GDP API surface needed by the Rust `GtprClient`:

  1. POST /cgi/getGDPRParm  -> RSA modulus (nn), exponent (ee), seq
  2. POST /cgi_gdpr?9        -> login: verifies RSA sign, returns JSESSIONID
  3. GET  /                  -> HTML page with `var token="..."` (TokenID)
  4. POST /cgi_gdpr          -> gl/go operation: verifies RSA sign, returns
     the response AES-128-CBC encrypted with the session key/iv (base64),
     matching the real firmware behaviour documented in
     `ex520-network-map-gdpr.md`.

The mock generates its own RSA-1024 key pair. The client signs with the
router's *public* key (m^e mod n); the mock verifies with its private key
(sig^d mod n) and unpads PKCS#1 v1.5 to recover the signed message.

For login, the signed payload carries `&key=<k>&iv=<v>` (ASCII 16-byte key/iv).
The mock parses those out, stores them per session, and uses them to AES-encrypt
the `gl`/`go` operation responses so the client's `decode_response` decrypts
them correctly.

Usage:
    python3 python/mock_router.py
    # then:
    #   ./target/release/detectic --url http://127.0.0.1:18099 map
    #   DETECTIC_UPLOAD_URL=http://127.0.0.1:8081/api/v1/events ... ./target/release/detectic --url http://127.0.0.1:18099 sensor
"""

import base64
import itertools
import json
import re
import time
from http.server import HTTPServer, BaseHTTPRequestHandler, ThreadingHTTPServer

from cryptography.hazmat.backends import default_backend
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import padding as asym_padding
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives import padding as sym_padding

# ---------------------------------------------------------------------------
# RSA key (1024-bit, matching EX520 firmware)
# ---------------------------------------------------------------------------

RSA_PUBLIC_EXPONENT = 65537
RSA_KEY_SIZE = 1024

_rsa_key = rsa.generate_private_key(
    public_exponent=RSA_PUBLIC_EXPONENT, key_size=RSA_KEY_SIZE, backend=default_backend()
)
_rsa_pub = _rsa_key.public_key()
_rsa_n = _rsa_pub.public_numbers().n
_rsa_e = _rsa_pub.public_numbers().e
_rsa_d = _rsa_key.private_numbers().d

_RSA_N_BYTES = _rsa_n.bit_length() // 8
_RSA_N_HEX = format(_rsa_n, 'x').zfill(_RSA_N_BYTES * 2)
_RSA_E_HEX = format(_rsa_e, 'x').zfill(6)

_seq = itertools.count(1)

# session_id -> {"key": bytes, "iv": bytes}
_sessions = {}
_session_counter = itertools.count(1)


# ---------------------------------------------------------------------------
# Crypto helpers
# ---------------------------------------------------------------------------

def aes_encrypt(key: bytes, iv: bytes, plaintext: bytes) -> str:
    """AES-128-CBC encrypt with PKCS#7 padding, return base64."""
    padder = sym_padding.PKCS7(128).padder()
    pt = padder.update(plaintext) + padder.finalize()
    cipher = Cipher(algorithms.AES(key), modes.CBC(iv), backend=default_backend())
    ct = cipher.encryptor().update(pt) + cipher.encryptor().finalize()
    return base64.b64encode(ct).decode()


def aes_decrypt(key: bytes, iv: bytes, b64_data: str) -> bytes:
    """AES-128-CBC decrypt with PKCS#7 unpadding."""
    ct = base64.b64decode(b64_data)
    cipher = Cipher(algorithms.AES(key), modes.CBC(iv), backend=default_backend())
    pt = cipher.decryptor().update(ct) + cipher.decryptor().finalize()
    unpadder = sym_padding.PKCS7(128).unpadder()
    return unpadder.update(pt) + unpadder.finalize()


def recover_signed_message(sig_str: str) -> str:
    """Raw RSA decryption (m^d mod n) with zero-byte padding.

    The client pads each chunk to the modulus byte length with leading zeros,
    encrypts each chunk, and concatenates them. We split the signature into
    modulus-size blocks, decrypt each one, strip leading zeros, and concatenate.

    The `sign` transport encoding depends on the firmware dialect:
    GdprJson hex-encodes the raw signature bytes; GdprText base64-encodes
    them. Try hex first, then fall back to base64.
    """
    sig_str = sig_str.strip()
    try:
        if re.fullmatch(r'[0-9a-fA-F]+', sig_str) and len(sig_str) % 2 == 0:
            sig_bytes = bytes.fromhex(sig_str)
        else:
            raise ValueError("not hex")
    except Exception:
        sig_bytes = base64.b64decode(sig_str)

    k = _RSA_N_BYTES
    if len(sig_bytes) % k != 0:
        raise ValueError(f"signature length {len(sig_bytes)} is not a multiple of {k}")

    parts = []
    for i in range(0, len(sig_bytes), k):
        block = sig_bytes[i:i + k]
        sig_int = int.from_bytes(block, 'big')
        dec_int = pow(sig_int, _rsa_d, _rsa_n)
        dec_bytes = dec_int.to_bytes(k, 'big')
        # Remove leading zero padding and any trailing nulls the client added
        # to fill the modulus block; the actual message is ASCII key/value text.
        msg = dec_bytes.strip(b'\x00').lstrip(b'\x00')
        parts.append(msg.decode('ascii', errors='replace'))
    return ''.join(parts)


def parse_body(body: str):
    """Parse the `sign=<b64>\r\ndata=<b64>\r\n` envelope."""
    parts = body.split('\r\n')
    sign_part = parts[0]
    data_part = parts[1] if len(parts) > 1 else ""
    if not sign_part.startswith('sign='):
        return None, None
    sig_b64 = body[len('sign='):body.index('\r\ndata=')] if '\r\ndata=' in body else sign_part[5:]
    data_b64 = data_part[len('data='):] if data_part.startswith('data=') else None
    return sig_b64, data_b64


def extract_kv(inner_msg: str):
    """Extract key= and iv= from a signed login payload."""
    k = re.search(r'key=([^&]+)', inner_msg)
    iv = re.search(r'iv=([^&]+)', inner_msg)
    key = k.group(1).encode() if k else None
    ivv = iv.group(1).encode() if iv else None
    return key, ivv


# ---------------------------------------------------------------------------
# Mock network-map data (matches the live EX520 GTPR array response schema)
# ---------------------------------------------------------------------------

def _rfc3339_now(offset: int = 0) -> str:
    from datetime import datetime, timezone, timedelta
    tz = timezone(timedelta(hours=-3))
    return datetime.now(tz).replace(microsecond=0).isoformat()


def _assoc_dev() -> list:
    now = _rfc3339_now()
    return [
        {
            "X_TP_HostName": "phone",
            "X_TP_IPAddress": "192.168.0.20",
            "MACAddress": "AA:BB:CC:11:22:33",
            "X_TP_RadioMac": "00:11:22:33:44:55",
            "operatingStandard": "n",
            "signalStrength": "50",
            "active": "1",
            "associationTime": now,
            "lastDataDownlinkRate": "26000",
            "lastDataUplinkRate": "52000",
            "X_TP_SignalStrengthLevel": "4",
            "X_TP_MaxLinkRate": "72000",
            "noise": "50",
            "steeringHistoryNumberOfEntries": "0",
            "stack": "1,1,2,1,0,0",
        },
        {
            "X_TP_HostName": "laptop",
            "X_TP_IPAddress": "192.168.0.21",
            "MACAddress": "AA:BB:CC:44:55:66",
            "X_TP_RadioMac": "00:11:22:33:44:55",
            "operatingStandard": "ax",
            "signalStrength": "78",
            "active": "1",
            "associationTime": now,
            "lastDataDownlinkRate": "26000",
            "lastDataUplinkRate": "52000",
            "X_TP_SignalStrengthLevel": "4",
            "X_TP_MaxLinkRate": "72000",
            "noise": "50",
            "steeringHistoryNumberOfEntries": "0",
            "stack": "1,1,2,2,0,0",
        },
    ]


def _dhcp_clients() -> list:
    return [
        {
            "MACAddress": "aa:bb:cc:11:22:33",
            "IPAddress": "192.168.0.20",
            "hostname": "phone-dhcp",
        }
    ]


def _host_entries() -> list:
    return [
        {
            "hostName": "printer",
            "physAddress": "DD:EE:FF:00:00:01",
            "IPAddress": "192.168.0.50",
        }
    ]


def fake_map_json(oid: str) -> str:
    if oid == "DEV2_WIFI_APDEV_ASSOCDEV":
        data = _assoc_dev()
    elif oid == "DEV2_DHCPV4_CLIENT":
        data = _dhcp_clients()
    elif oid == "DEV2_HOST_ENTRY":
        data = _host_entries()
    else:
        data = []
    return json.dumps({
        "data": data,
        "operation": "gl",
        "oid": oid,
        "success": True,
    })


# ---------------------------------------------------------------------------
# HTTP handler
# ---------------------------------------------------------------------------

class MockGdprHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        pass

    # --- POST ---

    def do_POST(self):
        path = self.path

        # Step 1: getGDPRParm (EX520V returns JS assignments, not JSON)
        if path == '/cgi/getGDPRParm' or path == '/cgi/getGDPRParm/':
            seq = next(_seq)
            body = (
                f'var adminSetting=0;\n'
                f'var userSetting=2;\n'
                f'var logoUrl="";\n'
                f'var ee="{_RSA_E_HEX}";\n'
                f'var nn="{_RSA_N_HEX}";\n'
                f'var seq={seq};\n'
            )
            self._send(200, 'text/javascript', body)
            return

        # Step 2/4: /cgi_gdpr?* handles both login (sign carries key/iv)
        # and gl/go (sign is h=...&s=...). Dispatch by sign content.
        if path == '/cgi_gdpr' or path.startswith('/cgi_gdpr?'):
            length = int(self.headers.get('Content-Length', 0))
            body = self.rfile.read(length).decode('utf-8', errors='replace')
            sig_b64, _ = parse_body(body)
            if sig_b64 is None:
                self._send(400, 'text/plain', 'Bad cgi_gdpr body')
                return
            try:
                inner = recover_signed_message(sig_b64)
            except Exception as e:
                print(f"[mock] sign error: {e}", flush=True)
                self._send(401, 'text/plain', 'Invalid RSA signature')
                return
            if 'key=' in inner and '&iv=' in inner:
                print(f"[mock] dispatch -> login", flush=True)
                self._handle_login(body)
            else:
                print(f"[mock] dispatch -> operation", flush=True)
                self._handle_operation(body)
            return

        self._send(404, 'text/plain', 'Not Found')

    # --- GET ---

    def do_GET(self):
        path = self.path
        if path == '/cgi/getGDPRParm' or path == '/cgi/getGDPRParm/':
            seq = next(_seq)
            body = (
                f'var adminSetting=0;\n'
                f'var userSetting=2;\n'
                f'var logoUrl="";\n'
                f'var ee="{_RSA_E_HEX}";\n'
                f'var nn="{_RSA_N_HEX}";\n'
                f'var seq={seq};\n'
            )
            self._send(200, 'text/javascript', body)
            return
        # Step 3: token page
        if path == '/' or path == '':
            token = base64.urlsafe_b64encode(f"token-{int(time.time())}".encode()).decode()[:32]
            html = f'<html><script>var token="{token}";</script></html>'
            self._send(200, 'text/html', html)
            return
        self._send(404, 'text/plain', 'Not Found')

    # --- handlers ---

    def _handle_login(self, body):
        sig_b64, _ = parse_body(body)
        if sig_b64 is None:
            self._send(400, 'text/plain', 'Bad login body')
            return
        try:
            inner = recover_signed_message(sig_b64)
        except Exception as e:
            print(f"[mock] sign error: {e}", flush=True)
            self._send(401, 'text/plain', 'Invalid RSA signature')
            return
        if not ('&s=' in inner and 'h=' in inner):
            self._send(401, 'text/plain', 'Invalid sign message')
            return
        key, ivv = extract_kv(inner)
        if not key or not ivv or len(key) != 16 or len(ivv) != 16:
            self._send(400, 'text/plain', 'Missing/invalid key/iv in login')
            return

        sid = f"sid-{next(_session_counter)}"
        _sessions[sid] = {"key": key, "iv": ivv}
        # The Rust client parses `JSESSIONID=<value>` out of the Set-Cookie
        # header, then echoes it back as `Cookie: JSESSIONID=<value>` on later
        # requests. The session key/iv (recovered from the login sign) are
        # reused to AES-encrypt every `gl`/`go` response.
        resp = json.dumps({"sessionKey": "deadbeef", "sessionIv": "feedface"})
        self._send(200, 'text/plain', resp, extra_headers={'Set-Cookie': f'JSESSIONID={sid}; Path=/'})

    def _handle_operation(self, body):
        sid = self._cookie_session()
        print(f"[mock] op sid={sid!r} cookie={self.headers.get('Cookie', '')!r}", flush=True)
        sess = _sessions.get(sid)
        if not sess:
            self._send(401, 'text/plain', 'No valid session (login first)')
            return

        sig_b64, data_b64 = parse_body(body)
        if sig_b64 is None:
            self._send(400, 'text/plain', 'Bad operation body')
            return
        try:
            inner = recover_signed_message(sig_b64)
        except Exception as e:
            print(f"[mock] gl sign error: {e}", flush=True)
            self._send(401, 'text/plain', 'Invalid RSA signature')
            return
        print(f"[mock] gl inner={inner[:80]!r}", flush=True)
        if not ('&s=' in inner and 'h=' in inner):
            self._send(401, 'text/plain', 'Invalid sign message')
            return

        # Determine the requested OID by decrypting the payload.
        oid = "DEV2_WIFI_APDEV_ASSOCDEV"
        if data_b64:
            try:
                plaintext = aes_decrypt(sess["key"], sess["iv"], data_b64)
                req = json.loads(plaintext)
                oid = req.get("oid", oid)
            except Exception:
                pass

        # AES-encrypt the JSON response with the session key/iv and base64 it,
        # exactly as the real firmware does (see ex520-network-map-gdpr.md §Responses).
        encrypted_b64 = aes_encrypt(sess["key"], sess["iv"], fake_map_json(oid).encode())
        print(f"[mock] sending 200 oid={oid} ct_len={len(encrypted_b64)}", flush=True)
        self._send(200, 'text/plain', encrypted_b64)

    # --- helpers ---

    def _cookie_session(self):
        cookie = self.headers.get('Cookie', '')
        for part in cookie.split(';'):
            part = part.strip()
            if part.startswith('JSESSIONID='):
                return part[len('JSESSIONID='):]
        return None

    def _send(self, code, ctype, body, extra_headers=None):
        self.send_response(code)
        self.send_header('Content-Type', ctype)
        if extra_headers:
            for k, v in extra_headers.items():
                self.send_header(k, v)
        self.send_header('Content-Length', str(len(body.encode())))
        self.end_headers()
        self.wfile.write(body.encode())


def start_mock_server(port=18099):
    server = ThreadingHTTPServer(('127.0.0.1', port), MockGdprHandler)
    import threading
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Mock TP-Link GDPR router")
    parser.add_argument("--port", type=int, default=18099)
    args = parser.parse_args()
    server = start_mock_server(args.port)
    print(f"[*] Mock GDPR router running on http://127.0.0.1:{args.port}")
    print(f"    RSA 1024-bit key (nn/ee) via POST /cgi/getGDPRParm (JS format)")
    print(f"    Login via POST /cgi_gdpr?9 (responses AES-128-CBC encrypted)")
    print("    (CTRL+C to stop)")
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        print("\n[*] Shutting down")
        server.shutdown()


if __name__ == "__main__":
    main()
