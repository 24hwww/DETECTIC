#!/usr/bin/env python3
"""Map every API endpoint / data-model OID exposed by the live EX520.

Read-only: only `gl` (get-list) operations and unauthenticated GET probes.
Nothing is written to the router.

Usage:
  python3 python/api_mapper.py --url http://192.168.0.1 --password '<REDACTED>'
"""
import argparse
import base64
import hashlib
import html
import json
import random
import re
import sys
import time
from pathlib import Path

import requests
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

ROOT = Path(__file__).resolve().parent.parent
OID_JS = ROOT / "_rootfs/web/js/oid_str.js"
OUT = ROOT / "API_MAP.md"

# ---------------------------------------------------------------------------
# Crypto primitives (identical to detectic_client.py / src/crypto.rs)
# ---------------------------------------------------------------------------

def md5_hex(s: str) -> str:
    return hashlib.md5(s.encode()).hexdigest()


def gen_login_aes_pair():
    ms = int(time.time() * 1000)
    r1, r2 = random.randint(0, 999), random.randint(0, 999)
    return f"{ms}{r1:03d}".encode()[:16], f"{ms}{r2:03d}".encode()[:16]


def aes_cbc(key, iv, data, decrypt=False):
    cipher = Cipher(algorithms.AES(key), modes.CBC(iv))
    if decrypt:
        d = cipher.decryptor()
        pt = d.update(data) + d.finalize()
        pad = pt[-1]
        return pt[:-pad] if 0 < pad <= 16 else pt
    pad = 16 - (len(data) % 16)
    proc = cipher.encryptor()
    return proc.update(data + bytes([pad]) * pad) + proc.finalize()


def rsa_sign_public(n: int, e: int, msg: bytes) -> bytes:
    k = (n.bit_length() + 7) // 8
    ps_len = k - 3 - len(msg)
    assert ps_len >= 8
    block = b"\x00\x01" + b"\xff" * ps_len + b"\x00" + msg
    sig = pow(int.from_bytes(block, "big"), e, n)
    return sig.to_bytes(k, "big")


def build_sign(n, e, auth_h, seq, data_len, key_iv=None):
    payload = f"h={auth_h}&s={seq + data_len}"
    if key_iv:
        payload += f"&key={key_iv[0].decode()}&iv={key_iv[1].decode()}"
    sig = rsa_sign_public(n, e, payload.encode())
    return base64.b64encode(sig).decode()  # EX520 uses base64 transport


# ---------------------------------------------------------------------------
# GDPR session — tolerant of JSON and JS-variable getGDPRParm responses
# ---------------------------------------------------------------------------

class GdprSession:
    def __init__(self, base, user, password):
        self.base = base.rstrip("/")
        self.user, self.password = user, password
        self.s = requests.Session()
        self.s.headers.update({"Referer": self.base + "/",
                               "X-Requested-With": "XMLHttpRequest"})
        self.n = self.e = 0
        self.seq = 0
        self.key = self.iv = b""
        self.jsid = ""
        self.token = ""

    def connect(self):
        r = self.s.post(f"{self.base}/cgi/getGDPRParm", timeout=10)
        txt = r.text
        try:                                   # JSON firmware style
            p = json.loads(txt)
            nn, ee, seq = p["nn"], p["ee"], p["seq"]
        except ValueError:                     # JS-variable style (EX520 live)
            def var(name):
                m = re.search(rf'var {name}\s*=\s*"([^"]+)"', txt)
                return m.group(1) if m else ""
            nn, ee, seq = var("nn"), var("ee"), var("seq")
        if not nn:
            raise RuntimeError(f"getGDPRParm unusable: {txt[:200]!r}")
        self.n, self.e = int(nn, 16), int(ee, 16)
        self.seq = int(seq)

        self._login()
        self._token()

    def _login(self):
        self.key, self.iv = gen_login_aes_pair()
        auth_h = md5_hex(self.user + self.password)
        results = []
        for dialect, payload in (
            ("text", f"{self.user}\n{self.password}"),
            ("json", json.dumps({
                "data": {"UserName": base64.b64encode(self.user.encode()).decode(),
                         "Passwd": base64.b64encode(self.password.encode()).decode(),
                         "Action": "1", "stack": "0,0,0,0,0,0", "pstack": "0,0,0,0,0,0"},
                "operation": "cgi", "oid": "/cgi/login"})),
        ):
            ct = aes_cbc(self.key, self.iv, payload.encode())
            data_b64 = base64.b64encode(ct).decode()
            sign = build_sign(self.n, self.e, auth_h, self.seq,
                              len(data_b64), (self.key, self.iv))
            body = f"sign={sign}\r\ndata={data_b64}\r\n"
            r = self.s.post(f"{self.base}/cgi_gdpr?9", data=body,
                            headers={"Content-Type": "text/plain"}, timeout=10)
            sc = r.headers.get("set-cookie", "")
            m = re.search(r"JSESSIONID=([^;]+)", sc)
            results.append((dialect, r.status_code, bool(m)))
            if m:
                self.jsid = m.group(1)
                self.login_dialect = dialect
                break
        if not self.jsid:
            raise RuntimeError(f"login refused: tried {results}")

    def _token(self):
        r = self.s.get(self.base, timeout=10)
        m = re.search(r'var token="([^"]+)"', r.text) or \
            re.search(r"token=([\w]+)", r.text)
        if not m:
            raise RuntimeError(f"no TokenID; page head: {r.text[:300]!r}")
        self.token = m.group(1)

    # -- operations ---------------------------------------------------------
    def gl(self, oid: str):
        raw = (json.dumps({"data": {"stack": "0,0,0,0,0,0",
                                    "pstack": "0,0,0,0,0,0"},
                           "operation": "gl", "oid": oid}) + "\r\n")
        ct = aes_cbc(self.key, self.iv, raw.encode())
        data_b64 = base64.b64encode(ct).decode()
        sign = build_sign(self.n, self.e, md5_hex(self.user + self.password),
                          self.seq, len(data_b64))
        body = f"sign={sign}\r\ndata={data_b64}\r\n"
        r = self.s.post(f"{self.base}/cgi_gdpr", data=body, timeout=15,
                        headers={"Content-Type": "text/plain",
                                 "TokenID": self.token,
                                 "Cookie": f"JSESSIONID={self.jsid}"})
        compact = "".join(c for c in r.text if not c.isspace())
        try:
            pt = aes_cbc(self.key, self.iv, base64.b64decode(compact), decrypt=True)
        except Exception as exc:
            return {"error": f"decrypt-fail ({exc})", "http": r.status_code,
                    "raw": r.text[:120]}
        try:
            return {"ok": True, "json": json.loads(pt.decode("utf-8", "replace"))}
        except ValueError:
            return {"ok": True, "text": pt.decode("utf-8", "replace")[:400]}


# ---------------------------------------------------------------------------
# OID enumeration from the firmware's own JS registry
# ---------------------------------------------------------------------------

def load_oids() -> list[str]:
    names = set()
    for m in re.finditer(r'\b(DEV2[A-Z0-9_]*)\b', OID_JS.read_text(errors="replace")):
        names.add(m.group(1))
    # keep plausible object ids (exclude pure flag names like INCLUDE_*)
    oids = sorted(n for n in names if n.startswith("DEV2"))
    return oids


CGI_PROBES = [
    "/cgi/getGDPRParm", "/cgi_gdpr?9", "/cgi_gdpr", "/cgi/login", "/cgi/conf.bin",
    "/", "/web/main/login.htm", "/web/main/backNRestore.htm",
]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://192.168.0.1")
    ap.add_argument("--user", default="user")
    ap.add_argument("--password", required=True)
    ap.add_argument("--only", help="comma-separated OIDs to probe (default all)")
    ap.add_argument("--out", default=str(OUT))
    args = ap.parse_args()

    sess = GdprSession(args.url, args.user, args.password)
    print(f"[*] connecting to {args.url} …")
    sess.connect()
    print(f"[+] login OK (dialect={sess.login_dialect}, "
          f"RSA-{(sess.n.bit_length()+7)//8*8}, seq={sess.seq}, token={sess.token[:12]}…)")

    oids = load_oids()
    if args.only:
        want = set(args.only.split(","))
        oids = [o for o in oids if o in want]
    print(f"[*] probing {len(oids)} OIDs with gl …")

    results = {}
    for i, oid in enumerate(oids, 1):
        res = sess.gl(oid)
        results[oid] = res
        status = summarize(res)
        print(f"  [{i:>3}/{len(oids)}] {status:<28} {oid}")
        time.sleep(0.05)

    write_report(sess, oids, results, args)


def summarize(res):
    if res.get("error"):
        return res["error"][:26]
    j = res.get("json")
    if isinstance(j, dict):
        err = j.get("errorcode")
        if err not in (None, 0):
            return f"errorcode {err}"
        data = j.get("data")
        if isinstance(data, dict) and data:
            keys = ",".join(list(data)[:2])[:24]
            return f"OK data[{keys}]"
        if isinstance(data, list):
            return f"OK list({len(data)})" if data else "OK empty"
        return "OK"
    return "text"


def shape_of(j):
    """Describe the response shape without dumping values (privacy)."""
    if not isinstance(j, dict):
        return type(j).__name__
    out = {}
    data = j.get("data", j)
    if isinstance(data, dict):
        for k, v in list(data.items())[:12]:
            if isinstance(v, list):
                inner = shape_of(v[0]) if v and isinstance(v[0], dict) else \
                    (type(v[0]).__name__ if v else "empty")
                out[k] = [inner]
            elif isinstance(v, dict):
                out[k] = shape_of(v)
            else:
                out[k] = type(v).__name__
    elif isinstance(data, list):
        out = [shape_of(data[0])] if data and isinstance(data[0], dict) else []
    return out


def write_report(sess, oids, results, args):
    ok = [o for o, r in results.items() if r.get("ok") and summarize(r) != "OK"]
    errs = [(o, summarize(r)) for o, r in results.items() if summarize(r).startswith("errorcode")]
    lines = [
        "# EX520 Live API Map",
        "",
        f"> Generated by `python/api_mapper.py` against `{args.url}` "
        f"on {time.strftime('%Y-%m-%d %H:%M')}. Read-only (`gl` operations only).",
        f"> Login dialect used: **{getattr(sess, 'login_dialect', '?')}**; "
        f"sign transport: base64; RSA-{(sess.n.bit_length()+7)//8*8}.",
        "",
        "## Endpoints",
        "| Endpoint | Method | Purpose |",
        "|----------|--------|---------|",
        "| `/cgi/getGDPRParm` | POST | RSA `nn`/`ee` + `seq`. Response is **JS variables** (`var nn=\"…\"`), not JSON |",
        "| `/cgi_gdpr?9` | POST | Login (AES-CBC body + RSA sign incl. `key`/`iv`) → `JSESSIONID` cookie |",
        "| `/` | GET | HTML with `var token=\"…\"` (TokenID header) |",
        "| `/cgi_gdpr` | POST | Encrypted `gl`/`go` operations (header `TokenID`, cookie `JSESSIONID`) |",
        "",
        "## OID probe results",
        f"- Probed: **{len(results)}** OIDs from `_rootfs/web/js/oid_str.js`",
        f"- Responded with data: **{len(ok)}**",
        f"- Data-model errors: **{len(errs)}**",
        "",
        "### Working OIDs (data returned)",
        "",
        "| OID | Shape (types only, no values) |",
        "|-----|-------------------------------|",
    ]
    for o in sorted(ok):
        lines.append(f"| `{o}` | `{json.dumps(shape_of(results[o]['json']))[:180]}` |")
    lines += ["", "### OIDs rejected by the data model", "",
              "| OID | errorcode |", "|-----|-----------|"]
    for o, e in sorted(errs):
        lines.append(f"| `{o}` | {e.split()[-1]} |")
    Path(args.out).write_text("\n".join(lines))
    print(f"[+] wrote {args.out}")


if __name__ == "__main__":
    main()
