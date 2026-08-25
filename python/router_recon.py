#!/usr/bin/env python3
"""Non-destructive reconnaissance of a TP-Link EX520v for shell/debug exposure.

Goal: confirm Milestone M1 (shell access) vectors WITHOUT modifying the router
and WITHOUT brute-force. Everything here is read-only:

  1. TCP connect-scan of well-known management ports (ssh/telnet/cwmp/...).
  2. Telnet banner grab if port 23 answers (banner only, no login attempt).
  3. Single TDDP GET probe on udp/1040 (TP-Link Device Debug Protocol).
  4. Confirm the GDPR API is alive (getGDPRParm -> RSA nn/ee + seq).
  5. Probe candidate debug/telnet/enable URLs and report status codes.
  6. Probe the config-backup download endpoint (read-only) to learn its path.

Nothing here authenticates via telnet, uploads a crafted backup, or writes to
the device. An open port or a 200/401 response is a LEAD to investigate
manually (e.g. via UART), not an exploit.

Usage:
  python3 router_recon.py --url http://192.168.0.1
"""

import argparse
import concurrent.futures
import socket
import struct
import sys

import requests

# Well-known management/service ports worth checking on consumer routers.
MANAGEMENT_PORTS = [
    (21, "ftp"),
    (22, "ssh"),
    (23, "telnet"),
    (53, "dns"),
    (80, "http"),
    (443, "https"),
    (1900, "upnp/ssdp"),
    (7547, "cwmp/tr-069"),
    (8080, "http-alt"),
    (8443, "https-alt"),
]

# Candidate debug/telnet/enable endpoints seen across TP-Link firmware families.
PROBE_PATHS = [
    "/cgi-bin/telnetd",
    "/cgi/telnet",
    "/cgi/set_telnet",
    "/cgi/get_telnet",
    "/cgi/telnetEnable",
    "/cgi-bin/telnetEnable",
    "/admin/telnet",
    "/telnet",
    "/cgi/debug",
    "/cgi/getDebug",
    "/cgi/getDebugInfo",
    "/cgi/api/debug",
    "/cgi-bin/diagnostic",
    "/cgi/diagnostic",
    "/cgi/enable_debug",
    "/cgi-bin/enable_debug",
    "/cgi/serial",
    "/cgi/getGDPRParm",          # known-good control
    "/cgi/getDebugCfg",
    "/cgi/api/setTelenet",
    "/cgi/api/telnet",
    "/cgi/sysinfo",
    "/cgi-bin/sysinfo.cgi",
    "/cgi-bin/luci/",            # OpenWrt-derived webui
    "/ubus",                     # OpenWrt ubus-over-http
]

# Candidate read-only config/backup download endpoints.
BACKUP_PATHS = [
    "/cgi/conf.bin",
    "/cgi/export",
    "/cgi/backup",
    "/cgi-bin/ExportSettings.cfg",
    "/backup/Config.bin",
    "/cgi-bin/config.bin",
]


def tcp_scan(host, ports, timeout=1.0):
    """Connect-scan a list of (port, label); returns {(port,label): bool}."""
    def probe(entry):
        port, label = entry
        try:
            with socket.create_connection((host, port), timeout=timeout):
                return entry, True
        except OSError:
            return entry, False

    results = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=len(ports)) as ex:
        for entry, ok in ex.map(probe, ports):
            results[entry] = ok
    return results


def telnet_banner(host, timeout=2.0):
    """Read-only banner grab on port 23. No credentials are ever sent."""
    try:
        with socket.create_connection((host, 23), timeout=timeout) as s:
            s.settimeout(timeout)
            return s.recv(256)
    except OSError:
        return None


def tddp_probe(host, timeout=2.0):
    """Send ONE read-only TDDP v1 GET (opCode=1, empty payload) to udp/1040.

    TDDP header: ver(1) opCode(1) status(2) md5digest(16) dataLen(2).
    We only listen for a reply; we never send SET/command opcodes.
    """
    hdr = struct.pack(">BBH", 1, 1, 0) + b"\x00" * 16 + struct.pack(">H", 0)
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.settimeout(timeout)
    try:
        s.sendto(hdr, (host, 1040))
        data, _ = s.recvfrom(2048)
        return data
    except OSError:
        return None
    finally:
        s.close()


def status(session, url):
    try:
        r = session.get(url, timeout=5, allow_redirects=False)
        return r.status_code
    except requests.RequestException as e:
        return f"ERR:{type(e).__name__}"


def main():
    ap = argparse.ArgumentParser(description="EX520v read-only shell/debug recon")
    ap.add_argument("--url", default="http://192.168.0.1")
    ap.add_argument("--user", default="user")
    ap.add_argument("--password", default="<REDACTED>")
    ap.add_argument("--dialect", default="gdpr-json")
    ap.add_argument("--ports", default=None,
                    help="comma-separated TCP ports to scan (default: built-in list)")
    args = ap.parse_args()

    base = args.url.rstrip("/")
    host = base.split("//", 1)[-1].split("/", 1)[0].split(":", 1)[0]

    if args.ports:
        ports = [(int(p.strip()), "") for p in args.ports.split(",") if p.strip()]
    else:
        ports = MANAGEMENT_PORTS

    sess = requests.Session()
    sess.verify = False
    sess.headers.update({
        "User-Agent": "Detectic-Recon",
        "Referer": f"{base}/",
    })
    try:
        sess.get(f"{base}/", timeout=5)
    except requests.RequestException:
        pass

    print(f"[*] target: {base} ({host})")

    # 1) TCP service scan
    print("[1] TCP management-port scan:")
    open_ports = []
    for (port, label), ok in sorted(tcp_scan(host, ports).items()):
        mark = "OPEN <-- LEAD" if ok else ""
        if ok:
            open_ports.append(port)
            print(f"    {port:<6} {label:<12} {mark}")
    if not open_ports:
        print("    (none open)")

    # 2) telnet banner (only if 23 answered)
    if 23 in open_ports:
        print("[2] telnet banner grab:")
        banner = telnet_banner(host)
        if banner:
            print(f"    {banner!r}  <-- LEAD (note model/firmware text)")
        else:
            print("    connected but silent (may wait for input)")
    else:
        print("[2] telnet banner grab: skipped (port 23 closed)")

    # 3) TDDP udp/1040 single GET
    print("[3] TDDP udp/1040 probe (single read-only GET):")
    resp = tddp_probe(host)
    if resp is not None:
        print(f"    replied {len(resp)} bytes: {resp[:64]!r}  <-- LEAD")
    else:
        print("    no reply")

    # 4) GDPR API sanity check
    try:
        r = sess.post(f"{base}/cgi/getGDPRParm", timeout=5,
                      headers={"Referer": f"{base}/"})
        ok = r.status_code == 200 and ("nn" in r.text and "ee" in r.text)
        print(f"[4] getGDPRParm -> HTTP {r.status_code} "
              f"{'RSA params present' if ok else 'no RSA params'}")
    except requests.RequestException as e:
        print(f"[4] getGDPRParm -> ERR:{type(e).__name__}")

    # 5) debug/telnet probes
    print("[5] debug/telnet endpoint probes:")
    leads = 0
    for p in PROBE_PATHS:
        code = status(sess, f"{base}{p}")
        if code in (404, "ERR:ConnectionError"):
            continue
        leads += 1
        print(f"    {code:<18} {p}  <-- LEAD")
    if not leads:
        print("    (no responses other than 404/unreachable)")

    # 6) backup download probes (read-only)
    print("[6] config-backup download probes (read-only):")
    leads = 0
    for p in BACKUP_PATHS:
        code = status(sess, f"{base}{p}")
        if code in (404, "ERR:ConnectionError"):
            continue
        leads += 1
        print(f"    {code:<18} {p}  <-- LEAD")
    if not leads:
        print("    (no responses other than 404/unreachable)")

    print("\n[*] done. LEAD = something responded (open port / non-404).")
    print("    Investigate leads manually; UART remains the reliable path.")
    print("    No writes were performed.")


if __name__ == "__main__":
    main()
