#!/usr/bin/env python3
"""Analyze a PCAP of a TP-Link GDPR session and dump the decrypted handshake.

This is the diagnostic counterpart to `detectic_client.py`: give it a capture of
a real login + map pull (e.g. `tshark -i eth0 -w ex520.pcap` while you open the
router web UI, or run `detectic_client.py map`), and it will:

  1. recover the login AES key/IV via the timestamp brute-force (see crack_login),
  2. decrypt the login request and every subsequent `cgi_gdpr` request/response,
  3. print a structured summary plus the raw JSON it observed.

This is how you pin down the exact dialect (gdpr-json vs gdpr-text, hex vs base64
sign) for a given EX520 firmware without guessing.

Usage:
  python3 analyze_pcap.py --pcap ex520.pcap --epoch 1717180000
  python3 analyze_pcap.py --pcap ex520.pcap --epoch 1717180000 \
      --dialect gdpr-json --user user --password <REDACTED>
"""

import argparse
import base64
import binascii
import json
import sys

import crack_login as cl


def split_http(payload: bytes):
    """Crude HTTP message splitter: (headers_text, body_bytes)."""
    if b"\r\n\r\n" in payload:
        head, _, body = payload.partition(b"\r\n\r\n")
    elif b"\n\n" in payload:
        head, _, body = payload.partition(b"\n\n")
    else:
        return None, payload
    return head.decode("latin1", "replace"), body


def field(body: bytes, name):
    import re
    pat = name if isinstance(name, bytes) else name.encode()
    m = re.search(pat + rb"=([A-Za-z0-9=+/]+)", body)
    return m.group(1).decode() if m else None


def decode_b64(s: str) -> bytes:
    return base64.b64decode(s)


def try_decrypt(key, iv, b64text: str):
    try:
        raw = decode_b64(b64text)
    except binascii.Error:
        return None
    try:
        from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
        c = Cipher(algorithms.AES(key), modes.CBC(iv))
        pt = c.decryptor().update(raw) + c.decryptor().finalize()
        pad = pt[-1]
        pt = pt[:-pad] if 0 < pad <= 16 else pt
        return pt.decode("utf-8", "replace")
    except Exception:
        return None


def collect_messages(pcap_path):
    try:
        from scapy.all import rdpcap, TCP
    except ImportError:
        print("[-] scapy required for pcap analysis. Install it or pass "
              "pre-extracted bodies.", file=sys.stderr)
        sys.exit(1)
    pkts = rdpcap(pcap_path)
    msgs = []
    for pkt in pkts:
        try:
            payload = bytes(pkt[TCP].payload)
        except Exception:
            continue
        if not payload or b"HTTP" not in payload and b"\r\n" not in payload:
            continue
        head, body = split_http(payload)
        if head is None:
            continue
        is_request = head.startswith(("POST", "GET", "PUT"))
        msgs.append((is_request, head.splitlines()[0] if head else "", body))
    return msgs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pcap", required=True)
    ap.add_argument("--epoch", type=int,
                    help="Unix epoch SECONDS of the captured login request")
    ap.add_argument("--dialect", choices=["gdpr-text", "gdpr-json"],
                    default="gdpr-text")
    ap.add_argument("--user")
    ap.add_argument("--password")
    ap.add_argument("--out", help="write full analysis as JSON to this path")
    args = ap.parse_args()

    msgs = collect_messages(args.pcap)

    parm = None
    login_sign = login_data = None
    ops = []  # (label, decrypted_request_or_response)

    for is_req, first_line, body in msgs:
        if "cgi/getGDPRParm" in first_line or b"cgi/getGDPRParm" in body:
            # response carries nn/ee/seq
            try:
                parm = json.loads(body.decode("utf-8", "replace"))
            except Exception:
                pass
        if "cgi_gdpr" in first_line:
            s, d = field(body, "sign"), field(body, "data")
            if s and d and is_req:
                if "cgi_gdpr?9" in first_line or "cgi_gdpr%3F9" in first_line:
                    login_sign, login_data = s, d
                else:
                    ops.append(("REQUEST", None, d))
        # JSON-ish responses (gl answers, login $.ret)
        if (not is_req) and body[:1] in (b"{", b"[", b"$"):
            txt = body.decode("utf-8", "replace").strip()
            if txt.startswith("{") or txt.startswith("[") or txt.startswith("$.ret"):
                ops.append(("RESPONSE", txt, None))

    analysis = {"gdpr_parm": parm, "login": {}, "operations": []}

    # Recover login key/IV if we have the epoch.
    key = iv = None
    if login_data and args.epoch:
        if args.dialect == "gdpr-json" and not (args.user and args.password):
            print("[!] gdpr-json needs --user/--password to crack the key.")
        res = cl.crack(login_data, args.epoch, args.dialect, args.user, args.password)
        if res:
            key, iv, pt = res
            analysis["login"] = {
                "aes_key": key.decode(), "aes_iv": iv.decode(),
                "sign": login_sign, "decrypted": pt.decode("utf-8", "replace"),
            }
            print(f"[+] Recovered login key/IV")
            print(f"    key={key.decode()}  iv={iv.decode()}")
        else:
            print("[-] Could not recover login key/IV (check --epoch/--dialect).")

    for kind, txt, data in ops:
        if kind == "REQUEST" and key is not None:
            dec = try_decrypt(key, iv, data)
            analysis["operations"].append({"kind": "request", "decrypted": dec})
            print(f"[req] {dec if dec else '(encrypted)'}")
        elif kind == "RESPONSE" and txt is not None:
            analysis["operations"].append({"kind": "response", "raw": txt})
            print(f"[resp] {txt[:200]}")

    if args.out:
        with open(args.out, "w") as f:
            json.dump(analysis, f, indent=2)
        print(f"[*] wrote analysis to {args.out}")


if __name__ == "__main__":
    main()
