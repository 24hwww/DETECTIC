#!/usr/bin/env python3
"""Detectic GTPR/GDPR client (Python reference implementation).

Mirrors the Rust `detectic` crate and ports the verified protocol from
`@hertzg/tplink-api` + the `0xf15h/tp_link_gdpr` capture analysis:

  1. POST /cgi/getGDPRParm        -> RSA modulus/exponent (nn/ee) + seq
  2. POST /cgi_gdpr?9             -> login (AES-128-CBC of creds with a
                                     client-generated key/iv; RSA "sign" of
                                     h=<md5>&s=<seq+len>&key=<key>&iv=<iv>)
  3. GET  /                       -> TokenID from HTML
  4. gl/go operations             -> AES-encrypted + RSA-signed, reusing key/iv

Only the standard library + `requests` + `cryptography` are required.

Usage:
    export DETECTIC_PASSWORD="tu-password"
    python3 detectic_client.py map --url http://192.168.0.1 --user admin
    python3 detectic_client.py capture --url http://192.168.0.1 --db detectic.db
"""

import argparse
import base64
import hashlib
import json
import os
import re
import secrets
import sqlite3
import struct
import sys
import time
from urllib.parse import urljoin

import requests
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes


# --------------------------------------------------------------------------
# Crypto primitives
# --------------------------------------------------------------------------

def md5_hex(s: str) -> str:
    return hashlib.md5(s.encode()).hexdigest()


def auth_hash(user: str, password: str) -> str:
    return md5_hex(user + password)


def gen_login_aes_pair():
    """16-byte ASCII key/iv: Unix ms timestamp (13) + 3 random digits."""
    import random
    ms = int(time.time() * 1000)
    r1 = random.randint(0, 999)
    r2 = random.randint(0, 999)
    key = f"{ms}{r1:03d}".encode()[:16]
    iv = f"{ms}{r2:03d}".encode()[:16]
    return key, iv


def aes_cbc(key: bytes, iv: bytes, data: bytes, decrypt: bool = False) -> bytes:
    cipher = Cipher(algorithms.AES(key), modes.CBC(iv))
    if decrypt:
        proc = cipher.decryptor()
        pt = proc.update(data) + proc.finalize()
        pad = pt[-1]
        return pt[:-pad]
    pad = 16 - (len(data) % 16)
    data = data + bytes([pad]) * pad
    proc = cipher.encryptor()
    return proc.update(data) + proc.finalize()


def rsa_sign_public(n: int, e: int, msg: bytes) -> bytes:
    """TP-Link RSA-512 signature: raw chunks padded with trailing zeros.

    The EX520V firmware uses a 64-byte RSA modulus and "nopadding" mode:
    the message is split into <=k-byte chunks, each chunk is zero-padded
    to exactly k bytes, and each block is encrypted as m^e mod n.
    For messages longer than k (e.g. login with key/iv), this produces
    multiple k-byte signature blocks concatenated together.
    """
    k = (n.bit_length() + 7) // 8
    sig = b""
    for i in range(0, len(msg), k):
        chunk = msg[i:i + k]
        block = chunk + b"\x00" * (k - len(chunk))
        m = int.from_bytes(block, "big")
        c = pow(m, e, n)
        sig += c.to_bytes(k, "big")
    return sig


def build_sign(n: int, e: int, auth_h: str, seq: int, data_b64_len: int,
               key_iv=None, encoding: str = "hex") -> str:
    # Match tpEncrypt.js: for login the aesKeyString ("key=...&iv=...") is the
    # prefix of the signed message; non-login uses h=...&s=...
    if key_iv:
        payload = (f"key={key_iv[0].decode()}&iv={key_iv[1].decode()}"
                   f"&h={auth_h}&s={seq + data_b64_len}")
    else:
        payload = f"h={auth_h}&s={seq + data_b64_len}"
    sig = rsa_sign_public(n, e, payload.encode())
    if encoding == "hex":
        return sig.hex()
    return base64.b64encode(sig).decode()


def build_body(sign: str, data_b64: str) -> str:
    return f"sign={sign}\r\ndata={data_b64}\r\n"


def decode_response(key: bytes, iv: bytes, base64_chunks: str) -> str:
    compact = "".join(c for c in base64_chunks if not c.isspace())
    raw = base64.b64decode(compact)
    pt = aes_cbc(key, iv, raw, decrypt=True)
    return pt.decode("utf-8", "replace")


def parse_gdpr_parm(text: str) -> dict:
    """Parse the /cgi/getGDPRParm response.

    Some firmwares return plain JSON, e.g.:
        {"nn":"...","ee":"010001","seq":123}

    The EX520V firmware (and this extracted rootfs) returns a JavaScript
    snippet that must be eval'd, e.g.:
        var adminSetting=0; var userSetting=2; var logoUrl="";
        var ee="010001"; var nn="..."; var seq=123;

    This helper tries JSON first, then falls back to regex extraction.
    """
    text = text.strip()
    if text.startswith("{"):
        try:
            return json.loads(text)
        except ValueError:
            pass
    # JavaScript var assignments
    parm = {}
    for key in ("nn", "ee", "seq"):
        m = re.search(
            r"var\s+" + key + r"\s*=\s*(?:\"([^\"]*)\"|'([^']*)'|([^;]+))\s*;",
            text,
        )
        if not m:
            m = re.search(
                r"\b" + key + r"\s*=\s*(?:\"([^\"]*)\"|'([^']*)'|([^;]+))\s*;",
                text,
            )
        if m:
            value = m.group(1) if m.group(1) is not None else (
                m.group(2) if m.group(2) is not None else m.group(3)
            )
            parm[key] = value.strip()
    if not all(k in parm for k in ("nn", "ee", "seq")):
        raise ValueError(f"could not parse getGDPRParm response: {text[:200]!r}")
    return parm


# --------------------------------------------------------------------------
# GTPR client
# --------------------------------------------------------------------------

class Dialect:
    GDPR_JSON = "gdpr-json"
    GDPR_TEXT = "gdpr-text"


def login_payload(dialect: str, user: str, password: str) -> str:
    if dialect == Dialect.GDPR_JSON:
        u = base64.b64encode(user.encode()).decode()
        p = base64.b64encode(password.encode()).decode()
        return (f'{{"data":{{"UserName":"{u}","Passwd":"{p}","Action":"1",'
                f'"stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}},'
                f'"operation":"cgi","oid":"/cgi/login"}}')
    return f"{user}\n{password}"


def sign_encoding_for(dialect: str) -> str:
    return "hex" if dialect == Dialect.GDPR_JSON else "base64"


class GtprClient:
    def __init__(self, base_url, user, password, dialect=Dialect.GDPR_JSON):
        self.base = base_url.rstrip("/")
        self.user = user
        self.password = password
        self.dialect = dialect
        self.session = requests.Session()
        self.rsa_n = 0
        self.rsa_e = 0
        self.seq = 0
        self.key = b""
        self.iv = b""
        self.jsessionid = ""
        self.token = ""

    def connect(self):
        resp = self.session.post(
            urljoin(self.base + "/", "cgi/getGDPRParm"),
            headers={
                "Referer": self.base + "/",
                "Origin": self.base,
                "Accept": "*/*",
            },
        )
        if not resp.text:
            raise RuntimeError(
                f"getGDPRParm returned empty body (status={resp.status_code}, "
                f"headers={dict(resp.headers)})"
            )
        try:
            parm = parse_gdpr_parm(resp.text)
        except ValueError as e:
            raise RuntimeError(
                f"getGDPRParm did not return a recognized JSON/JS format. "
                f"status={resp.status_code}, body={resp.text[:500]!r}"
            ) from e
        self.rsa_n = int(parm["nn"], 16)
        self.rsa_e = int(parm["ee"], 16)
        self.seq = int(parm["seq"])
        self.login()
        self.fetch_token()

    def login(self):
        self.key, self.iv = gen_login_aes_pair()
        auth_h = auth_hash(self.user, self.password)
        payload = login_payload(self.dialect, self.user, self.password)
        ct = aes_cbc(self.key, self.iv, payload.encode())
        data_b64 = base64.b64encode(ct).decode()
        enc = sign_encoding_for(self.dialect)
        sign = build_sign(self.rsa_n, self.rsa_e, auth_h, self.seq,
                          len(data_b64), (self.key, self.iv), enc)
        body = build_body(sign, data_b64)
        resp = self.session.post(
            urljoin(self.base + "/", "cgi_gdpr?9"),
            data=body,
            headers={"Content-Type": "text/plain",
                     "Referer": self.base + "/",
                     "Origin": self.base,
                     "Accept": "*/*",
                     "X-Requested-With": "XMLHttpRequest"},
        )
        sc = resp.headers.get("set-cookie", "")
        has_cookie = "JSESSIONID=" in sc
        try:
            plain = decode_response(self.key, self.iv, resp.text) if resp.text else ""
        except Exception as e:
            plain = f"<decrypt error: {e}>"
        print(f"[DEBUG login] status={resp.status_code} jsessionid-present={has_cookie} body_len={len(resp.text)} plain={plain[:200]!r}")
        if "JSESSIONID=" in sc:
            self.jsessionid = sc.split("JSESSIONID=", 1)[1].split(";", 1)[0]
        if not self.jsessionid:
            raise RuntimeError("login refused: no JSESSIONID in response")
        # Some firmware returns session key/iv in the body; prefer if present.
        try:
            lr = resp.json()
            kk = lr.get("key") or lr.get("sessionKey")
            ivv = lr.get("iv") or lr.get("sessionIv")
            if kk and ivv:
                kb, ivb = bytes.fromhex(kk), bytes.fromhex(ivv)
                if len(kb) == 16 and len(ivb) == 16:
                    self.key, self.iv = kb, ivb
        except Exception:
            pass

    def fetch_token(self):
        html = self.session.get(
            self.base,
            headers={"Cookie": f"JSESSIONID={self.jsessionid}",
                     "Referer": self.base + "/",
                     "Origin": self.base,
                     "Accept": "*/*"},
        ).text
        i = html.find('var token="')
        if i == -1:
            # If the router does not publish a token, generate a client-side
            # token matching the observed 32-hex format.
            self.token = secrets.token_hex(16)
        else:
            self.token = html[i + len('var token="'):].split('"', 1)[0]

    def gl(self, oid: str) -> str:
        raw = (f'{{"data":{{"stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}},'
               f'"operation":"gl","oid":"{oid}"}}\r\n')
        return self._operation(raw)

    def _operation(self, raw: str) -> str:
        ct = aes_cbc(self.key, self.iv, raw.encode())
        data_b64 = base64.b64encode(ct).decode()
        auth_h = auth_hash(self.user, self.password)
        enc = sign_encoding_for(self.dialect)
        sign = build_sign(self.rsa_n, self.rsa_e, auth_h, self.seq,
                          len(data_b64), None, enc)
        body = build_body(sign, data_b64)
        print(f"[DEBUG gl] token_len={len(self.token)} jsessionid_len={len(self.jsessionid)} sign_len={len(sign)} data_len={len(data_b64)}")
        resp = self.session.post(
            urljoin(self.base + "/", "cgi_gdpr?9"),
            data=body,
            headers={"Content-Type": "text/plain",
                     "TokenID": self.token,
                     "Cookie": f"JSESSIONID={self.jsessionid}",
                     "Referer": self.base + "/",
                     "Origin": self.base,
                     "Accept": "*/*"},
        )
        print(f"[DEBUG gl] status={resp.status_code} body_len={len(resp.text)} body={resp.text[:200]!r}")
        return decode_response(self.key, self.iv, resp.text)

    def network_map(self):
        oid = "DEV2_WIFI_APDEV_ASSOCDEV"
        js = self.gl(oid)
        data = json.loads(js).get("data", {})
        devs = data.get("ASSOCDEV", []) if isinstance(data, dict) else []
        return {"captured_at": int(time.time()), "devices": devs, "raw": {oid: js}}


# --------------------------------------------------------------------------
# Persistence (mirrors the Rust store)
# --------------------------------------------------------------------------

def hmac_pseudonym(secret: bytes, ident: str) -> str:
    import hmac, hashlib
    return hmac.new(secret, ident.encode(), hashlib.sha256).hexdigest()


def open_store(path, secret: bytes):
    conn = sqlite3.connect(path)
    conn.executescript(
        """
        CREATE TABLE IF NOT EXISTS snapshots (
            id INTEGER PRIMARY KEY, captured_at INTEGER NOT NULL, raw_json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS devices (
            id INTEGER PRIMARY KEY, snapshot_id INTEGER NOT NULL, device_key TEXT,
            pseudonym TEXT, hostname TEXT, ip TEXT, mac TEXT, rssi INTEGER,
            standard TEXT, onemesh_stack TEXT, assoc_time INTEGER, radio_mac TEXT,
            FOREIGN KEY(snapshot_id) REFERENCES snapshots(id));
        CREATE INDEX IF NOT EXISTS idx_dev_key ON devices(device_key);
        CREATE INDEX IF NOT EXISTS idx_dev_pseudo ON devices(pseudonym);
        """
    )
    return conn


def save_snapshot(conn, secret, netmap):
    cur = conn.cursor()
    cur.execute("INSERT INTO snapshots (captured_at, raw_json) VALUES (?,?)",
                (netmap["captured_at"], json.dumps(netmap)))
    sid = cur.lastrowid
    for d in netmap["devices"]:
        ident = d.get("mac") or d.get("ip") or d.get("hostname") or ""
        pseudo = hmac_pseudonym(secret, ident if ident else json.dumps(d))
        cur.execute(
            "INSERT INTO devices (snapshot_id, device_key, pseudonym, hostname, ip, mac, "
            "rssi, standard, onemesh_stack, assoc_time, radio_mac) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            (sid, ident, pseudo, d.get("hostname"), d.get("ip"), d.get("MACAddress") or d.get("mac"),
             d.get("signalStrength") or d.get("rssi"),
             d.get("opStandard") or d.get("standard"),
             d.get("stack") or d.get("onemesh_stack"),
             d.get("assocTime") or d.get("assoc_time"),
             d.get("radioMAC") or d.get("radio_mac"))),
    conn.commit()
    return sid


def latest_before(conn, before=None):
    if before:
        row = conn.execute(
            "SELECT raw_json FROM snapshots WHERE id < ? ORDER BY id DESC LIMIT 1",
            (before,)).fetchone()
    else:
        row = conn.execute(
            "SELECT raw_json FROM snapshots ORDER BY id DESC LIMIT 1").fetchone()
    return json.loads(row[0]) if row else None


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(description="Detectic GTPR client")
    ap.add_argument("--url",
                    default=os.environ.get("DETECTIC_URL", "http://192.168.0.1"))
    ap.add_argument("--user",
                    default=os.environ.get("DETECTIC_USER", "user"))
    ap.add_argument("--password",
                    default=os.environ.get("DETECTIC_PASSWORD"))
    ap.add_argument("--dialect", choices=[Dialect.GDPR_JSON, Dialect.GDPR_TEXT],
                    default=Dialect.GDPR_JSON)
    ap.add_argument("--db", default="detectic.db")
    ap.add_argument("--secret",
                    default=os.environ.get("DETECTIC_SECRET"))
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("map")
    sub.add_parser("capture")
    sub.add_parser("stats")
    sub.add_parser("report")
    args = ap.parse_args()

    if not args.password:
        ap.error("--password or DETECTIC_PASSWORD environment variable is required")

    secret = bytes.fromhex(args.secret)

    if args.cmd == "map":
        c = GtprClient(args.url, args.user, args.password, args.dialect)
        c.connect()
        print(json.dumps(c.network_map(), indent=2))
        return

    conn = open_store(args.db, secret)
    if args.cmd == "stats":
        n = conn.execute(
            "SELECT COUNT(DISTINCT pseudonym) FROM devices").fetchone()[0]
        snaps = conn.execute("SELECT COUNT(*) FROM snapshots").fetchone()[0]
        print("snapshots stored:", snaps)
        print("distinct devices ever seen:", n)
        return

    if args.cmd == "report":
        rows = conn.execute(
            """SELECT d.pseudonym, d.hostname, d.mac, d.source,
                      MIN(s.captured_at) AS first_seen,
                      MAX(s.captured_at) AS last_seen,
                      COUNT(*)           AS observations,
                      CAST(ROUND(AVG(d.rssi)) AS INTEGER) AS avg_rssi,
                      MIN(d.rssi)        AS min_rssi,
                      MAX(d.rssi)        AS max_rssi
               FROM devices d JOIN snapshots s ON d.snapshot_id = s.id
               GROUP BY d.pseudonym ORDER BY last_seen DESC"""
        ).fetchall()
        if not rows:
            print("no data yet")
            return
        print(f"{'pseudonym':<12} {'first':>10} {'last':>10} {'obs':>6} "
              f"{'avg_rssi':>8} {'min':>7} {'max':>7} src")
        for p, h, m, src, f, l, obs, avg, mn, mx in rows:
            print(f"{p[:12]:<12} {f:>10} {l:>10} {obs:>6} "
                  f"{str(avg):>8} {str(mn):>7} {str(mx):>7} {src or '-'}")
        return

    # capture
    c = GtprClient(args.url, args.user, args.password, args.dialect)
    c.connect()
    netmap = c.network_map()
    prev = latest_before(conn)
    diff = {"added": [], "removed": [], "changed": []}
    if prev:
        pmap = {d.get("mac") or d.get("ip") or d.get("hostname"): d
                for d in prev["devices"]}
        cmap = {d.get("mac") or d.get("ip") or d.get("hostname"): d
                for d in netmap["devices"]}
        for k, d in cmap.items():
            if k not in pmap:
                diff["added"].append(d)
            elif pmap[k] != d:
                diff["changed"].append((pmap[k], d))
        for k, d in pmap.items():
            if k not in cmap:
                diff["removed"].append(d)
    sid = save_snapshot(conn, secret, netmap)
    print(f"snapshot {sid} | devices: {len(netmap['devices'])}")
    print(f"+ added: {len(diff['added'])}  - removed: {len(diff['removed'])}  "
          f"~ changed: {len(diff['changed'])}")
    for d in diff["added"]:
        print("  +", d.get("hostname"), d.get("ip"), d.get("mac"))
    for d in diff["removed"]:
        print("  -", d.get("hostname"), d.get("ip"), d.get("mac"))


if __name__ == "__main__":
    main()
